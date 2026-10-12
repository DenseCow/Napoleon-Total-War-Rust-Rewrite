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
`TickPeriod` (the speed's name, "pause" ... "ffwd", see "Speed ring" below); `Pause/Slow/Play/Fwd/Ffwd/CycleBattleSpeed` → model speed;
`Current_Selection_*` / formation / fire names → orders; `SelectAll{Infantry,Cavalry,Artillery}`;
`UnitListForBattleIds → {Land={}, Naval={}}` (no floating unit ids yet); `GetHealthStatus` (player share of men alive,
INFERRED); `LocalisationString`; `PostBattleInfo` (see §4); `UICardManager` (Lua class: AddCard, RemoveCard, Selected,
ManageSelection, SelectCardList, DeselectAll, PositionCards, ...); `MPAvatar` (no Steam: no-op);
`Component.LockPriority` returns the previous lock (PanelManager logs "Was: ..").
`WindowsTime` (`0x005D4BF0`, registered at `0x0040C320` with "Returns the current time in seconds", CONFIRMED): `timeGetTime()`
(winmm import `0x013073EC`, ms since Windows started) widened unsigned to double, rounded to float, times `0.001f`
(`0x01318030`), pushed as that float (`0x010565A0`): fractional seconds since Windows started at float precision. The
campaign's `CampaignUI.WindowsTime` (`0x009FB0B0`) is the same clock and arithmetic but truncates to whole seconds
(`CVTTSS2SI`) before pushing. Ours: both from `host::time_get_time_ms` (winmm `timeGetTime`); before 2026-10-09 the
battle one returned `os.clock()` (process CPU time). `ElapsedBattleTime` (`0x005D07D0`) reads a float through the
battle UI manager (`0x014A418C`, `0x005CB810`: `[[[+0xF8]+8]+0xB0]+0x24`), not traced further.

The battle bindings table (2026-10-09, CONFIRMED by enumeration): every BattleUI binding is a static-init stub that
pushes (description, name, function) and calls the registrar `0x0059EB40` with its own small static object in ECX
(218 calls; the battle ones `0x00408094`..`0x00410104`, then the advice table `0x00415344`.. and the radar's
`0x00418D74`..). The earlier "no Time string is referenced" was wrong: the names are referenced from these stubs.
Among them `Time` (stub `0x0040BCF4`, "Returns the current time in seconds") → `GetBattleUITimeSeconds` `0x005D3BB0`,
next to `TickPeriod` (`0x0040BCD4`) and `WindowsTime` (`0x0040C334`). It pushes the float at
`[[0x014A418C]+0xF8]+0x2821C`: the battle UI's frame time, which `0x005F4CF0` / `0x005F5530` (the battle handler's
vtable slots +0x34 / +0x38, thunks `0x005EE260` / `0x005EE270`) set each frame to `(float)ms * 0.001f` from the
main loop's frame-time struct {u32 total ms, delta ms, f32 total s, delta s}. The main loop
`ProcessMVCManagerFrameTick` `0x0048A650` builds it from its `+0xB8` counter: each frame adds the real elapsed time
(`0x010A1E10`: µs as a float × 0.001f, truncated to whole ms; the frame timer `0x014A0A08` restarts every frame, so
the fraction is dropped), capped at 300 ms (`0x0048A906`) unless the `frame_rate_test_fps` preference (`0x0149B8C0`)
fixes a 1000/fps step, times the `root_time` debug variable (`0x0149F5E8` `+0x5C`, default 1.0). Battle pause and
speed never reach it (the handler's time multiplier feeds a second counter, `+0xBC`). The same ms value is what the
battle root's update gets (`0x005C5BB0` → `0x00DB2AE0` → root vtable +0x30, the `OnUpdatePulse` time). So
`BattleUI.Time` is the UI pulse clock in seconds, as `CampaignUI.Time` is the campaign's. The shipped battle scripts
use it for UI timing (hud_rotate, land_hud_increase_file / _rank, land_hud_move_*, land_hud_special_ability_button,
lb_unit_id's morale bar, mp_speed_voting's pulse). Ours: `B.Time` reads the host's pulse clock (`Inner::ui_time_secs`,
shared with `CampaignUI.Time`), and the battle HUD's clock follows the rule above (`battle::hud::advance_ui_clock`);
before, `B.Time` returned the battle's elapsed time (frozen while paused) and the clock had no cap or truncation.
Not modelled: `frame_rate_test_fps` and `root_time`. Whether the campaign UI's counter (`0x00A0EB80`, manager
`+0x9FC` `+0x90`) is fed from the same main-loop struct is not traced.

**The battle HUD scripts' frame (CONFIRMED: static trace and debugger sitting 2026-10-09).** The campaign HUD's
scripts see the root at its layout size, 1280x960, at 1920x1080 (debugger sitting 2026-10-07,
`analysis/campaign/CAMPAIGN_UI.md` "Panel placement"); the battle HUD's do too. Static evidence:
- One root loader for every UI manager, CONFIRMED: the generic manager ctor `0x00DAFCD0` (callers: front end
  `0x004581B0`, battle `0x00596B00`, campaign `0x0098C2F0`) calls `LoadUIManagerRootLayout` `0x00DB21E0`
  ("data/UI/<folder>/<layout>", root built by `0x00DA6860` → the component reader `0x0101E270`, stored at the
  sub-object's `+0xAC`). The battle HUD's mode reload `LoadBattleHudLayoutForMode` `0x0060BAC0` ("Battle UI" /
  `layout` or `minimised_HUD`) goes `0x00DB2880` → `0x00DB2900` → the same `0x00DB21E0`. Battle and campaign pass the
  same trailing ctor arguments (1, 8; font / `ui.xml` set-up via `0x01023840` → `0x0102AF00`, not geometry); the front
  end passes (0, 0xC). Neither `0x00DB21E0` nor the battle ctor `0x00596B00` nor `0x0060BAC0` resizes the root at
  their own level (their callees are not all read).
- The layout-to-screen mapping is device-wide, CONFIRMED: position `0x011881C0` / size `0x011617C0` (device vtable
  +0x274 / +0x278) apply `ComputeUIScaleForScreen` `0x0114EB20` (min(1, W/1280, H/960)) and the component's anchor
  share of the screen to every DrawMode-0 draw; the only switch, device `+0x69C70` ("no UI scaling"), is written
  once, in the device ctor `0x01132530`. No per-HUD frame exists at the draw level, so the battle HUD is drawn
  through the same anchor mapping (and the same UI scale under 1280x960) as the campaign HUD.
- `UIComponent:Dimensions` (`0x01014B70`) returns the current state's own size (`[[comp+0xAC]+0x24]`, `+0x28`): no
  screen term. The battle root is `[[g_pBattleUIManager 0x014A418C]+0xC4]`: manager `+0x18` is the generic part
  (the root holder), and the root sits at holder `+0xAC` (`0x00596BDD` stores the manager in the global). (Earlier
  notes said `+0xE0`; the sitting found the root at `+0xC4`.)
Debugger sitting 2026-10-09 (the original at a 1920x1080 screen, custom land battle, deployment; user-confirmed):
M = `[0x014A418C]`; the root at M+0xC4 has the root class vtable `0x01393B6C`; S = `[root+0xAC]`; the floats at
S+0x24 / S+0x28 are 1280.0 / 960.0. So the battle root keeps its layout size and the scripts work in that 1280x960
frame, as in the campaign. Ours: the battle host uses `UiFrame::ScriptFrame` (`ui::battle::install`), like the
campaign HUD (before, `UiFrame::Panels` gave the scripts screen geometry; the two agreed only at a 1280x960
window).
The UI scale (2026-10-09) is the device's, not the frame's: the device applies it to every draw (the bullet above),
and its inverse `0x0119B270` (device vtable +0x270) maps screen points back (`Component.CursorPosition`
`0x01018590` returns the virtual screen, window / scale, as its screen size). So the battle HUD uses the campaign's
mapping (`frontend::render::ui_virtual_screen` / `window_to_ui` / `ui_rect_to_window`): laid out in window / scale,
drawn scaled, the mouse mapped back.

Cards: the engine calls review_DY.lua `CreateCards(list, state)` with `{CardID="card_<unit id>", Portrait}` per player
unit (Portrait = `data/ui/units/icons/<faction unit_icon_path>_<key>_icon`, the script adds `.tga`; CONFIRMED file
names, prefix rule INFERRED), then each card's `SetInitialState` (arguments below) and every
frame `Update(info, time)` with `{Men, NumMen, Guns, NumGuns, IsArtillery, HasAmmo, AmmoRemainingAsPercent, Experience,
RoutingState, WaveringState, WalkingState, RunningState, FiringState, MeleeState, UnderFireState, Inactive, ...}`
(field names CONFIRMED in template.battleunitcard.lua). The `time` (traced 2026-10-10, CONFIRMED): the battle UI's
frame update `0x005E2AD0` calls `0x0058E3D0` (land; `0x0058E280` naval), which calls review_DY.lua
`UpdateCards(units, time)` with 2 arguments: `units` = one table per unit built by `0x005ABF40` (land: CardID,
Inactive, the nine *State flags, Garrisoned, Hidden, IsArtillery + Guns for artillery, Experience, Men, HasAmmo,
AmmoRemainingAsPercent, UniformColour {r,g,b}) or `0x005ABBD0` (naval: IsNaval, HullStrength, HullHealth_l/_r,
SailHealth, SailDamage, Repairing, ...), and `time` = the frame struct's third field pushed as a float
(`0x00587D50`): `ProcessMVCManagerFrameTick`'s `+0xB8` total ms × 0.001f, the value `0x005F5530` stores as
`BattleUI.Time`. UpdateCards (source line 227) forwards both to each card's `Update`; its `UpdateStateIcon(time)`
(line 152) cycles the state icons once per `delaySeconds` (1) by comparing `nextShowTime <= time` (line 163). Ours
passed no time, so any unit with a state raised "compare number with nil" every frame; fixed (`ui_battle::update_cards`
passes `ui_time_secs`, set before the cards as `0x005F5530` does; test
`a_firing_units_card_updates_without_script_errors`). Not yet compared: our info table's extra NumMen/NumGuns and
missing UniformColour against the exe's (BACKLOG §7 Battle UI). `Update(info)` never keeps
its argument: it reads the fields and copies the ones it compares into its own `previous_details` upvalue (CONFIRMED,
bytecode of `Update`, source line 297: only GETTABLE on the argument), so we make one info table per card and refresh it
in place. `SetInitialState`'s first parameter is the card's naval flag (CONFIRMED, bytecode at source line 55: it is
stored as `m_naval` and given to `ship_damage:PropagateVisibility`); our prelude passes the info table there (BACKLOG
§7 Battle UI: trace what the engine passes). Selected cards get the
template's `Selected` state by the card manager's rules (CONFIRMED, see "Speed ring and selected cards" below). A click on a card selects its unit (shift adds; the model holds one
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
`set_frame(UiFrame::ScriptFrame)` for the battle (its panels keep their size and dock themselves; the scripts work
in the root's 1280x960 frame, §3). Battle-only (prelude): the root's
InitState runs after its children's; image lists `{id:n}path` are applied.

**Where CreateFromLayout puts a layout (traced 2026-10-09, CONFIRMED).** `Component.CreateFromLayout(path, id,
parent[, x, y][, images])` is `0x01016BD0` (reads its Lua arguments from the top of the stack; numbers there are an
explicit x, y) → `CreateUIComponentFromLayoutFile` `0x01027400` → `0x01026F90` (instantiates the template under the
parent with an offset). The template: on a cache miss the file is read into a holder through the UI manager's
+0x24 callback (`0x00DA92D0`, set by the manager ctor `0x01022A50` from `0x00DA6A60`: allocates 0x304 bytes and runs
`0x00DA6860` → the component reader `0x0101E270`, which reads one component record and builds its children through
the same callback), so the holder is the file's own root; the template is the holder's FIRST child
(`[[holder+0x90]]`), detached by `0x01027BA0` (`+0x8C` count, `+0x90` children, its parent `+0x80` cleared) and
cached (`0x01039610`), and the holder is destroyed. So only that first child is created, renamed to `id`; a file's
other top children never exist (land_battle_ordersOLD). Without x, y (branch `0x010275F0`) the offset is the
template's absolute position minus the parent's, both from `GetUIComponentAbsolutePosition` `0x0102FEA0` (vtable
+0x14: parent's absolute position + own offset `+0x94` / `+0x98`, no screen or dock term), truncated to int; the
detached template has no parent, so the new component's absolute frame position is its own file offset. With x, y
they go in that offset slot directly. (`MoveTo` is `0x010140B0` → vtable +0x10 `0x0102D780` → +0xC
`SetUIComponentAbsolutePosition` `0x0102D710`: offset = target − parent's absolute position.) On screen,
`GetUIComponentDrawAnchor` `0x0102DAD0` (vtable +0x40, with the device mapping) recurses up to the root's child and
uses that ancestor's docking (`+0xDC`): it subtracts the dock share of the root state's size (1280x960) and returns
the same share of the screen as the anchor; our `UiWorld::layout` gives every subtree its top-level ancestor's dock
shift, the same rule.
So battle_hud.lua's `CreateFromLayout(".../land_battle_orders", "orders", Address, images)` under `veneer_DY`
creates the file's first panel (land_battle_orders) as "orders" at its file offset (3, 749) in the frame, drawn with
`veneer_DY`'s shift (dock 8: ((W − 1280) / 2, H − 960) at scale 1), so its bottom sits on the cards' top (854 at
1280x960, 974 at 1920x1080). Ours (`host::create_layout`, one rule for every CreateFromLayout): the file root's first
child, offset = file offset − the parent's absolute frame position, or the given x, y; test
`the_orders_bar_is_the_files_first_panel_at_its_own_position`. Before 2026-10-09 ours created the file root as a
wrapper when it had two or more children, and the battle prelude moved it to (0, 0), hid the second panel and
forwarded calls to the first; a single child was placed at its file offset relative to the parent.

**Speed ring and selected cards (traced 2026-10-12, CONFIRMED; user check 2026-10-10 found both broken).**
- *Speed ring.* `BattleUI.TickPeriod` (`0x005D3AA0`) returns a string, not the multiplier: "pause", "slow",
  "play", "fwd", "ffwd" for 0, 0.4, 1, 2, 4, and nil for any other speed. root.lua uses these strings (line 737's
  pause toggle, line 790's `pre_pause_speed`, line 810 `ChangeDisplayFor(period)`, which sets the matching button
  "selected" and the rest "normal"). The speed buttons only ask for a speed; the ring moves when the engine grants a
  time request and calls root.lua `TimeRequestChanged(is_mp, request)` (`0x005E79A0`, a vtable method; skipped when
  handler +0x281F0 is set (replays); `is_mp` = handler +0x281E4 != 0). The request table is built by `0x005ABB60`:
  `alliance_index`, `army_index`, `player_multiplier`, `overall_multiplier`. In single player, line 838 then calls
  `ChangeDisplayFor(TickPeriod())`. Before: our TickPeriod returned the number and nothing called
  `TimeRequestChanged`, so the ring stayed on "play". Ours: `battle_prelude.lua` TickPeriod (the five strings,
  1e-6 tolerance); `ui::battle::time_request_changed` (the player's alliance and army from the setup, both
  multipliers = the battle's; `is_mp` false, PROVISIONAL until multiplayer battles), called by `battle::hud::frame`
  once for the time requests granted since the last frame (`BattleSim::set_speed`, the speed's only writer; never at
  battle start), and only after the facts were written. Test: `the_speed_ring_follows_the_battle_speed`.
- *Card selection.* The card manager's select (`SelectCardList` `0x01019000`) sets a card "Selected" through
  `SetState` (`0x010358B0` → `0x01035B30`), unless its state is "UnSelectable" or "Routing", or its current state
  is not interactive (+0x114) or is disabled (+0x115), or the card is already in the list. Deselect (`0x010245D0`)
  sets "Default" from any state but "UnSelectable" and "Routing" (no other check). Both run only when a card joins
  or leaves the selection, never while it stays: a selected unit that routs and rallies keeps the state its card's
  Update gave it. So a card under the pointer ("roll") becomes "Selected" too. Before: ours only moved Default →
  Selected. Ours: `__ntw_battle_update_card` applies these rules when the unit's selected flag changes
  (`card_selected` stands in for the manager's list), with `__CurrentStateSelectable` for the interactive and
  disabled check. Test: `a_rallied_selected_card_is_not_reselected_and_deselect_goes_default_from_roll`.
- *The highlight is a per-state pixel shader.* BattleUnitCard's states differ only by shader: Default `brighten_t0`
  -0.3, Selected `brighten_t0` +1.0, roll `normal_t0`, UnSelectable `set_greyscale_t0` (1, 1), Routing `red_pulse_t0`
  0. All five states are interactive. The transitions are Default Enter → roll and roll Leave → Default; Selected has
  none. The technique is state +0x118 and its vars x..w are +0x11C..+0x128. The text copy is +0x12C..+0x13C.
  - `ShaderTechniqueGet` (`0x01014E40`) and `ShaderVarsGet` (`0x01014BD0`) read the current state (component +0xAC).
  - `ShaderTechniqueSet(name, all_states, text_too)` (`0x01014D50`) and `ShaderVarsSet(x, y, z, w, all_states,
    text_too)` (`0x01014C50`) write the current state, or every state (`0x010360F0` / `0x010361D0`). They also write
    the text copy (`0x01036B70` / `0x01036C60`) unless text_too is false.
  - ShaderVarsSet takes its booleans off the top of the Lua stack, then pops numbers into w, z, y, x until the stack
    is empty. A nil skips its slot. Any other non-number (a numeric string too) is a script error "Trying to retrieve
    type number, found type <t>" (`0x01055700` checks lua_type == number, `0x01095090` raises); the slots popped
    before it stay written. Test: `shader_vars_set_refuses_a_string_with_a_script_error`.
  - `Component.ShaderVarsGet()` (`0x01018280`) reads the running component's current state.

  The card script caches every state's pair at creation and puts it back after its burning or sinking effect, which
  sets `unitcard_burning_t0` on all states. `sprite.fx` in data.pack (behaviour, not code):
  - `brighten_t0`: RGB × (1 + x), clamped.
  - `set_greyscale_t0`: lerp to luminance by x, alpha × y.
  - `red_pulse_t0`: lerp red toward 1 by x.

  Before: the script shader methods were Lua stubs on the component, and the renderer drew no shader. Ours: the
  methods are host methods on the current state or all states (tests `shader_methods_read_and_write_the_current_state_or_all_states`,
  `shader_vars_set_fills_from_w_back`). `frontend::render::shaded_color` draws `brighten_t0` as an sRGB colour
  multiply (INFERRED: equal to the original's gamma-space product for a power-law curve). Every other technique is
  drawn as `normal_t0`, logged once (PROVISIONAL, needs a UI material); x < -1 gives black, never a negative colour.
  Text shaders are stored per component, not per state, and not drawn (PROVISIONAL, ui_prelude.lua). Test:
  `a_selected_units_card_lights_up_even_under_the_pointer`.

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
  drag/grouping; multi-selection; `UIComponent:DockingPoint`; UI shader techniques other than `brighten_t0` / `normal_t0` (greyscale,
  red pulse, burning) and text shaders are not drawn (§3).
- Flags: `FlagPath` = `data/` + the factions flag folder; the republic factions (ita_/egy_french_republic) show the
  checkered placeholder (their folder rule UNKNOWN).
- Return to the campaign via `CampaignModel::pending_battle` / `apply_battle_result`.

## Open questions for Ghidra (battle UI registrar 0x59EB40 and the battle mode)
- When the engine calls root.lua's ShowDeploymentPopup / SetDeploymentPopupAsDeploymentStart / ClearDeploymentPanels /
  ShowSinglePlayerEndPhasePopup / ShowBattleSummaryPopup / ShowPostBattleResultsPopup, and with which text.
- The order buttons' enable rules per unit type (SetOrderButtonState callers) and the unit-card info table
  (`SquadInfoByPointer`, card manager `PositionCards`, `Selected`).
- InitState order (children before parent?); `Find` search scope; `DockingPoint` return values.
- Victory: the exact "kill or rout" test (routing units leaving the map, rallying), the time unit of `duration`, the
  victory grading (close/decisive/heroic/pyrrhic) thresholds.
- Deployment placement rules (formation drag, spacing) and how the human army is chosen in a battle file.

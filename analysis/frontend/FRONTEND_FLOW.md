# Front-end flow (how the original builds the main menu)

All addresses are Napoleon.exe (Steam 1.3). Decompiled code is not stored; this is the spec.

## Loading
- `0x00405610` (static init): global string `"FrontEnd UI"` at `0x0149EC78` (CONFIRMED).
- `0x004581B0` (front-end UI object ctor) calls `0x00DAFCD0(..., "FrontEnd UI", ...)`, which leads to `0x00DB21E0`:
  it opens `data/UI/<folder>/<name>` through the `uif()` file factory and builds the component tree with the
  layout reader `0x00DA6860` → `0x0101E270`; the skins folder is `data/UI/<folder>/Skins/` (CONFIRMED).
  The first layout is `frontend ui/layout` (root, `layout`, `background`, `movie_bg`, `top_bar`, `bottom_bar`).
- After loading each component the engine finds its Lua: `template.` prefix, `<layout>_scripts/<id>.lua`,
  `data/ui/templates/%S`, `%S/%s.lua` (strings `0x013F22E4..0x013F2344`). The front-end scripts are
  `ui\frontend ui\<layout>.<component>.luac` or `ui\frontend ui\<layout>_scripts\<component>.luac` (CONFIRMED file names).

## Scripted page flow (from the scripts' string constants; INFERRED order)
- `layout_scripts/root.lua`: `package.path = ";?.lua;data/ui/templates/?.lua;data/ui/?.lua"`, `require "Utilities"`.
  Keeps `m_current_layout` and a `Previous` list. `TransitionTo(name)` = `UIComponent(...):CreateFromLayout("data/ui/frontend ui/" .. name)`,
  `Adopt`, `OnEnter`; `TransitionBack` pops `Previous`. First page: `main` (or `spain_main` if `FrontEnd.SpanishCampaignEnabled()`;
  the `spain_main` layout is in data.pack, corrected 2026-10-03). Also: `ShowMessageBox` (`message_box` layout), `password_entry`, `CentreMenu`,
  `ScreenSize`, ESCAPE → `TransitionBack`, `StartDefaultCampaign/Land/Naval/Fort`.
- `main.main.lua`: `InitState` → `ShowButtons("default")`; groups `default`/`singleplayer`/`multiplayer`/`options` map to
  `button_group_default` / `single_player_expanded` / `multiplayer_expanded` / `options_expanded`.
  Version text `version_number` ← `FrontEnd.GameVersion()`; `advert`/`button_steam_store` depend on `FrontEnd.SteamNewContentAvailable()`;
  `continue_campaign` state depends on `FrontEnd.CampaignSavesExist()`; buttons set `g_click_callback` (template
  `template.fe_button_standard.lua`) and call `TransitionTo("sp_load_game" | "napoleon_battles" | "grand_campaign" | "options" | "credits" | ...)`.

## What NapoleonRust does today (`crates/napoleon/src/frontend/`)
- `GameMode` Bevy state (`FrontEnd` default, `CampaignLoad`, `Campaign`, `Battle`). `--battle` starts the battle slice (test harness).
- Loads `frontend ui/layout` into `ntw_script::ui::UiScriptHost`, which runs the original scripts: root.lua `InitState` →
  `TransitionTo("main")`, main.main.lua `ShowButtons("default")`, button templates. Mouse input drives the transition maps and the
  Lua click handlers (Single Player / Multiplayer / Options expand their groups; Quit quits).
- Draws every visible component's current-state image metrics (TGA/DDS from the install) and texts (`.cuf` fonts).
- PROVISIONAL: page roots adopted under the root are sized to the window; `GameVersion` = "1.3.0"; docking rule.
- Background movie `Frontend2.bik` in `movie_bg`: behind `--frontend-movie` until checked (default: the layout's still image, PLACEHOLDER; BINK.md §8). Not done: tooltips, keyboard, all other pages'
  engine functions (they log UNKNOWN), sounds.
- Test harnesses: `--screenshot <file.png>` (save after 4 s and quit), `--ui-click id1,id2` (click visible components, 1 per second).

## Pages (2026-10-03)
See `FRONTEND_PAGES.md`: page stack and Back (ESCAPE), Single Player pages (load game, Napoleon's campaigns, Napoleon's battles, Campaigns of the Coalition), Options, and the engine rules found while running them.

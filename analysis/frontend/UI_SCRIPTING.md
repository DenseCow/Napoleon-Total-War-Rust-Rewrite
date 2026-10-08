# UI scripting: how the original runs UI Lua, and what `ntw_script::ui` implements

Code: `crates/ntw_script/src/ui/` (`world.rs` live tree, `host.rs` + `ui_prelude.lua` bindings).
Real-install test: `cargo test -p ntw_script --test frontend_ui -- --nocapture` (runs the original root.lua,
main.main.lua and the button templates; checks the main page, `ShowButtons` and a Single Player click).
Bytecode lister for reading the scripts: `cargo run -p ntw_script --release --example luac_dis -- "ui/frontend ui/main.main.luac"`.

Evidence below comes from the scripts' own bytecode (listed with `luac_dis`) and the layout data. Tags: CONFIRMED (seen
directly), INFERRED (deduced, consistent with all data seen), PROVISIONAL (our stand-in).

## Engine model
| Rule | Evidence | Tag |
|---|---|---|
| Each component has its own global environment; globals one script sets are invisible to the others | `UIComponent(x):SetGlobal("g_click_callback", f)` in main.main.lua sets a different callback in every button; `GlobalExists("OnExit")` returns the function from another component | CONFIRMED (usage) |
| Global `Address` = the component's own address; `Component.*` acts on the running component | `UIComponent(Address)`, `Component.Adopt(x)`, `Component.Root()`, `Component.CreateFromLayout(path, id, Address)` | CONFIRMED (usage) |
| `UIComponent(addr)` wraps an address; `obj:Find("id")` / `obj:Find(0)` return addresses (by id, or n-th child) | root.lua, template.fe_button_standard.lua | CONFIRMED |
| Scripts of a component: inline `script` text from the layout, then `<folder>/<layout>_scripts/<id>.lua` or `<folder>/<layout>.<id>.lua` (as `.luac`), or `ui/templates/<ScriptFileNameOverride>` | file names in data.pack; exe strings `_scripts/`, `template.`, `%S/%s.lua` | names CONFIRMED, order INFERRED |
| `dofile` inside a component script runs in that component's environment | inline scripts `dofile "data/ui/templates/template.fe_button_standard.lua"` then rely on its `Initialise`/`InitState` | INFERRED |
| `InitState{State=name}` is called after the layout is built and on every state change | template.fe_button_standard.lua's InitState greys text when `State == "inactive"`, shifts it 1 px when `"down"`; root.lua's InitState starts the first page | INFERRED |
| State changes from the mouse follow each state's transition map: key 0 mouse enter, 1 leave, 2 left down, 3 left up, 11 left up elsewhere; value = target state `this` | all front-end buttons (normal/roll/down/mouse_off and up/roll/down/down_off sets) | data CONFIRMED, key meaning INFERRED |
| Mouse events call the layout-bound function (`OnMouseLClickUp → OnLeftClickUp`) or a `SetEventCallback` callback | layouts + main.main.lua `SetEventCallback("OnMouseLClickUp", OpenSteamStore)` | INFERRED dispatch |
| Event bindings may be a call on another component: `call Root.LuaCall, Quit`, `call Parent.LuaCall,PlayBattle`, `call Root.LuaCall,TransitionBack` (target Root/Parent, method, comma-separated string args) | strings in the front-end layouts; the main menu Quit button uses it | strings CONFIRMED, meaning INFERRED (Quit works) |
| `CreateFromLayout(path, id, parent)` builds a layout under `parent` and renames its root to `id` | root.lua `TransitionTo` later calls `:Id()` on it and stores it for `TransitionBack` | INFERRED |
| State text labels are localised at load | state reader `0x01021410` looks up the label (CONFIRMED in exe) | CONFIRMED |

## Implemented bindings
- `UIComponent` methods: Address, Id, Parent, ChildCount, Find, SetVisible, Visible, CurrentState, SetState, Position, Dimensions,
  MoveTo, Resize, GetStateText, SetStateText, Get/SetStateTextDetails (colour), GetProperty, Adopt, Divorce, DestroyChildren,
  SetGlobal, GlobalExists, LuaCall, SetEventCallback. Any other method logs `UNKNOWN UIComponent:<name>` and returns nil.
- `Component.*`: Address, Root, Adopt, Destroy, CreateFromLayout, LockPriority (no effect). Others log UNKNOWN.
- `FrontEnd.*`: ScreenSize, CampaignSavesExist (looks for `%APPDATA%\The Creative Assembly\Napoleon\save_games\*.save`),
  SpanishCampaignEnabled (false, PROVISIONAL), GameVersion ("1.3.0", PROVISIONAL), SteamNewContentAvailable (false: no Steam),
  MPHasGameInvite (false), PreviousGameType (-1), Quit, LocalisationString, UILocalisationString. The other 100 log UNKNOWN.
- `Cursor(name)` → object whose `SetMode` logs UNKNOWN; `system.ClearRequiredFiles` no-op; `defined = {}` (retail: `defined.demo` nil);
  `out.*` and `print` go to the host log; `require` reads from the install (same rules as the campaign host).

## Known gaps (UNKNOWN / next steps)
- Seen as UNKNOWN on the main menu: `SetImageColour`, `SetImageMetrics` (root.lua's black full-screen backdrop), `StealShortcutKey`,
  `StealInputFocus`, `Cursor.SetMode`.
- Every other front-end page needs many more `FrontEnd.*` functions (campaign lists, save enumeration, battle setup, options).
- Tooltips (`RegisterTooltipObject`, `CreateComponentFromTemplate`), keyboard (`OnKey`, `OnShortcut`), priorities/focus.
- Whether the engine calls `InitState` before or after a page is adopted, and in which order across components.

## Update 2026-10-03
Many more bindings now exist (keys, pulses, properties, Component.Call paths, CreateFromComponent/Template, Layout, UISelectionManager, UIImage, UIPrefsInterface, battle setups). The rules and their tags are listed in `FRONTEND_PAGES.md`; the gaps above about SetImageColour/SetImageMetrics/StealInputFocus/Cursor are filled.

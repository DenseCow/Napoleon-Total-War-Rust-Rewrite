# Handoff from Worker 2 to Worker 3

Worker 2 did only a quick survey of the areas Worker 3 now owns. Nothing below was analysed in depth.

## Loose files (CONFIRMED, from `find` on the install)
- `data\campaigns\<campaign>\{scripting.lua, startpos.esf}` for 8 campaigns: egy_napoleon, eur_napoleon, ita_napoleon, spa_napoleon, tut_napoleon, mp_egy_napoleon, mp_eur_napoleon, mp_ita_napoleon.
- `data\campaign_maps\nap_{egypt,europe,italy,spain,tut}\`: `regions.esf, pathfinding.esf, poi.esf, sea_grids.esf, trade_routes.esf, metadata.dat, <name>_lookup.tga, <name>_map.tga, stratradar_*.tga`, plus `display\` (borders/roads/rivers *.rigid_spline, heightmap.tga, supertexture .stpd/.stpi, *.lighting, world.markers, bridge.markers, coastline *.rigid_mesh, trees *.rigid_trees). Totals over campaigns+campaign_maps+UI: 1417 files (903 rigid_spline, 178 cur, 176 tga, 45 dds, 33 esf, 20 lighting, 10 markers, 8 lua, ...).
- `data\all_scripted.lua` (396 B, Lua source): `require "data.export_triggers"`, `"data.export_ancillaries"`, `"data.export_historic_characters"`, `"data.export_missions"`; sets global `events = triggers.events`.
- `data\battle_scripted.lua` (116 B, source): `require "data.all_scripted"`; `require "data.export_advice"`.
- `data\language.txt` = ASCII "EN" (2 bytes).
- `data\UI\`: `Campaign UI\Pips\*.tga`, `Cursors\` (.cur/.ani), `Templates\`.

## Lua inside data.pack (CONFIRMED from the pack index, see `pack_indexes\data.pack.txt`)
- 497 `.luac` entries plus 10 `.lua` at the pack root: episodicscripting.lua, events.lua, export_advice.lua, export_ancillaries.lua, export_historic_characters.lua, export_historic_events.lua, export_missions.lua, export_triggers.lua, scripting_library.lua, scripting_library_wellington.lua. Also 24 `.battle_script`, `profiling_scripts\` (66).
- Not checked: the luac version byte or which entries are bytecode.

## Tools you can use (read-only)
- `data_tools\target\release\data_tools.exe ls <pack> [substr]`, `hex <pack> <path> [n] [skip]`, `cat <pack> <path> [n] [skip]`, `grep <pack> <path-substr> <string>`. See `data_tools\README.md`.
- Full pack indexes are in `pack_indexes\<pack>.txt` (size, absolute offset, path).
- An earlier Python helper, `pack_index.py`, does the same index parsing.

ESF was not decoded at all by Worker 2.

# Battle terrain (battleterrain.pack) — structure notes

Tags: **CONFIRMED** = checked in the bytes (on every file where a count is given); **INFERRED** = strong reading; **UNKNOWN**.
Tools (Rust, read-only, nothing extracted to disk): `analysis/worker5/terrain_tools` (`inventory`, `ls`, `hex`, `esf`, new
`l16`, `l16-all`) and `analysis/worker2/data_tools` (`ls`, `hex`, `cat`, `u32s`, `dds-survey`, `db`).
Structure only. No map data is copied here.

## 1. Inventory (CONFIRMED, pack index)
battleterrain.pack: PFH0 type 1 (release), 3,627 files, 2.26 GB.

| extension | files | what it is |
|---|---|---|
| .dds | 773 | heightfields (L16, 623), texture sheets (DXT5 107, DXT1 35, A8R8G8B8 8) |
| .tga | 764 | colour/ground-type/grass/sound maps, radar images |
| .jpg | 729 | per-preset colour maps and blend maps (+ `_alpha` companions) |
| .xml | 697 | preset definitions and small settings (§3) |
| .building_list | 147 | ESF: placed buildings and props (§4) |
| .farm_fields_tile_texture | 131 | custom binary (§6, UNKNOWN) |
| .settings | 109 | ESF `HEIGHTFIELD_SETTINGS` for tiles (§2) |
| .tree_list | 95 | ESF: placed trees/shrubs (§4) |
| .markers | 54 | custom binary marker list (§6) |
| .farm_manager / .farm_template_tile / .prop_list | 33 / 32 / 20 | ESF (§4) |
| .environment | 25 | XML `<SCENE>` lighting/fog/sky |
| .tai | 15 | text: texture-atlas index from "AtlasCreationTool.exe" (grass atlases) |
| .rigid_spline | 1 | `walls\wall01_xs.rigid_spline`, magic "SPLN" (see worker3 CAMPAIGN_MAP_GRAPHICS.md §4) |
| .idx / .cmd | 1 / 1 | `tiles\metadata.idx` (tile catalogue, §5); a leftover `texconv` batch file |

Top folders: `presets\<map>` (≈60 maps: historical `hb_*`, multiplayer `nap_mp_*`, forts `rti_fort_*`, `*_artillery_fort`, …),
`tiles\generic\<kind>\<tile>` and `tiles\lc_desert\…` (tile library), shared texture folders (`blend_maps`, `cliff_maps`,
`colour_maps`, `detail_maps`, `rock_maps`, `tiled_maps`, `grass`, `grass_maps`, `forest_underlays`, `groundcover_*`,
`farm_templates`, `walls\textures`, `radar`, `river`, `decals`, `surfaces`).

data.pack also holds `battleterrain\templates\<type>\<name>\{attribute,deployment,terrain}.tga` (507 tga, 39 thumbs.db)
— INFERRED: small layout masks for generated (non-preset) battlefields — and the DB tables `battle_terrain_sets` (81 rows:
climate/season → colour, blend, grass, cliff, rock, tiled and detail map paths), `battle_terrain_set_climates_jcts`,
`battle_terrain_set_groupings`, `battle_terrain_farms`, `battle_terrain_farm_walls`, and the shaders `fx\terrain*.fx`.

## 2. Heightfields — CONFIRMED format, INFERRED scaling
There is **no "HF" magic file** in battleterrain.pack (first-4-byte histogram over all 3,627 files). Heightfields are plain
DDS files with an uncompressed **16-bit luminance** pixel format (`DDPF_LUMINANCE`, 16 bits, mask 0xFFFF, no mips; file size
exactly 128 + w·h·2), plus a settings file next to them.

Preset maps have **four nested heightfields**, `height_map_0..3.dds`, each 1025×1025 (57 presets × 4 = 228 files), with
`height_map_N_settings.xml`:
```xml
<HEIGHTFIELD_SETTINGS world_width='…' world_height='…' normalize='true' simple_colour_blending='false' scale='…' bias='…' />
```
World sizes in the example preset are 2,048 / 4,096 / 8,192 / 16,384 m: each ring covers twice the area of the previous one at
the same 1025² resolution (INFERRED: the playable area is level 0, the rest is the distant landscape). 1025 = 2¹⁰+1 samples,
so samples sit on the cell corners (INFERRED).

Tiles (`tiles\…\height_map_0.dds`, 384×384, 512×384, 768×768, 1280×1280, …; tile height + alpha maps together are the other 395 L16 files) use an **ESF** settings file
`height_map_0.settings`, root record `HEIGHTFIELD_SETTINGS` v2 with 6 values (CONFIRMED order): f32, f32, bool, f32, f32, bool
— INFERRED to be world_width, world_height, normalize, scale, bias, simple_colour_blending (the XML attributes, with the
same values pattern: e.g. 256, 256, false, 100, -40, false). Tiles also carry `alpha_map_0.dds` (L16, same size as the height map;
INFERRED blend weight when the tile is stamped into a map).

`normalize='true'` and `l16-all`: 322 of 623 L16 files use the full 0..65535 range exactly (CONFIRMED). INFERRED height formula:
`height_m = sample / 65535 * scale + bias`. The exact formula (and whether it is `/65535` or `/65536`) is UNKNOWN until the
exe's heightfield loader is read.

## 3. A preset folder (CONFIRMED file set; example `presets\hb_austerlitz`, 38 files)
| file | format | content (structure) |
|---|---|---|
| definition.xml | XML `BATTLE_MAP_DEFINITION` | bmd_type, additive_type, terrain_type, climate (e.g. lc_tundra), season, base_terrain_width/height (2048), shrub/tree density, subculture |
| textures.xml | XML `BMD_TEXTURES` | grass_type, tiled_detail_map, cliff_map, rock_map, forest_underlay, 4× `DETAIL_MAP id name` — all paths into the shared folders |
| weather.xml | XML `WEATHER` | has_ambient_fog, heat/cold fatigue, max_weather_type_key, environment_key → `*.environment` |
| *.environment | XML `SCENE` → `LIGHTING` … | light direction (euler), colours, fog, sky |
| deployment_areas.xml | XML `BATTLE_DEPLOYMENT_AREA_HASH_TABLE` | per player-count setup: `ALLIANCE id` → `deployment_area` with centre x/y, width, height (metres), orientation (radians) |
| zones.xml, capture_location_list.xml, ef_lines.xml, non_terrain_outlines.xml, sound_emitter_list.xml | XML | zone templates, capture points, lines, outlines, sound emitters (often empty) |
| height_map_0..3.dds + _settings.xml | §2 | terrain height |
| colour_map_0..3.jpg + `_alpha.jpg` | JPEG | ground colour per heightfield level (RGB + separate alpha JPEG) |
| blendmap.jpg + blendmap_alpha.jpg | JPEG | texture blend weights |
| ground_type_map_0.tga | TGA | ground type per cell (INFERRED: feeds `unit_movement_modifiers` ground types: road, grass, mud, forest…) |
| grassmap.tga | TGA 4 MB | grass density |
| loading_screen_radar.tga, screenshot_small.tga | TGA | UI images |
| bmd.tree_list, bmd_near_buildings.building_list, bmd_far_buildings.building_list, farms.farm_manager, bmd.markers | ESF / binary | §4, §6 |

## 4. Object lists are ESF (CONFIRMED: magic 0xABCE, parsed by `ntw_formats::esf`)
- `.tree_list`: root `TREE_LOD_LIST` v1 → U32 (3) + several `TREE_LIST` v3 records. A TREE_LIST is a *flat* child list:
  `Bool`, `U32`, then repeated groups { `Utf16String` species key (e.g. "lc_tundra-lowbrush_shrub"), `U32` count,
  count × (`Coord2d` x,y, `U8`, `I32`) } (INFERRED grouping: U8 = scale/variation, I32 = flags). One list has 354,904 children.
- `.building_list`: root `BATTLEFIELD_BUILDING_LIST` v2 → record array `BATTLEFIELD_BUILDING_LIST_BLOCK` v0, each item
  { `Utf16String` model key ("reeds_1"), `Coord2d` position, `Angle` (u16 angle), `F32` scale }.
- `.farm_manager`: root `ROOT_FARM_MANAGER` v0 → `FARM_MANAGER` v4 (2 Coord2d + 7 U32 in the example; empty farms).
- `.farm_template_tile`, `.prop_list`, tile `height_map_0.settings`: ESF too (roots not all walked yet).
Object keys are INFERRED to resolve to models via `battlefield_buildings` (DB) and the rigid_model files in buildings.pack /
rigidmodels.pack (graphics worker).

## 5. Tile library
`tiles\generic\<kind>\<NxM_xx>\` with kinds beach, coast, hills_1..3, lakes_ponds, mountains, plains, rivers_* (bend, bridge, ford,
kink, mouth, start, straight, tjunction), road_* (bend, bridge, crossroads, straight, tjunction), sea; plus `tiles\lc_desert\…`.
A tile has height_map_0.dds (L16) + .settings (ESF), often alpha_map_0.dds, colour_map_0.tga, and sometimes its own
tree/building lists and markers. `tiles\metadata.idx` starts with length-prefixed UTF-16 strings "RESERVED"/"UNOCCUPIED" and
0xFFFF runs (INFERRED: a tile-slot catalogue; layout UNKNOWN). INFERRED: generated battlefields are assembled from these tiles
following the `battleterrain\templates` masks in data.pack, while historical/MP maps use the fixed `presets`.

## 6. Not decoded
- `.markers` (54): starts with u16 length + UTF-16 "BASE_MARKER_REPOSITORY", then entries with names like "TREE_…" (custom
  binary, UNKNOWN).
- `.farm_fields_tile_texture` (131): first byte 0/1 then a table of u32 offsets (INFERRED: offset table of an RLE/chunked
  image). UNKNOWN.
- `.tai` grass atlas index: plain text with a header comment describing the columns
  (`<filename> <atlas filename>, <atlas idx>, <atlas type>, <woffset>, <hoffset>, <depth offset>, <width>, <height>`). CONFIRMED as text.

## 7. What a Bevy battlefield needs (plan)
1. Read `definition.xml` + `height_map_0.dds` + settings → a 1025² height grid → a Bevy mesh (2,048 m). Levels 1..3 as a cheap
   far ring later.
2. Colour from `colour_map_0.jpg` (+alpha) as the first material; detail/tiled maps later.
3. Deployment areas from `deployment_areas.xml`.
4. Sim side: ground type map → movement modifiers; heights → slope (fatigue gradient multipliers). The battle sim stays
   flat until the height formula is confirmed from the exe.
Note: JPEG and TGA decoding would need a crate or a small decoder; DDS L16 is trivial (raw u16).

## 8. Reader + renderer (battle-terrain worker, 2026-10-03)
Code: `ntw_formats::battle_terrain` (+ `tga`, `xml`), `napoleon::terrain` (`--battle --battle-map <name>`,
`--battle-map list`). Install tests: `cargo test -p ntw_formats --test battle_terrain_install -- --ignored`
(57/57 presets, 1,271 terrain TGAs, 109 tile `.settings`).

- **Grid layout — CONFIRMED statistically** (`examples/terrain_orient.rs`, 4 maps, all 8 orientations): column ↔ +x,
  **row 0 = the +y edge** (north up). Only this orientation puts buildings on the flattest ground (e.g. Waterloo
  slope 0.024 vs 0.10 map mean) and makes ground types under trees stand out (KL 0.83–1.10 bits vs ≤0.46).
  `ground_type_map_0.tga` (stored bottom-up) has the same layout once rows are put top-down, and `colour_map_0.jpg`
  too (napoleon test `colour_map_layout_matches_ground_types`: lowest within-ground-type colour variance unflipped).
- Map `(x, y)` = `ntw_sim` battle position (metres from the centre). Bevy: `(x, height_at(x, y), -y)`.
  How map y relates to the D3D world z used by the shaders (`uv = 0.5 + world.xz/2048`) is UNKNOWN (z = y with
  textures flipped on load, or z = −y).
- Heights: `sample/span·scale + bias`, span = the level's own sample range (fidelity 0-D: the exe rescales a
  `normalize` level by its min/max; every shipped level is normalised, starts at 0 and ends at 65534 or 65535;
  `analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` §5.1), bilinear (PROVISIONAL: the game may triangulate). Levels nest: level N+1 world size = 2× level N (CONFIRMED on all presets); level 0 =
  `base_terrain_width`. Sea maps (hb_naval, trafalgar, nile, caribbean) are flat at −100 m on 8,192 m.
- `TREE_LIST`: v3 = Bool, U32 groups, …; v2 (some MP maps) has no Bool; the per-instance U8 exists exactly when
  the Bool is true (CONFIRMED). Building list trailing F32 is NOT a scale (0.0 on Waterloo houses) — UNKNOWN.
- Building `Angle` u16 → radians `a/65536·2π`; rendered as −θ about Bevy Y; houses sit exactly on the footprints
  baked into the colour map (visual check, 180° ambiguity remains). Key → model by pack path
  `rigidmodels\buildings\<key>\<key>_pieceNN_destruct01_lod01` (INFERRED; 3 Waterloo keys have no such folder).
- Deployment `orientation`: 0 = +y, π/2 = +x (INFERRED from opposing armies); `sim_facing = π/2 − θ`.
- Terrain colour: `colour_map_near` from `fx\terrain_shared.fx_fragment` (shipped HLSL text). The real battle
  terrain technique is not in `fx\` (probably inside the exe) — renderer is PROVISIONAL.
- Maps are found by scanning the VFS for `battleterrain\presets\*\definition.xml`, so a mod pack adding a preset
  folder appears automatically. The original's own lookup (which DB table names a battle's preset) is UNKNOWN.

Open questions for Ghidra: heightfield loader formula and sample→world orientation; ground-type index names;
meaning of `bmd_type`/`additive_type`/`terrain_type`, the building F32, tree I32/U8; euler order of
`light_direction`; the battle terrain shader (detail/blend/cliff/rock maps); how a battle picks its preset.

## 9. Trees (battlefield worker, 2026-10-03)
Code: `ntw_formats::vegetation`, `napoleon::terrain::trees` (+ `trees.wgsl`). Probe: `cargo run -p ntw_formats --example
tree_probe -- model <climate> | ratio | ground <map> | palette <map>`. Install tests (`--ignored`):
`battle_terrain_install::every_tree_model_parses`, `every_tree_species_resolves`.

- Files (rigidmodels.pack, `rigidmodels\vegetation\battle\<climate>\`, 11 climates): 229 `.spt`, 11 `data.tree_model`,
  29 `*_compositemap.txt`, textures `textures\<map>_diffuse[_billboards].dds` (+ normal maps, bark maps).
- `.spt` = SpeedTree 4 tree descriptions (header token 1000 + `"__IdvSpt_02_"`, tagged tokens, Bezier-spline branch
  parameters as text) — CONFIRMED. The branch/frond/leaf geometry is generated at run time by the SpeedTree library: UNKNOWN
  to us, so the near-LOD trees are not reproduced.
- `data.tree_model`: f32 (UNKNOWN), u32 version 1, u32 count, records `{u16 n, UTF-16 .spt path, 14 × f32}` — CONFIRMED
  to EOF on all 11. Floats `[8..14]` INFERRED bounding box min/max (Y up; the vertical extent is symmetric on 311 of 345
  records, all trees but a few dead/odd ones), `[5]` grows with the height; the rest UNKNOWN.
- `*_compositemap.txt`: per `.spt` the UV rectangles of its `Leaves` and `Fronds` cards (in `_diffuse.dds`) and of its
  `Billboards` (in `_diffuse_billboards.dds`, 1024², DXT5): **8 pictures per tree, none for shrubs** (CONFIRMED). The
  pictures have a fixed height (165 px) and per-tree width (the tree's aspect).
- Species → file: tree lists use either the `warscape_trees` key or the lower-case `.spt` stem (the DB keys contain typos,
  e.g. `Chrsitmas_Scotch_Pine`), and `lc_sand_desert-Low_Brush_shrub` points at the fan palm in the DB. Our route: DB row by
  key or stem → else `<climate>\<species>[_<season>].spt` directly (which route the exe takes: UNKNOWN). The `lc_am_*`
  climates (Empire leftovers on 2 MP maps) have no files at all.
- `TREE_LOD_LIST` (CONFIRMED on the maps checked): list 0 = trees inside the 2 km playable area, with the U8
  (63..253); list 1 = trees outside it (out to ~4.8 km), no U8; list 2 empty. INFERRED: U8 = scale, 128 = 1.0 (PROVISIONAL).
- Drawing (PROVISIONAL): all trees as cylindrical billboards showing the picture nearest the view direction (which
  direction picture 0 shows is UNKNOWN), height = box vertical extent × scale, width = height × picture aspect,
  alpha test 0.33, flat lighting. One static mesh per billboard texture (180k trees on Austerlitz = 1 draw call).
  Shrubs are not drawn.

## 10. Ground types, slope and deployment in the sim (battlefield worker)
Code: `ntw_sim::battle::ground`, `napoleon::battle::setup`.
- **Ground-type names** — the TGA index is the position in a 25-entry name table in `Napoleon.exe` (pointer array at file
  offset 0x1050720, read as data): `field_ploughed, field_ploughed_wet, field_forest, grassland, mud, mud_wet, road,
  road_frozen, rock, sand, sand_wet, scree, snow, stone_masonry, vegetation_dense_forest, vegetation_light_scrub,
  vegetation_medium_woodland, water_deep, water_frozen, water_medium_ford, water_shallow, wood, stone, glass, none`,
  then `end_marker`, `invalid_ground_type`. Table order CONFIRMED (bytes); index = TGA palette index INFERRED (the palette
  has exactly 25 entries, identical on every map, and its editor colours match: grassland (0,255,0), road black, snow
  white, waters blue/cyan, vegetation greens).
- **Speed**: `unit_movement_modifiers` (28 rows, `s f f f f`) by name; the column for a unit is UNKNOWN — PROVISIONAL
  col 0 artillery, col 1 cavalry, col 2 infantry, col 3 unused. The modifier at the unit's position scales its step.
- **Slope**: the move's gradient (height gained / distance, level-0 heightfield, bilinear) feeds the existing `kv_fatigue`
  gradient ladder (>0.05 / >0.1 / >0.2 → 133 / 166 / 200 %); "moving on a slope" = moved this tick (PROVISIONAL); downhill
  never reaches the thresholds. No slope speed rule was found in the data, so slope does not change speed.
- **Deployment**: the 1v1 block (first) of `deployment_areas.xml`; alliance 0 = player (PROVISIONAL). Two area forms:
  `centre/width/height/orientation`, and an outline of 5 `position` corners (hb_waterloo and others) read as a rectangle
  whose 2nd edge points to the rear (INFERRED). Install test `deployment_areas_face_each_other`: in all 56 1v1 setups both
  armies face the other's area (cos > 0.5). Units are laid out by the original's default deployment from `groupformations.bin` (fidelity 0-D, `analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` §5.2); side by side 10 m apart only as a fallback.
- Formation depth `unit_stats_land` #43 (INFERRED ranks), speeds from `battle_entities`.

Open questions for Ghidra: the `unit_movement_modifiers` column per unit class; whether the ground type is sampled per man
or per unit; the exact gradient and "moving on a slope" test; default deployment layout in an area and which alliance is
the player's; the tree-list U8 and the near/far tree LOD; billboard picture order; the battle terrain shader with its
detail/blend/cliff/rock maps (not in `fx\`: `battlefieldterrain.fx` is an empty stub returning black).

## Where I am / what's next (battlefield worker, updated with each push)
- Done and pushed on `work/battlefield`: trees (billboards), deployment zones + DB ranks/speeds, ground types and
  slope in `ntw_sim` (+ tests), GPU skinning with per-man phase (FPS in `analysis/units/ANIM_FORMAT.md` §4), unit labels
  in the game's `.cuf` font, hidden over the HUD. Main merged at 62915fc (runtime map loading kept working).
- Next (not started): detail/cliff texture maps and grass (task 6); shrubs (no billboards, need SpeedTree geometry);
  per-man clip alternatives; tree normal maps / lighting from the `.environment`.
- Audio hook spots: shots are `Battle::volleys` (copied into `VolleyFx` in `battle::tick_battle`); impacts = a volley's
  `kills` at its target. A sound system can read `VolleyFx` there without touching the model.

# Campaign data and saves (BACKLOG §1)

Worker: campaign-data (branch `work/campaign-data`). Tags: CONFIRMED / INFERRED / UNKNOWN; our stand-ins
PLACEHOLDER / PROVISIONAL. Ghidra findings are written here as specs; decompiled code is never stored.

## Where I am / what's next
- **Done:** §1 items 1-4: `pathfinding.esf` and `sea_grids.esf` (§1, §2); the detail records (§3: names,
  portraits, traits, ancillaries, government posts, leader, ministers, governorship taxes, capitals, full
  diplomacy records); the script `save_value` slots (§4); save compatibility (§5: all 8 user saves round-trip
  byte for byte).
- **Done too:** the small §1 items: markers and farm files (§6), all 286 weighted meshes (§7), all 126
  uied.templates entries + ClipChildren (UI_LAYOUT_FORMAT.md), all 3,814 anims (§8), all 8,239 DDS (§9), languages
  (§10). Bink was given to a separate worker. Merged main (a4c0cb9) on 2026-10-03.
- **Second pass (s1-open worker, §11):** grass textures solved (TGA chunks); `other_income_mod` lists INFERRED not
  saved; cell header run-time layout found (meaning UNKNOWN); UI `unknown_e5`/`unknown_140` named (UI_LAYOUT_FORMAT.md).
- **Open (for a later round / Ghidra):** the campaign path search is FOUND and ported (PATHFINDING.md: search, smoothing, cell header bytes, flag bits, zones of control with the CONFIRMED 1-unit core; open: the run-time polygon cutter and mover families on work/pathfinding-ports); state `unknown_d0/d4` (60/64 RESOLVED: TextX/YOffset; DrawMode 0/1/2 CONFIRMED, UI_LAYOUT_FORMAT.md); testing our saves in the original game. (Mission records: layout done and targets re-linked, S1_MISSIONS_UI.md; `DIPLOMACY_RELATIONSHIP` fields: done, #16/#21/#22 CONFIRMED unused, S1_LEFTOVERS.md §1.)
- HUD now shows character names (selection bar, unit cards, lists), capitals (`IsCapital`) and the faction leader
  (`FactionDetails().Leader` {Name, Address, Portrait}: PROVISIONAL field names). Checked with
  `--campaign eur_napoleon --campaign-demo --screenshot`: "Jean-André Masséna, France".

Tools:
- `cargo run -p ntw_campaign --example pf_probe -- "<data>\campaign_maps\<map>" <cmd>` with `fields`,
  `cell <col> <row>`, `orient`, `area`, `png <out.png> <px per unit> [x0 z0 x1 z1] [region|lo|flags0..3]`,
  `grid <out.png>` (the movement grid we build).
- Install tests: `cargo test -p ntw_formats --test pathfinding_install -- --ignored --nocapture`.

## 1. `pathfinding.esf` (`ntw_formats::campaign_pathfinding`)
Layout (CONFIRMED on all 5 maps; the reader consumes every value):
```text
root v0
  {obstacles} {u8[1024]}            nap_spain only: the key of the value cipher (1.2)
  pathfinding_areas[] (1 item)
    vertices[] {i32 x, i32 z}       Fixed20; items 0..3 are (-2^31+1, -2^31+1) sentinels
    u32[]                           outline lists: n, then n vertex indices (the array splits exactly)
    {grid_data}
      i32 x0, i32 z0                Fixed20 south-west corner = the theatre's minimum
      u16, u16                      UNKNOWN (Europe 115, 65; Italy 68, 100)
      i32 cell                      Fixed20 2.0 on every map
      u32 cols, u32 rows            Europe 375 x 193 (= theatre / 2)
      u32                           the total polygon (boundary) count (CONFIRMED all 5 maps: Europe 103563)
      u16 region_count, u16         region_count = the map's land regions (Europe 72); second u16 equal
      i16[region_count]             region map: values are regions.esf region indices
      u16[]                         region table, below
      grid_cells[]                  below
  barriers[]                        nap_tut only: {utf16 name "shroud_map_1_3", 9 x u32[]} (INFERRED Fixed20 polylines)
```
- **Region table** (CONFIRMED by the exe's reader `FUN_00af2050` and by matching cells to `regions.esf`): the
  first `region_count` u16 are 1-based indices into the i16 region map; then count-prefixed groups of the same
  indices. Region id `k` (the 10-bit id in a boundary) means: `k < region_count` the single region
  `map[table[k]-1]`; higher ids are the groups in order (the first group is empty = the **sea**; the others are
  pairs/triples = the thin shared strips along region borders); **1023 = no region** (off-map: the big
  impassable mountain blobs and the land outside the theatre).
- **Grid cells**: one record is either {u8[8], `boundaries[]` {u32 flags, u32 link}} or, when `boundaries` is
  empty, a compact run {u8[8], [], u32 flags, u16 region id, u8[12 x n]} = this cell plus n more cells, each
  {u8[8] header, u32 flags} with the single boundary {flags, region << 22}. Expanded, the records give exactly
  `cols x rows` cells (CONFIRMED on all 5 maps), row-major from the south-west (CONFIRMED: every single-region
  cell of all 5 maps lies in its region in `regions.esf`; 0 match flipped).
  **nap_spain stores its cells last-first** (the encrypted stream's cell reader `FUN_00b60b20` reverses the list
  after reading; CONFIRMED 7527/7527 matches reversed, 29 in file order).
- **Boundary link**: bits 0..22 = offset of an outline list in the u32 array (CONFIRMED: always a list start;
  one boundary per list except the first list), bits 22..32 = the region id above.
- **Outlines**: each boundary's list is a polygon of its cell. Indices 0..3 are the **cell's corners**: 0 SW,
  1 NW, 2 SE, 3 NE (CONFIRMED: with this mapping the polygons of every split cell of Italy add up to exactly
  the cell's area, and all but one wind counter-clockwise). So each 2x2 cell is partitioned into labelled
  polygons: this is the original's walkable map.
- **Flags** (word 0): low 3 bits, by where they lie when rendered: 0 land, 1 water (sea; 17 too), 2 off-map, 3 river strips (PATHFINDING.md §5)
  (only with id 1023), 6 road (the ~0.3-unit strips along every road, and the octagon round every
  settlement), 7 road over water (INFERRED bridge/ford). INFERRED. Bits 4..23 (incl. the high nibble of byte 0: 16 interior
  land, 32/64/128/0 next to roads): INFERRED per-side segment masks used by the adjacency test (PATHFINDING.md §3); bits 24..31 = a per-direction "reaches that neighbour" mask (CONFIRMED by the data, PATHFINDING.md §3). Rivers are not obstacles in this
  data (no blocking strips along the river splines).
- **Cell header u8[8] = the per-direction movement cost bytes** (CONFIRMED in the path search, PATHFINDING.md §2: byte d is the cost of leaving the cell in direction d = 0 E, 1 NW, 2 N, 3 NE, 4 SW, 5 S, 6 SE, 7 W; multiplier = byte · 0.00995 + 0.796). Earlier notes (s1-open worker, see §11): the exe keeps it unchanged in the 16-byte run-time cell
  {boundary pointer, boundary count, the 8 header bytes} (CONFIRMED `0x00AF3230` / `0x00AF3290`). Meaning still UNKNOWN;
  data facts (all 5 maps, `pf_probe headers`): bytes 0, 2, 5, 7 are edge values shared with the neighbour (byte 7 = the
  west neighbour's byte 0, byte 2 = the north neighbour's byte 5, ~90 %); bytes 1, 3, 4, 6 are 255 on off-map cells;
  every byte is a spatially smooth field (Egypt: 85-90 % equal to the same byte of each neighbour), ~31 on open land in
  Italy, low (0-10) at sea, rising (~100-121) towards the impassable mountain blobs. Not the display heightmap (correlation
  ~0.3). INFERRED: a coarse per-cell terrain/clearance field used by the search; which one is UNKNOWN.

### 1.1 The movement grid (`ntw_campaign::pathing::build_grid`)
From these polygons: 1-unit cells (half the original cell; PROVISIONAL subdivision) over the original grid;
kind from the polygon under the centre (land region + land/road kind = Land, sea set or water = Sea, id 1023
or off-map = Blocked); a road cell if any road polygon overlaps it; region from the polygon's set. Europe
builds in ~0.35 s (dev profile). The raster stays for region lookups and as a fallback; paths now come from the original
polygon search ported in `ntw_sim::campaign::polypath` (PATHFINDING.md).
- (Raster fallback only.) Off-road cost: `road_level_0_action_point_cost` per map unit; road cells cost the region's road level
  (`road_level_<n>_action_point_cost`). In the original search off-road costs come from the cell header bytes instead (PATHFINDING.md §2).
- `campaign_ground_types` (19 rows: key, f32, 3 bools: plains 1.3, grassland 0.9, hills 0.6, forest 0.5,
  swamp 0.2, dense forest/jungle 0.0; bool 1 forests, bool 2 desert, bool 3 the `*_cold_att` rows): the f32
  cannot be a movement multiplier (0.0 for forests), INFERRED a farming/fertility factor; the per-location
  ground type is not in `pathfinding.esf` (UNKNOWN where: `metadata.dat` or `*_lookup.tga` are candidates).
  So no ground multiplier is applied to movement.

### 1.2 The value cipher (`obstacles`, nap_spain)
CONFIRMED (Spain decrypts to sane data: vertices inside the theatre, lists split, cells link to list starts).
`FUN_00b51f80` builds a 0x40C-byte decoding stream when `obstacles` exists, its first byte is 0x93 **and** a
game-settings bit (0x200 of a word reached from the campaign) is set (INFERRED: content ownership; we always
decode). Per byte (`FUN_00b273f0`): `seed = seed * 0x343FD + 0x269EC3` (wrapping u32),
`b ^= (seed >> 16) as u8 + key[pos]`, `pos = (pos - step) & 0x3FF`; start `seed` = u32 at key bytes 1..5,
`pos` = 0, `step` = u32 at key bytes 0x237..0x23B. Decrypted, in file order: the vertices (8 bytes each), the
u32 list array, the grid header (#0..#9), the i16 and u16 arrays. The cells are not XORed (read by the
stream's own cell reader).

## 2. `sea_grids.esf` (`SeaGrid`)
Root `CAI_SEA_GRID_ROOT` v0, all values flat (CONFIRMED: read to the last value on all 5 maps): u32 1, utf16
id (a number as text), coord2d min, coord2d max (= theatre), f32 cell size 75, u32 cols, u32 rows; per cell
{u32 col, u32 row, coord2d min, coord2d max}; per cell a zone {u32 index, n x utf16 land side (map areas like
`eur_map_west`, land region keys, `all`), n x utf16 sea regions, n x utf16 ports `port:<region>:<town>`, n x u32
(UNKNOWN ids)}; u32 count, then {u32 zone a, u32 zone b, f32 distance} for every pair of non-empty zones
(Europe 741 = 39 choose 2). It is the **campaign AI's** coarse sea-zone graph (`CAI_SEA_GRID_CELL*` names in
the exe), not navy movement: navies move on the sea polygons of `pathfinding.esf`. Loaded into
`CampaignMap::sea_grid` for the AI.

## Questions for Ghidra
1. ~~The campaign path search~~ SOLVED: PATHFINDING.md (A* over (cell, polygon) nodes; roads cost per region).
2. ~~Cell header bytes~~ (per-direction costs) and ~~the u32~~ (polygon count) SOLVED; the flag high bits (INFERRED edge mask, PATHFINDING.md §3) and the two u16 in `grid_data` remain.
3. `barriers` (tutorial shroud lines): how they block movement.
4. The content bit checked before decrypting nap_spain.

## 3. Startpos/save details now loaded (`ntw_sim::campaign::details`, loader `ntw_campaign/src/details.rs`)
Kept in `World::character_details`, `World::faction_details`, `World::relationships` (separate maps so
code that builds `Character`/`Faction` by hand is unchanged). Checked on all 8 startpos files and the 8 user saves
by `tests/real_install.rs` (`check_details`).

| ESF | model | tag |
|---|---|---|
| `CHARACTER_DETAILS` v3: #0 `TRAITS/TRAIT[]` {key, i32}; #1/#2 `CAMPAIGN_LOCALISATION` forename/surname loc keys (`names_name_names_frenchNapoléon`); #3 a second name pair; #4 utf16; #5 `DATE` birth; #6 `DATE`; #7 i32; #8 `PORTRAIT_DETAILS` {card tga, utf16, info jpg, i32 index}; #9 faction key; #10 u32 = the character id; #11 `AgentAncillaries[]` {key}; #12 `AgentAttributes[]` {key, i32}; #13 `AgentAbilities[]` {key, i32, utf16}; #14 `AgentAttributeBonuses[]` {key, u32}; #15 bool; #16 on-screen type name loc key; #17 utf16 | `CharacterDetails` | structure CONFIRMED; birth INFERRED; #3 #4 #6 #7 #15 #17 UNKNOWN |
| `CHARACTER` #8 u32 | `CharacterDetails::post` | CONFIRMED: = the `CHARACTER_POST` id the character holds |
| `GOVERNMENT` v1: #0 i32 id, #1 key, #2 i32, `POSTS_ARRAY[]` of `CHARACTER_POST` v1 {i32 post id, utf16 `ministerial_positions` key, u32 holder, bool is governorship, then i32 government id or `GOVERNORSHIP` + i32} | `FactionDetails::posts`, `leader()`, `ministers()` | CONFIRMED (France: finance, faction_leader = Napoleon, navy, accident, justice, army, head_of_government, governor_europe) |
| `GOVERNORSHIP` v1: `GOVERNORSHIP_TAXES` {u32 lower level, u32 upper level, u8 lower rate, u8 upper rate}, i32 (= REGION #22), u32[] governed region ids, u32 faction id, bool, bool | `Governorship`; `Faction::tax_lower/upper` | structure CONFIRMED; u8 = `taxes_levels` rate CONFIRMED (15 normal); level index order minimal..extortionate INFERRED (only level 2 occurs) |
| `FACTION`: the two i32 after the last `FORT_UPGRADE_MANAGER` | `FactionDetails::capital` / `capital_2` | CONFIRMED Paris for France; every living faction has one; meaning INFERRED |
| `FACTION`: the `rel_*` utf16 | `FactionDetails::religion` | CONFIRMED values |
| `DIPLOMACY_RELATIONSHIP` v14 (writer `0x00AFC340`, reader `0x00AE9480`): #0 target, #1 24 attitude factors {drift, value, limit, limited, cap, capped}, #2 trade agreement, #3 military access turns given, #4 stance, #5 war ally, #6 alliance commitment turns, #7 war momentum, #8 protectorate tribute, #9 protectorate income, #10..#13 war balances and counters, `REGULAR_PAYMENTS[]` {amount, turns}, #15 friendship countdown, #16, `ALLIED_IN_WAR_AGAINST[]` {enemy, saved access turns}, u32[14] `force_diplomacy` permissions, #19 access streak, #20 previous stance, #21 #22, #23 start attitude, #24..#26 military access grant/elapsed/grievance, #27 embargo turns, #28 allows region return | `Relationship` (every field named) | **Resolved by the s1-leftovers worker: see `S1_LEFTOVERS.md` §1** (tags per field; #16, #21, #22 UNKNOWN: the exe only loads, saves and copies them; #2 is the trade agreement, not military access) |

Not found / not in the data:
- **Missions** (RESOLVED 2026-10-03, `analysis/campaign/S1_MISSIONS_UI.md`): the `CAMPAIGN_MISSION_MANAGER` /
  `CAMPAIGN_MISSION` (+ `_OBJECTIVES`, `_REWARDS`, `_LOCALISATION_OVERRIDES`) layout is CONFIRMED from the exe's writer
  `0x0099F820` and reader `0x00989CF0`; reader + writer `ntw_campaign::missions`. The start positions hold none; the
  user's newest `auto_save.save` holds one (France, `eur_take_vienna`), which reads and writes back identically.
- **`other_income_mod(faction, amount)`** (`FUN_0097bb70` → `FUN_00a15b30`): a positive amount is appended as an
  8-byte entry to a per-faction list (faction +0x130) and added to economics category 3 (other income, the
  `ECONOMICS_DATA` #2 i32[4] slot 3 = 1700 for France); a negative one goes to a second list (+0x13C). Where those
  lists are saved: **nowhere (INFERRED)**: of the 356 functions touching those member offsets, only the adder
  (`0x00A15B30`) and a clear-all (`0x00A0EAF0`, run while the campaign model is torn down from `0x0088D1A0`) walk the
  lists; no ESF writer or loader reads them (the `FACTION` writer `0x00892A80` and `FACTION_ECONOMICS` writer
  `0x00BD46E0` do not). The amount itself is saved through the economics data it was added to (category 3).
- **Campaign variables**: the DB `campaign_variables` with the campaign's junction overrides, already in
  `CampaignRules::variables`; not save data.
- `FACTION_ECONOMICS` v6: `history[]` of `ECONOMICS_DATA` v5 (6 i32 arrays per turn: income/expense categories),
  #1 treasury, #2 u8[25] (all 1 in the saves; UNKNOWN), #3 u32.

## 4. Script `save_value` slots (`ntw_campaign::script_values`)
`CAMPAIGN_MODEL/EPISODIC_RESTRICTIONS` v17 child `LUA[]`: one value per item, in call order (13 in the user's
saves). CONFIRMED from the Lua handlers: `save_value` (`FUN_009796c0`) stores a **bool** (0x01) for a Lua boolean,
otherwise `lua_tointeger(v)` as an **i32** (0x04); `load_value(default)` (`FUN_009797f0`) reads the next item with
the default's type, else returns the default. Loaded into `LoadedCampaign::script_values`; the app gives them to
`ScriptHost::load_values` before `LoadingGame`, and F5 fires `SavingGame` and writes the host's values back
(`save::write_save_with`).

## 5. Save compatibility
- **Original saves load fully**: all 8 user saves (mp_eur, 1.3.0 build 2081) load with every detail record
  (`tests/real_install.rs`).
- **Our writer is byte-faithful**: loading each user save and writing it back with no changes gives the original
  file **byte for byte** (measured once; the test was removed 2026-10-08, loading the original's saves being out of scope). The writer
  patches the source tree in place; for things the model rebuilds it keeps the original records: construction
  items (only the turns-done counter moves), repair items (`BUILDING_CONSTRUCTION_ITEM` #0 = 2, no key: kept),
  recruitment items (kept in the manager they were in, including the several port managers of one region;
  only turns and cost move).
- Written from the model: header, calendar, RNG, treasury, character positions and action points, forces and
  units, region owners, buildings, construction and recruitment, **taxes** (all governorships), **stances**
  (#4, the old one moved to #20), the **script slots**.
- **What the original needs to load ours** (we cannot run it): the same root (`CAMPAIGN_SAVE_GAME` v5 with
  `BUILD`, `SAVE_GAME_HEADER` v2, `CAMPAIGN_ENV`), every block it wrote (we keep all of them: AI 18 MB, trade,
  shroud, pathfinder, camera, ...), the same record versions and child order, consistent ids. Risks that remain
  UNKNOWN: new records we create from templates (new forces, units, buildings, recruitment items: ids are ours,
  PROVISIONAL), a commander-less garrison `ARMY`, and AI/shroud blocks that still describe the state before our
  changes (the original rebuilds most AI state each turn, INFERRED).

## 6. Battle-map extras (`ntw_formats::battle_markers`; install test `tests/battle_extras_install.rs`)
- **`.markers`** (55 files, 54 battle maps/tiles + `profiling_scripts`): CONFIRMED layout (every file reads to the
  last byte; matches the exe's writer `FUN_00f9fde0`): `"BASE_MARKER_REPOSITORY"`, key16 (12 zero bytes + u32 id),
  u32 1, u16 group count; per group key16 (class id), name, key16, u32 1, u32 item count and, when non-zero,
  the items, u16 model groups {u16 1, utf16 model key, u32 n, n x u32 item index}. Classes: `0x9D9BE`
  (second `PROP_MARKER`) placed props {f32 x, f32 y, u32 (UNKNOWN: angle or seed), f32 scale} (only the 3
  `nap_mp_*` maps, 1,532 each); `0x9D9C0` (third `PROP_MARKER`) polygons {u32 n, n x (x, y)} (119, 97 closed;
  INFERRED clear areas) with a NUL model key; `TREE_MARKER`, `BUILDING_MARKER`, the first `PROP_MARKER` and
  `DECAL_MARKER` are empty in every file (item layout UNKNOWN).
- **`rigidmodels\campaignbridges\bridge.markers`**: another format: {utf16 `bridge:<faction>:<n>`, 4x4 f32} to the
  end (68 campaign bridges; `NamedTransforms`).
- **`.farm_fields_tile_texture`** (131): `u8 kind, u32 n, (n+1) x u32 absolute offsets` (last = file length,
  CONFIRMED). Kind 1 (blend, colour; 111 files): chunk = `u32 a, u32 b`, two JPEG files (CONFIRMED FF D8;
  INFERRED colour + alpha). Kind 0 (20 large grass maps): **every chunk is an uncompressed TGA file** (type 2, 32-bit, 8
  alpha bits; CONFIRMED on all 720 chunks: header size x 4 + 18 = chunk size, apart from trailing zero padding in a few;
  the "u32 0x20000" was the TGA header's first bytes). `FarmTileTexture::tga`. The 20 small fort grass maps are kind 1.
  The pixels' channel meanings (as in a preset `grassmap.tga`: B, G, A used) are UNKNOWN.
- **`.prop_list`** (20, `BATTLE_PROP_LIST` v1): u32 n, n x {utf16 model, coord2d, angle, f32 scale} as flat
  values (28,277 props; `read_prop_list`).
- **`.farm_template_tile`** (32, `ROOT_FARM_TILE_TEMPLATE`) and **`.farm_manager`** (34, `ROOT_FARM_MANAGER`
  {`FARM_MANAGER` v3: bounds, `FARM_TILE_TEMPLATE` names, 3 `FARM_TILE_SET`, `FARM_INSTANCE`s}): ESF, read
  losslessly (round-trip test); `FarmTileTemplate` summarises farms, walls, every `FARM_DATA_ITEM` {key,
  position, angle, scale}, trees, wall posts and `EF_LINE`s. The fields of `FARM_COLLISION`,
  `FARM_TILE_SET`, `FARM_INSTANCE` and the whole `FARM_MANAGER` are now typed and named (s1-leftovers worker,
  `S1_LEFTOVERS.md` §2; a few values stay UNKNOWN there).

## 7. `.variant_weighted_mesh`: the last 23 files (`ntw_formats::weighted_mesh`)
All 286 files in the packs now read to the end (`tests/real_install.rs::all_286_weighted_meshes_parse`).
- The final u32 (formerly "UNKNOWN, 0") is an **attachment count**: `napoleon_battleoutfit_lod1/2` and
  `campaign_napoleon_battleoutfit_lod1/2` carry 1 (his sword, `rigid_equip_euro_cutlass01`), `euro_equipment` 134.
  An attachment (CONFIRMED by reading to EOF): magic, u32 5, utf16 name, u32 (UNKNOWN 1/2), 3 x {u8, utf16
  texture: diffuse, normal, gloss}, 2 bytes, materials (scalars, vectors), then a **rigid** mesh: u32 V,
  V x 20 f32 (INFERRED position, normal, uv, tangent, binormal, colour, uv2), u32 I, I x u32.
- **Headerless layouts** (no magic/version/materials), tried in order: (1) piece table + current vertices +
  attachments: the campaign agents `campaign_native_american_general_lod3/4`, `campaign_ottoman_commodore_lod3/4`,
  `campaign_soldier_base`; (2) piece table + vertices without the 4 extra floats + old attachments {name, u32 3,
  u32 V, V x 14 f32, u32 I, I x u32}: `testdata\diplomat`, `testdata\euroline`; (3) {u32 pieces, u32 total V,
  u32 total I} + unnamed pieces with {uv, influences} vertices: `testdata\test`, `ranger_test_lod1..4`;
  (4) the same with {uv, normal, tangent, influences}: `testdata\ranger\*`, `testdata\musketman\*`.

## 8. Older `.anim` clips
All 3,814 `.anim` files read (`every_anim_parses`): the 31 `testdata\animations` clips have 28-byte keys (no
unknown floats); the 4 oldest testdata clips have unnamed bones (parent indices only) and no events. See
`analysis/units/ANIM_FORMAT.md` §1.

## 9. Exotic DDS formats (`ntw_formats::dds`)
The fourCC field holds a D3DFORMAT number in two files: 36 = `A16B16G16R16` (`testdata\gadgets.dds`, 8 bytes per
pixel, decoded by keeping each channel's high byte) and 63 = `Q8W8V8U8` (`ui\cinematicicons4.dds`, 4 bytes per pixel;
decoded as plain RGBA bytes: it renders as the cinematic control icon strip, so the signed meaning is not used;
INFERRED). All 8,239 DDS files decode (`every_dds_parses`).

## 10. Languages
- The install chooses its text language with `data\language.txt` (`EN`; the exe reads that file, CONFIRMED string,
  e.g. in the video module `FUN_011c3520`) and the matching `local_<code>*.pack` files (this install has only
  `local_en` and `local_en_patch`).
- `ntw_formats::pack`: `installed_languages(data)`, `install_language(data)`, `effective_language(data)`,
  `set_language_override(..)`, `Vfs::open_install_language(data, code)` (any installed language). `Vfs::open_install`
  mounts the effective language, so every loader (loc, UI, scripts) follows the setting.
- Setting (our own; the install's file is never written): `--language <code>` or a one-line
  `%APPDATA%\NapoleonRust\language.txt`; a language that is not installed is ignored with a message.

## 11. Open §1 questions, second pass (s1-open worker, 2026-10-03, branch `work/s1-open`)
Ghidra on the main project (read-only), tooling `analysis/graphics/run_ghidra.ps1` + `GfxDecomp.java`.
- **Farm grass textures: SOLVED.** Kind-0 chunks are plain TGA files (§6). Install test `all_farm_textures_read` decodes
  all 720.
- **`other_income_mod` storage: answered, INFERRED not saved** (§3 "Not found"). The positive list (8-byte entries
  {faction, amount}) and the negative list (12-byte entries) live at +0x12C..+0x144 of the object the adder runs on
  (the decompile does not show for sure whether that is the faction or the campaign world); only the adder
  and the teardown clear touch them.
- **Pathfinding cell header:** run-time layout CONFIRMED, meaning UNKNOWN; data facts in §1.
- **Boundary words at run time** (CONFIRMED `0x00B78960`, `0x00B108D0`, `0x00B28E30`, `0x00AEF3E0`): word 0 = bits 0..21
  outline offset (bit 21 = 0x200000 marks a polygon added at run time, stored in a 24-byte pool at pathfinder +0x124, i.e.
  cut by forts/armies/barriers), bits 22..31 region id; word 1's low 4 bits are the polygon kind bits that run-time edits
  copy (`& 0xF`). The compact cell form builds word 0 = region << 22 (offset 0). Which of the high flag bits (16..128 in
  byte 0, bytes 1..3) the search reads: UNKNOWN.
- **The campaign path search: NOT found yet** (our grid A* stays PROVISIONAL). Leads: the pathfinder is created right
  after the log line "Creating CAMPAIGN_PATHFINDER" by `0x00AF8750` (campaign setup `0x00872550`); its code sits in
  `0x00B08000..0x00B80000` (boundary/outline queries `0x00B79DA0` and its 11 callers, e.g. `0x00B153A0`, `0x00B489E0`,
  `0x00B5CCC0`); `0x00B49A80` (13.5 KB, 64-bit fixed-point maths) is the polygon geometry (cutting), not the search.
  `road_level_N_action_point_cost` are only DB column names in the exe (globals `0x0164B5A0..B8`, no direct code
  reference), so the cost rule must be found from the search side. AI-only structures (`CAI_HIGH_LEVEL_PATHFINDER`,
  `HLP_DISTANCE_MULTIPLIER_*`) are a separate high-level region graph.
- **Polygon kind = the low 4 bits of the boundary flags** (CONFIRMED: the getter `0x00B77EC0` returns `flags & 0xF`;
  every rule below compares that value). Kinds in the data (all 5 maps): 0 land, 1 sea, 2 and 3 off-map, 4 and 5 (rare:
  73 / 71 polygons on Europe and the tutorial; meaning UNKNOWN), 6 road, 7 road over water (INFERRED bridge/ford). Kinds
  8..11 appear only in the code (INFERRED run-time polygons). The high nibble (16..128) is not part of the kind.
- **A cell flood search found** (`0x00B39FE0`, 7 callers incl. army code at `0x008ECAD0`/`0x008ECE20`; siblings
  `0x00B322F0`, `0x00B34010`, `0x00B35640`, `0x00B3B4B0`, `0x00B3D180` share its heap helpers `0x00AC1B50`/`0x00AF8620`):
  a best-first flood over grid cells from a query point. Open list = binary heap of 36-byte nodes {cell, grid, priority,
  per-polygon reachable flags, mover type}; closed set = hash of the packed (col, row) xor 0x4A545EED; it expands the 8
  neighbours of each cell; a node's priority (`0x00AF0910`) is minus the smallest signed distance from the query point to
  the cell's four edges (so cells are visited nearest first); it stops at a distance limit. It answers "which polygons
  around this point are reachable for this mover" (CONFIRMED structure); it is not the action-point path itself.
- **Which polygon kinds each mover type may enter** (CONFIRMED table in `0x00AF0910`; the mover type is the node's
  +0x20 value, its meaning UNKNOWN): types 0, 3, 6, 9: kinds 0, 6, 7 (land, road, bridge); types 1, 4, 7, 10: kinds 0, 6, 7,
  8; types 5, 8, 11 and the others: kinds 1, 4, 7 (sea, kind 4, bridge/ford). Kinds 10 and 11 pass for every type. A
  polygon of an allowed kind counts as reachable only if it touches the current one (`0x00B16800`, edge test). INFERRED:
  armies use the land set, fleets the sea set; kind 7 is shared, i.e. both can cross at bridges/fords.
- Our `ntw_campaign::pathing` already treats land + road + kind 7 as passable for armies and sea for fleets, so the kind
  rules agree; the cost along the path (roads, `road_level_*`) and the funnel are still UNKNOWN and stay PROVISIONAL.

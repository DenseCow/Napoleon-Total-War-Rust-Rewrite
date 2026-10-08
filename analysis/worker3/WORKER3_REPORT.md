# Worker 3 report: campaign data, save format, scripting and UI data

Napoleon: Total War (Steam build, data dated 2010; saves written by "1.3.0 Build 2081"). Everything was read only from the install and `%APPDATA%`. Nothing there was written.

Tags: **CONFIRMED** means verified from bytes, by an exact parse of every file, or by a cross-check. **INFERRED** means a strong hypothesis from patterns. **UNKNOWN** means not determined.

Tooling: `analysis\worker3\campaign_tools\` (Rust, std only, no crates). Build it with
`CARGO_TARGET_DIR=%USERPROFILE%\Documents\NapoleonRust\target-w3 cargo build --release`.
Binary: `target-w3\release\campaign_tools.exe`. Subcommands:

| command | purpose |
|---|---|
| `esf-scan-all` | parse every loose `.esf`, scan all packs for `.esf` names and ABCE magic |
| `esf-summary FILE [depth]` | header, counts, value-type histogram, schema tree (record paths, versions, child signature) |
| `esf-paths FILE` | flat list of unique record paths with counts and signatures (diffable) |
| `esf-tree FILE PATH [depth] [items] [matches]` | dump a subtree, e.g. `CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY[0]/FACTION` |
| `startpos FILE [summary\|regions\|chars\|<faction>]` | decoded start position or save summary |
| `lua-scan calls\|events\|context\|defs\|firstargs\|files [filter] [pat]` | Lua source scanner |
| `luac-info summary\|calls\|globals\|strings\|events` | Lua 5.1 bytecode reader for the UI `.luac` files |
| `lua-api` | writes the full API listing (`lua_api.txt`) |
| `ui-probe summary\|detail\|strings <substr>` | binary UI layout probe |
| `pack-ls/pack-head/pack-text PACK ...`, `hex FILE OFFHEX N` | small pack and hex helpers (FILE may be `pack:<pack>:<entry>`) |

---

## 1. Inventory

### Loose files (CONFIRMED, `find`)
- `data\campaigns\<c>\{startpos.esf, scripting.lua}` for 8 campaigns: `eur_napoleon` (32.0 MB esf), `mp_eur_napoleon` (32.3 MB), `egy_napoleon` (4.8 MB), `mp_egy_napoleon`, `ita_napoleon` (5.1 MB), `mp_ita_napoleon`, `spa_napoleon` (7.1 MB, Peninsular DLC), `tut_napoleon` (7.8 MB).
- `data\campaign_maps\nap_{europe,egypt,italy,spain,tut}\` contains `regions.esf`, `pathfinding.esf`, `poi.esf`, `sea_grids.esf`, `trade_routes.esf`, `metadata.dat` (missing for nap_tut), `*_lookup.tga`, `*_map.tga`, `stratradar_*.tga` and `display\`.
  - `display\` subfolders: `borders/roads/rivers/traderoutes *.rigid_spline` (903 in total), `heightmap\heightmap.tga`, `supertexture\supertexture.{stpi,stpd}`, `environment\{spring,summer,autumn,winter}.lighting`, `world.markers`, `bridges\bridge.markers`, `coastline\*.rigid_mesh`, `trees\campaign.rigid_trees`, `arrows/tradenodes *.rigid_model`, `detail/fogmask/snow/snowmask *.dds` and `README.txt`.
- `data\all_scripted.lua` (396 B) and `data\battle_scripted.lua` (116 B) are plain source and contain only `require` glue.
- `data\UI\` holds `Campaign UI\Pips\*.tga`, `Cursors\*.cur/.ani` (179) and `Templates\` (1). It contains no layouts.

### ESF in packs (CONFIRMED, `esf-scan-all`)
- No pack entry has the `.esf` extension.
- 440 pack entries have the ABCE magic: `data.pack` 4 and `battleterrain.pack` 436. Their extensions are `.tree_list`, `.building_list`, `.farm_manager` and `.farm_template_tile`. They use the same container (e.g. root `BATTLEFIELD_BUILDING_LIST`, `TREE_LOD_LIST`, `ROOT_FARM_MANAGER`). These are battle-map assets, so they belong to Worker 2's battle-terrain scope.

### Lua in packs (CONFIRMED)
- `data.pack` root holds 10 source `.lua` files: `episodicscripting`, `events`, `export_advice`, `export_ancillaries`, `export_historic_characters`, `export_historic_events`, `export_missions`, `export_triggers`, `scripting_library`, `scripting_library_wellington`.
- 24 `.battle_script` files are Lua source (historical battles, tutorials, profiling, test).
- 497 `.luac` files under `ui\` are Lua 5.1 bytecode (see section 6).
- The 22 `sound.pack *.script` files are **not** Lua. They are pack-builder scripts (`create ... add_directory ... build;`).

### UI layouts (CONFIRMED)
- 174 extensionless binary layouts under `ui\{campaign ui, battle ui, common ui, frontend ui, templates}` in data.pack and boot.pack.
- There is one text layout, `ui\templates\post_battle_entry.twui`, and one `.twui.images`.

### Save games (CONFIRMED)
- `%APPDATA%\The Creative Assembly\Napoleon\save_games\` holds 8 `.save` files (28 to 37 MB). They are all "Great Britain" in `mp_eur_napoleon`, dated Sep 1805 to Apr 1806, plus `auto_save` and `quick_save`. `save_games_multiplayer\` is empty.
- Other AppData files (not analysed): `scripts\preferences.script.txt`, `user.script.txt`, `custom_keys.keys.xml`, `advice_history\advice.advice`, `battle_preferences\`, `fx_cache\*.fxc`.

---

## 2. ESF format spec (ABCE variant), byte-exact

The spec is CONFIRMED by a full parse of all 33 loose ESF files, all 8 saves and sampled ABCE pack entries. For every file the parse ends exactly at the name table, and every block end offset matches. All integers are little-endian.

### 2.1 File layout
```
0x00  u32  magic            = 0x0000ABCE   (bytes CE AB 00 00)
0x04  u32  unknown          = 0 in every file seen (UNKNOWN meaning)
0x08  u32  timestamp        unix seconds (e.g. 0x4B27B413 = 2009-12-15 for eur startpos;
                            saves 0x6A38xxxx = 2026, i.e. the time of saving)
0x0C  u32  names_offset     absolute offset of the node-name table
0x10  ...  root node        always a record node (type 0x80)
names_offset:
      u16  name_count
      name_count x { u16 len; len bytes ASCII }     -- record names, indexed from 0
EOF   (0 trailing bytes in every file)
```
The root record ends exactly at `names_offset` (CONFIRMED). The root record's own name index is 0.

### 2.2 Node encoding (one type byte, then a payload)
| code | type | payload | seen |
|---|---|---|---|
| 0x01 | bool | u8 (0/1) | CONFIRMED |
| 0x02 | i8 | 1 byte | INFERRED (scalar never seen) |
| 0x03 | i16 | 2 bytes | INFERRED (scalar never seen) |
| 0x04 | i32 | 4 | CONFIRMED |
| 0x05 | i64 | 8 | INFERRED (never seen) |
| 0x06 | u8 | 1 | CONFIRMED |
| 0x07 | u16 | 2 | CONFIRMED |
| 0x08 | u32 | 4 | CONFIRMED |
| 0x09 | u64 | 8 | INFERRED (never seen) |
| 0x0A | f32 | 4 | CONFIRMED |
| 0x0B | f64 | 8 | INFERRED (never seen) |
| 0x0C | coord2d | 2 x f32 (x, z on the map) | CONFIRMED |
| 0x0D | coord3d | 3 x f32 (x, y = height, z) | CONFIRMED |
| 0x0E | UTF-16 string | u16 **char** count, then count x u16 (UTF-16LE) | CONFIRMED (`0e 1d 00` + 29 chars at eur startpos 0x20) |
| 0x0F | ASCII string | u16 byte count, then bytes | CONFIRMED |
| 0x10 | angle | u16 (INFERRED: fraction of a full turn, 65536 = 360 deg) | CONFIRMED size |
| 0x40 + t | packed array of primitive t | u32 **absolute end offset**, then elements back-to-back until end | CONFIRMED for t = 01,02,03,04,06,07,08,0A,0C,0D (i.e. 0x41,42,43,44,46,47,48,4A,4C,4D) |
| 0x80 | record | u16 name_index, u8 version, u32 absolute end offset, then child nodes until end | CONFIRMED |
| 0x81 | record array | u16 name_index, u8 version, u32 absolute end offset, u32 item_count, then item_count x { u32 absolute item end offset, child nodes until that offset } | CONFIRMED |

Notes:
- Arrays 0x45, 0x49, 0x4B, 0x4E, 0x4F and 0x50 never occur. A reader should still accept them: string arrays would be repeated length-prefixed strings until the end offset. The element size of i16[] is confirmed by `pathfinding grid_data`, where the u16 count of 72 matches `i16[72]` = 144 bytes.
- All offsets are absolute file offsets. A writer must therefore back-patch end offsets after writing children. No sizes are relative.
- **Records carry a per-record `version` byte.** The same record name has different layouts in different versions. Examples: `CHARACTER` is v12 in eur startpos and v14 in the 1.3 saves; `SETTLEMENT` is v2 vs v3; `SAVE_GAME_HEADER` v1 (ita/mp_ita startpos) has **no** `DATE` child while v2 does; `CAI_BDI_WAR` is v1 vs v2. A 1:1 loader must branch on the version (CONFIRMED by `esf-paths` diff).
- **Optional children.** Records with the same name and version can have different child signatures (e.g. `FACTION` nsig=2, `CHARACTER_POST` may contain `GOVERNORSHIP`, `BUILDING_MANAGER` = `bool, {BUILDING}, bool` or `bool, bool`). The pattern is a bool flag followed by an optional record (INFERRED). Readers must match children by name and order, not by fixed position.
- Records are addressed by order. There are no field names, only record names.

### 2.3 Reader
`campaign_tools\src\esf.rs` (about 200 lines) is a complete std-only reader. It loads the file into memory (32 MB maximum here), builds the tree (`Item::{V, Rec, RecArr}`) and validates every end offset. Parsing all 49 files takes about 5 s in a release build.

### 2.4 Coordinates (CONFIRMED)
Logical positions in the campaign model are stored as **i32 fixed point with 20 fractional bits**: `world = raw / 1048576`.
- Paris settlement `SIEGEABLE_GARRISON_RESIDENCE` holds (-222517056, 2406541), which is (-212.2088, 2.29506).
- `regions.esf settlement_and_slots` stores Paris as coord2d (-212.2088, 2.2950563), exactly the same value.
- The pathfinding grid origin (-429916160, -199229440) is (-410, -190), which matches the sea_grids bounds.
- Scripts address the same space: `SetCameraTargetInstant(-215, -2)` and `show_message_event(..., -211, -3)` for Paris.

Map bounds are (-640, -320)..(640, 320) for Europe (`EPISODIC_RESTRICTIONS` and `region_data`) and (-320, -320)..(320, 320) for the smaller theatres.

---

## 3. Start position contents

The root is `CAMPAIGN_STARTPOS` v5 (CONFIRMED). A startpos is a full campaign save plus one extra block, `CAMPAIGN_PREOPEN_MAP_INFO`, used by the front end.

```
CAMPAIGN_STARTPOS v5
  BUILD v0                 utf16 build-id, utf16 version ("v2.0.0  Build ?.?" in shipped startpos;
                           spa: "Napoleon: Total War 1.3.0 (Release; Build 1630 ...)")
  SAVE_GAME_HEADER v2      faction key, portrait path, u32 turn_number(1-based), u32 year,
                           utf16 season name, flag path, {DATE}, [MAPS: utf16 name, u32 w, u32 h,
                           i32 (=w*4, row stride), u32[w*h] ARGB preview image]
  CAMPAIGN_PREOPEN_MAP_INFO v4   (startpos only)
     {CAMPAIGN_PLAYERS_SETUP}[PLAYERS_ARRAY], {DATE}, campaign key, map key,
     [VICTORY_CONDITION_OPTIONS: playable faction -> 3 x CAMPAIGN_VICTORY_CONDITIONS],
     [FACTION_INFOS: key, portrait, bool, bool, i32, description loc key, flag path, i32],
     [REGION_OWNERSHIPS_BY_THEATRE: theatre -> [region key, owner faction key]]
  CAMPAIGN_ENV v2: bool, bool, CAMPAIGN_SETUP v3, CAMPAIGN_SETUP_LOCAL, CAMPAIGN_CAMERA_MANAGER, CAMPAIGN_MODEL v10
```

`CAMPAIGN_MODEL` v10 children in order (CONFIRMED):
`CAMPAIGN_MAP_DATA`, u32 (= `regions.esf` timestamp, a map-version link, CONFIRMED equal to 1260299822 for Europe), `RandSeed`, `CAMPAIGN_CALENDAR`, `WORLD`, bool, `LOCOMOTION_MANAGER`, `PENDING_BATTLE`, `HISTORICAL_EVENT_MANAGER`, `HISTORICAL_CHARACTER_MANAGER`, `CAI_INTERFACE` (campaign AI, the largest block at about 18 MB), `CAMPAIGN_TRADE_MANAGER`, `MARKER_MANAGER`, `PORTRAIT_ALLOCATOR[]`, `CAMPAIGN_PATHFINDER` (runtime pathfinding grid plus obstacles), `EPISODIC_RESTRICTIONS` v17 (script state, including `LUA[]`), `CULTURE_UNIT_CLASS_MAPPING[]`, 3 bools, `OSMOSIS_CULTURES[]`, u32, f32, u32, u32, `TURN_TIMER`, `CAMPAIGN_DIRECTOR[]`, bool.

The full schema dump (877 record paths for eur) can be regenerated with `esf-summary <file> 30`. It is not saved, to avoid shipping a dump.

### 3.1 Calendar and date (CONFIRMED by save filenames)
`DATE` = { u32 year, u32 season, u32 month (0 = Jan), u32 half (0 = "Early", 2 = "Late") }.
- "Early April 1806" is stored as 1806/2/3/0.
- "Late December 1805" is stored as 1805/1/11/2.

Season codes come from the header string: 0 = Summer, 1 = Winter, 2 = Spring, 3 = Autumn. One anomaly: the save "Early September 1805" has season 2 "Spring" (INFERRED: the season is computed by a separate rule, UNKNOWN).

`CAMPAIGN_CALENDAR` = { u32 turns_per_year = 24, u32 turn_in_year = month*2 + (half==2), {DATE}, u32 turns_elapsed } (CONFIRMED across the 8 saves, e.g. Early Apr has index 6). `SAVE_GAME_HEADER` u32 turn_number = turns_elapsed + 1 (CONFIRMED).

### 3.2 Campaign settings
`CAMPAIGN_SETUP` = { key, PLAYERS_SETUP, CAMPAIGN_SETUP_OPTIONS v4 {f32 1.0, bool, INGAME_MODIFIABLES{u32, u32, i32, i32, 5 bools}, i32, i32, u32 (1 in startpos; 4095 in the MP save; 1023 spa), u32 20, u32 14}, u32 (51914 in all startpos; UNKNOWN), utf16, bool }.
- In the MP save, `INGAME_MODIFIABLES` holds 3600 (INFERRED: turn timer, in seconds).
- `CAMPAIGN_PLAYER_SETUP` = {CAMPAIGN_VICTORY_CONDITIONS, INGAME_MODIFIABLES{i32 x3, bool}, faction key, bool, bool, bool}. The second bool is 1 for playable factions and the first is 1 for the human (britain in the save) (INFERRED).
- `CAMPAIGN_VICTORY_CONDITIONS` v5 = {[REGION_KEYS], bool, u32 24, {DATE deadline}, u32 regions_required, bool, u32 kind(1/3/4/5), bool, bool, u32, {DATE}} (INFERRED field meaning).
  - Example, France in eur: the long campaign needs the regions [brandenburg, moscow, east_prussia, france, austria] plus 35 regions by 1812/1/11/2.
  - The short variant needs 18 regions. The "prestige" variant needs eur_france plus 60.

### 3.3 Factions (eur_napoleon, CONFIRMED values; field meaning INFERRED)
There are 41 `FACTION` v18 records plus the `REBEL_FACTION`.
- Leading values: i32 **object id** (cross-reference key used by diplomacy and armies, e.g. france = 749327284; INFERRED to be serialized pointers or handles), key, display name.
- Treasury is the first scalar of `FACTION_ECONOMICS`. Start values: france 6500, austria 7500, britain 7000, prussia 6000, russia 5000, ottomans 7000, spain 6000, most minors 5000, dormant/emergent factions 0.
- Other faction blocks:
  - `GOVERNMENT`: posts plus a `GOV_IMP` record named `GOVERNMENT::ABSOLUTE_MONARCHY` / `CONSTITUTIONAL_MONARCHY` / `REPUBLIC`. britain is constitutional; netherlands and ita_venice are republics.
  - `FAMILY` (monarchy characters), `FACTION_TECHNOLOGY_MANAGER` with `techs[]` = {key, u32 state, f32, u32, u32[], u32}. Tech state values are 0, 2 and 4 (INFERRED: 0 = researched, 2 = researchable, 4 = locked).
  - `PRESTIGE`, `FACTION_FLAG_AND_COLOURS` (flag path plus 3 RGB triples), `CAMPAIGN_BONUS_VALUES`, unit-name allocators, `CHARACTER_ARRAY`, `ARMY_ARRAY`, the religion string (`rel_catholic`, ...).
- Playables (5 in eur): france, austria, britain, prussia, russia (`PLAYERS_ARRAY` flags).
- Region ownership at start (`REGION_OWNERSHIPS`): france 12, russia 12, austria 9, prussia 7, britain 5, ottomans 5, spain 4, and 1 to 2 each for 16 minors. That is 72 land regions.

### 3.4 Diplomacy (CONFIRMED structure)
Each faction has `DIPLOMACY_MANAGER/DIPLOMACY_RELATIONSHIPS_ARRAY` with one `DIPLOMACY_RELATIONSHIP` v14 per other faction (41 x 40 = 1640).
- Fields: i32 target faction id, `[ATTITUDES_ARRAY]` (24 entries of i32, i32, i32, bool, i32, bool), bool, i32, utf16 **stance** ("neutral" / "war" / "allied" / "protectorate" / "patron"), ..., `[REGULAR_PAYMENTS]`, `[ALLIED_IN_WAR_AGAINST]`, u32[14], u32, utf16 stance again (INFERRED: previous stance), ..., i32 (e.g. -45; INFERRED: relation score).
- eur start: 60 allied, 78 war, 1502 neutral directed pairs. France is at war with britain, austria, russia, sweden, naples and sicily, and allied to spain, bavaria, italy_kingdom, netherlands, wurttemberg and swiss_confederation (the Third Coalition).

### 3.5 Characters (CONFIRMED structure)
`CHARACTER` (v12 startpos, v14 save) = {LOCOMOTABLE, CHARACTER_DETAILS, i32 id, utf16 type, u32 force_id, u32 commanding_unit_id, ...}.
- Types seen: General, colonel, admiral, captain, minister, gentleman, rake.
- `LOCOMOTABLE` = {i32 x, i32 z, i32 x2, i32 z2 (fixed 2^20), f32 x4, i32, i32 (INFERRED: current/max movement-type points; values 25/27 colonel, 26/30 general, 90/66 admiral; UNKNOWN exact), bool, i32, i32, angle, u32, u32, f32, u32[], u32[]}. Ministers are at (0, 0) because they are off-map.
- `CHARACTER_DETAILS` = {TRAITS[TRAIT: key, i32 points], forename / surname / title localisation keys (`names_name_names_french...`), utf16, DATE birth, DATE, i32, PORTRAIT_DETAILS, faction key, u32 id, [AgentAncillaries: key], [AgentAttributes: key, i32] x14, [AgentAbilities: key, i32, utf16] x12, [AgentAttributeBonuses], bool, onscreen-name loc, utf16}.
- Example: Napoléon Bonaparte, born 1769. Traits C_Napoleon_Career_Traveller=1, C_General_Press_Hero=12, C_General_of_Artillery=16, C_NTW_General_Grand_Armee=1, C_General_Good_Field_Commander=12, C_General_Brave=4. Ancillaries Ancillary_Army_ADC and Anc_Balloonist.
- Totals in eur: 531 characters. 29 historical characters are pre-created (`HISTORICAL_CHARACTER_MANAGER/CREATED_CHARACTER_ARRAY`).

### 3.6 Armies and navies (CONFIRMED structure)
An `ARMY_ARRAY` item contains either `ARMY` v2 {MILITARY_FORCE{u32 force_id, u32 commander_character_id, u32[]}, [UNITS_ARRAY: LAND_UNIT], ...} or `NAVY` v1 (same plus `THEATRE_TRANSITION_INFO`).
- `LAND_UNIT` = {LAND_RECORD_KEY, UNIT v3 {UNIT_RECORD_KEY, UNIT_HISTORY{DATE raised, u32, u32}, COMMANDER_DETAILS{forename, surname, faction}, TRAITS, i32 unit_id, u32 men, u32 max_men, i32 (UNKNOWN: 44 general, 41 cavalry, 32 line infantry, 27 militia), ..., regiment-name loc, i32, f32}, u32}.
- `NAVAL_UNIT` adds `SHIP_DAMAGE_INFO`.
- eur totals: 69 armies, 18 navies, 441 units. France has 12 armies and 2 navies (51 units). Napoleon's stack is Gen_Late_Napoleon, 2x Chasseurs, 2x 12-lber, Old Guard, Grenadiers, Fusiliers, ...

### 3.7 Regions and settlements (CONFIRMED structure)
`REGION` v5 has 72 entries.
- Fields: key, POPULATION (classes, religion breakdown, totals), TRAITS, REGION_SLOT_MANAGER ([REGION_SLOT] + ROAD_SLOT + FORTIFICATION_SLOT; each slot has an optional `BUILDING` {u32 health 100, building level key, faction, government}), i32, SETTLEMENT v2/v3 {SIEGEABLE_GARRISON_RESIDENCE (position i32 fixed), loc key, i32, "settlement:<region>:<town>", i32, ...}, theatre "europe", rebel faction key and name, climate "sc_european_west", recruitment manager, resources (`RESOURCES_ARRAY`: "global", "horses", ...).
- Example: eur_france (Paris) has a population of 17,386,000 and the buildings sAdmin2_magistrate, sArmy2_barracks, sCannon2_ordnance_factory, sCulture1_Theatre, rTimber1, rHorse1, rIron1, tEducation1, ...

### 3.8 Other startpos facts (CONFIRMED)
| campaign | factions | regions | chars | armies / navies / units | start date | map |
|---|---|---|---|---|---|---|
| eur | 41 | 72 | 531 | 69 / 18 / 441 | Early Jan 1805 | nap_europe |
| mp_eur | 41 | 72 | 539 | 69 / 18 / 440 | Early Jan 1805 | nap_europe |
| egy | 5 | 30 | 113 | 14 / 1 / 97 | Early Jun 1798 | nap_egypt |
| ita | 12 | 25 | 166 | 33 / 0 / 137 | Early Apr 1796 | nap_italy |
| spa | 4 | 31 | 135 | 42 / 6 / 198 | Late Mar 1811 | nap_spain |
| tut | 4 | 8 | 82 | 2 / 2 / 6 | Early Jan 1778 | nap_tut |

`CAMPAIGN_TRADE_MANAGER` (eur) has 8 commodities (spices, tobacco, sugar, ivory, tea, cotton, coffee, furs), 20 resources, 39 ports, 766 trade segments, 1653 routes and 19 trade nodes.

---

## 4. Save game structure

The root is `CAMPAIGN_SAVE_GAME` v5 = {BUILD, SAVE_GAME_HEADER, CAMPAIGN_ENV}. It uses the same container and the same `CAMPAIGN_ENV` tree as the startpos. It has **no** `CAMPAIGN_PREOPEN_MAP_INFO` (CONFIRMED). The BUILD string is "Napoleon: Total War 1.3.0 (Final Release; Build 2081 (Curator); 12/05/2023 20:00) Changelist: 2931570". The name table has 691 names vs 631 in the startpos. The save is 36.2 MB, about 300k records.

Extra runtime state present in saves but absent from the startpos (record paths only in the save, CONFIRMED by `esf-paths` diff):
- **Pending and last battle:** `PENDING_BATTLE/BATTLE_SETUP` (alliances, armies, units with `LAND_UNIT_RECORD::STATS` and `UNIT_CAPABILITIES`, deployment areas, `BATTLE_SETUP_VICTORY_CONDITION_*`, `AUTO_GENERATOR_INPUT` with battlefield building list and `META_TERRAIN_GENERATOR_INPUT`), `BATTLE_RESULTS` (per-alliance, per-army, per-unit results), `PENDING_BATTLE_ALLIANCE/FACTION/PARTICIPANT`, reinforcements and `PLAYER_LIST/PLAYER_SETUP`. A save can therefore be made with a battle pending.
- **Queues:** `BUILDING_CONSTRUCTION_ITEM` {u32, bool, u32, u32, u32, building key} in region, road and fort slots; `RECRUITMENT_ITEM` (land, in `REGION_RECRUITMENT_MANAGER`; naval, in port slots) {i32 x3, u32, u32, bool, unit key, u32, u32, bool, i32, u32}.
- **Espionage:** `CAMPAIGN_SPYING/FORCE_DATA_BLOCK/MILITARY_FORCE_DATA/UNIT_DATA` (cached intel about other factions' forces).
- **Siege damage:** `FORTIFICATION_DAMAGE_INFO/HIT_POINTS_BLOCK`.
- **Rebels:** characters in `REBEL_FACTION`.
- **AI state:** many additional `CAI_*` belief, desire, intention and failure records (`CAI_HIGH_LEVEL_PATH`, `CAI_BDI_RESERVED_NAVY`, `CAI_BDI_FAILURE_INFO` with a `CAMPAIGN_CALENDAR`, `CAI_WORLD_TRADE_ROUTES` items, ...). Persisting the campaign AI's BDI pools is part of the save.
- **Scripting state:**
  - `EPISODIC_RESTRICTIONS/LUA[]` holds one positional value per `save_value` call. The mp_eur script calls `save_value` 13 times and the save has 13 items, with i32 for numeric and bool for boolean variables (CONFIRMED).
  - `EPISODIC_RESTRICTIONS/BUILDING_RESTRICTIONS[]` holds 55 entries (building keys added via `add_restricted_building_level_record`). Unit, visibility, location and time triggers, attack-of-opportunity, markers and resistance-immunity lists are all stored there.
- **Other changes:** camera position and zoom in `CAMPAIGN_CAMERA`; `CAMPAIGN_ENV` first bool = false (true in the startpos; INFERRED "fresh start" flag); `HISTORICAL_CHARACTER_MANAGER` grows (84 created); the mission manager exists only for the human faction.

Record versions differ between the shipped startpos files and 1.3 saves (`CHARACTER` 12 to 14, `SETTLEMENT` 2 to 3, `CAI_BDI_WAR` 1 to 2). The 1.3 engine therefore loads older record versions and writes the newest (CONFIRMED that both exist; the upgrade logic is UNKNOWN).

---

## 5. Campaign map files

| file | format | contents (CONFIRMED structure, INFERRED meaning) |
|---|---|---|
| `regions.esf` | ESF root `root` | `theatres_and_region_keys` (theatre name, bounds, region key + label position); `theatres` (bounds, 2 x `climate_map` {u32 w=2344, u32 h=1203, u8[w*h] climate index: land and sea}, `wind_map` {300 x 154, f32 scale, i8[2*w*h] vectors}, transition_areas); `region_data` {shared `vertices` coord2d[], bounds, 101 `regions` (key, ascii "land"/"sea", bbox, `areas[]` {flags, bbox, u16, `faces` u32[] triangle indices, `outlines[]` {coord2d bbox, u32[] vertex loop, `connectivity[]` (u32 x3: neighbour links)}}, i32 settlement flag, `settlement_and_slots` {settlement pos, coord2d[] footprint, `slot_descriptions[]` v1 {slot key "settlement:eur_france:paris:settlement_4_slot:0", slot type, coord3d, coord2d, angle, bool, utf16, 3 x coord2d[] footprints}, roads/railways/canals links}}; `mountain_data`, `climate_indices` (lc_* / sc_* name to u8), `query_info` (quadtree for point-to-region lookup), `groundtypes` (8 ground types: grassland, light_forest, hills, ... polygons + quadtree), `map_heights`, `trade_nodes` (19 positions), `bridges` coord3d[] |
| `pathfinding.esf` | ESF | `pathfinding_areas[]` {`vertices[]` (i32 x, i32 z fixed 2^20; 93,380), u32[] (369,994 indices), `grid_data` {origin i32 x2 (-410, -190), u16 115, u16 65, i32 cell = 2.0 units, u32 375, u32 193 (fine grid 375 x 193 x 2.0 = 750 x 386 units = theatre bounds), u32, u16 x2, i16[72], u16[540], `grid_cells[]` (22,242) {u8[], `boundaries[]`, u32, u16, u8[]}}}. This is the movement graph source; the startpos also contains a runtime `CAMPAIGN_PATHFINDER` copy with obstacles. |
| `sea_grids.esf` | ESF root `CAI_SEA_GRID_ROOT`, flat values | u32 1, theatre id, bounds (-410, -190)..(340, 195), f32 75 (cell size), u32 11 x u32 6 cells, then per cell {bounds, id, ...}. Used by the AI naval planner (`CAI_WORLD_SEA_GRID_CELLS` = 66 = 11 x 6). |
| `poi.esf` | ESF root `CAI_POI_ROOT`, flat values | u32 5513 = count of AI points of interest, each starting with i32 type, bool, i32 x, i32 z (fixed), strings, ... (UNKNOWN full record) |
| `trade_routes.esf` | ESF | `PORTS[]` (39 keys), `SETTLEMENTS[]` (72), `SPLINES[]` (766 polylines + bool), `ROUTES[]` (912: u32 from, u32 to, u32[] spline ids, f32 length), `TRADE_NODE_ROUTES[]` (20). These match `CAMPAIGN_TRADE_MANAGER` counts. |
| `metadata.dat` | `"MTGI"` + u32 w + u32 h, then w*h u8 | eur 2048 x 1024, ita/spa 2048 x 2048, egy 4096 x 4096 (size = 12 + w*h, CONFIRMED). Values form a small enum (eur: 0, 1, 6; ita: 0 to 8; INFERRED: per-texel terrain/ground-type class; UNKNOWN exact meaning). |
| `<theatre>_lookup.tga` | TGA type 1 (colour-mapped), 256 x 24-bit palette, 8 bpp, same size as the preview (eur 605 x 300) | 100 distinct indices for 101 regions (INFERRED: radar/minimap pixel to region index) |
| `<theatre>_map.tga`, `stratradar_*.tga` | TGA type 2, 24/32 bpp | minimap and strategic radar images |
| `display\heightmap\heightmap.tga` | TGA type 3, 8-bit grey, 4096 x 2048 (eur) | terrain heights |
| `display\supertexture\*.stpi/.stpd` | stpi: u32 6, u32 32768, u32 16384, u32 512, ... (INFERRED: mip count, virtual texture 32768 x 16384, 512 tiles); stpd: repeated {u32 compressed size, u32 32768 raw size, zlib stream `78 DA`} | terrain virtual texture (INFERRED) |
| `*.rigid_spline` | magic `"SPLN"`, u32 1, u32 1, UTF-16 name ("border:eur_austria:1"), ... | border, road, river and trade-route render splines |
| `world.markers`, `bridge.markers` | u16-length UTF-16 strings ("BASE_MARKER_REPOSITORY", "bridge:eur_portugal:4"), then floats | placed markers |
| `*.lighting` | XML `<SCENE_ENVIRONMENT>` | per-season lighting |
| `*.rigid_mesh`, `*.rigid_model` | magic 0x12345678 + version 5 | 3D assets (Worker 2's model scope) |
| `*.rigid_trees` | magic `"G@M="` | campaign tree placement |

Region adjacency (needed for movement and AI) is in `regions.esf outlines/connectivity` and in the startpos `CAI_WORLD_REGION_BOUNDARIES` (259 boundaries) and `CAI_WORLD_REGION_HLCIS`. Ports are `PORT_INDICES` and `PORT_GARRISON_MANAGER` slots. Sea zones are the `ascii "sea"` regions in `region_data` plus `sea_grids`.

---

## 6. Lua scripting API and event list

### 6.1 Formats (CONFIRMED)
- **Source:** all campaign scripts, `episodicscripting.lua`, `events.lua`, `export_*.lua`, `scripting_library*.lua` and all `.battle_script` files.
- **Bytecode:** the 497 UI `.luac` files. Header `1B 4C 75 61 | 51 | 00 | 01 | 04 04 04 04 | 00` means Lua **5.1**, official format, little-endian, `int` 4, `size_t` 4, Instruction 4, **`lua_Number` 4 bytes with integral flag 0, i.e. lua_Number = float (f32)**. All 497 files parse with 0 trailing bytes (`src\luac.rs`). Source names are `@s:/branches/napoleon/curator/napoleon/working_data/UI/...`.
- A 1:1 Lua VM must use **f32 numbers**, which affects arithmetic in scripts.

### 6.2 Architecture (CONFIRMED from source)
- `events.lua` is "Automatically generated via export from Empire.mdb". It declares 168 unique global event tables (`FactionTurnStart = {}` ...). The engine fires an event by calling every function in `events.<Name>` with a single `context` userdata.
- `all_scripted.lua` requires export_triggers, export_ancillaries, export_historic_characters and export_missions, and sets `events = triggers.events`. `battle_scripted.lua` requires all_scripted and export_advice.
- `episodicscripting.lua` (module `EpisodicScripting`, `package.path = ";?.lua;data/ui/templates/?.lua;data/ui/?.lua"`):
  - On `NewSession` it does `game_interface = GAME(context)`.
  - It provides `AddEventCallBack(event, fn, user_defined)`, `ClearEventCallbacks`, `SetCampaign`, `EnableFeature/DisableFeature/InitFeature` (a table of about 60 named UI and gameplay features), `HideComponent/RevealComponent/HighlightComponent`, and saves and restores the UI hierarchy (`m_root:SaveUIHeirarchy()/RestoreUIHeirarchy`).
  - It has a per-campaign `m_starting_configuration`: Features, Hidden and Disabled UI components, restricted Buildings and Units, Intro_movie, exclusion zones, triggers and model overrides. These are applied on `UICreated("Campaign UI")` if `game_interface:is_new_game()`.
- Each `campaigns\<c>\scripting.lua` does `require "EpisodicScripting"` and `scripting.SetCampaign("<c>")`, then registers handlers via `scripting.AddEventCallBack`.
- Engine-provided globals (not defined in any script): `GAME`, `UIComponent`, `Component.*`, `CampaignUI.*`, `FrontEnd.*`, `BattleUI.*`, `UIImage`, `mp_interface`, `conditions.*`, `effect.*`, `out.*`, `empire_battle`, `battle_vector`, `bit`, `CoreUtils`.

### 6.3 API surface (full list in `analysis\worker3\lua_api.txt`, 5,444 lines with counts and argc histograms)
- **`game_interface:` (campaign GAME object): 63 methods.** Most used: `force_diplomacy(5 args)` 714 times, `add_time_trigger(name, seconds)` 93, `save_value(v, ctx)` / `load_value(default, ctx)` 95 each, `other_income_mod(faction, amount)` 93, `show_message_event(key, x, z)` 78, `set_zoom_limit` 74, `trigger_custom_mission(13 args)` 67, `steal_user_input` 49, `add_location_trigger(x, z, r, faction)` 42, `add_marker(6)` 32, `treasury_mod` 28, `show_shroud` 25, `remove_time_trigger` 16, `remove_restricted_unit_record` 12, `set_map_bounds(4)` 10, `force_add_trait` 9, `unveil_black_shroud` 9, ... and also `force_make_peace / force_declare_war / force_make_protectorate / force_make_trade_agreement / force_rebellion_in_region / grant_faction_handover / grant_unit / award_experience_level / disable_saving_game / advance_to_next_campaign / register_instant_movie / register_outro_movie / set_tax_rate / exempt_region_from_tax / spawn_town_level / add_custom_battlefield / add_exclusion_zone / add_visibility_trigger / is_new_game / ...`.
  - `force_diplomacy` option strings: "trade agreement", "military access", "cancel military access", "alliance", "regions", "technology", "state_gift", "payments", "peace", "war", "protectorate".
- **`conditions.`: 193 predicates.** All take `context` last. Most used: CharacterType 524, CanGenerateHistoricalCharacter 505, CampaignName 444, CharacterHasAncillary 292, DateInRange 272, CharacterCultureType 243, IsComponentType 198, CharacterFactionName 159, BattleIsLandConflict 141, MissionName 106, TurnNumber 79, ...
- **`effect.`: 12 actions.** trait(key, "agent", points, chance%, ctx) 555, historical_character 505, advance_contextual_advice_thread 428, ancillary(key, chance%, ctx) 272, advance_scripted_advice_thread 122, trigger_mission / mission_success / mission_failure / mission_cancel 39 each, historical_event 8, remove_ancillary, rewind_scripted_advice.
- **`CampaignUI.`:** 14 in campaign scripts (ScrollCamera, SetCameraTargetInstant, SetCameraZoom, StopCamera, CameraTarget, IsMultiplayer, DismissAdvice, Highlight*Item, ...). The UI bytecode calls 269 distinct `CampaignUI.*`, 161 `BattleUI.*`, 107 `FrontEnd.*` and 18 `Component.*` functions. Worker 1's `script_bindings.tsv` independently recovered 499 such C bindings with descriptions; the two lists should be merged.
- **Battle (`battle = empire_battle:new()`):** 30 `battle:` methods (register_singleshot_timer 192, unregister_timer 203, register_repeating_timer 179, out 387, show_advisor_message 97, ui_component 91, alliances, buildings, camera, register_battle_phase_handler, ...). There are 159 distinct methods on army, unit, controller and vector objects (`alliances():item(i):armies():item(j):units()`, `create_unit_controller`, `add_units`, `take_control`, `goto_location`, `morale_behavior_fearless`, `battle_vector:new():set(x, y, z)`, `position():get_x()`, `number_of_men_alive`, `is_routing`, ...).
- **UI component methods** (`UIComponent(addr):X`): Find, SetVisible, SetState, SetStateText, LuaCall, Parent, Address, Position, SetEventCallback, MoveTo, Resize, SetTooltipText, CurrentState, SetDisabled, Highlight, Adopt, Divorce, DestroyChildren, ChildCount, ... (210 distinct method names in the bytecode).
- **`context` fields read in scripts:** only `context.string` (98) and `context.component` (1). Everything else is obtained through `conditions.X(context)`.

### 6.4 Events
The full table is in `lua_api.txt`, section "EVENTS". 168 names are declared. Events with script registrations (count) include:
- HistoricalCharacters 505, CharacterTurnEnd 116, CharacterCompletedBattle 92, BattleConflictPhaseCommenced 91, FactionTurnStart 73, BuildingCompleted 53, ComponentLClickUp 52, CharacterCreated 47, BattleUnitAttacksEnemyUnit 45, BattleDeploymentPhaseCommenced 26, CharacterSelected 23, PanelAdviceRequestedCampaign 22, PanelOpenedCampaign 21, ResearchCompleted 19, RegionTurnStart 17, BattleShipAttacksEnemyShip 16, CharacterPromoted 15, DummyEvent 15, CharacterFactionCompletesResearch 13, AdviceDismissed 11, DuelFought 11, SufferAssassinationAttempt 10, EspionageAgentApprehended 10, SlotSelected 10, UICreated 9, SlotRoundStart 8, HistoricalEvents 8, SettlementOccupied 7, SlotTurnStart 7, TimeTrigger 6, MissionSucceeded 6, WorldCreated 6, CameraMoverFinished/Cancelled 6, AdviceIssued 6, IncomingMessage 6, SavingGame 5, LoadingGame 5, NewCampaignStarted 5, BattleCompleted 5, ...
- 129 of the 168 declared events are registered at least once.
- The 26 MissionCheck* and MissionEvaluate* events come from `export_missions`.

---

## 7. What the campaign scripts do

- **Generated rule tables** (from the Empire.mdb export; each handler is "if conditions then effect; return true"):
  - `export_triggers.lua`: 149 trait triggers (e.g. C_Admiral_Bad on CharacterCompletedBattle when not won; `effect.trait(key, "agent", points, chance)`).
  - `export_ancillaries.lua`: 272 ancillary triggers (e.g. Anc_Balloonist for French generals with command_land >= 6, gated by `DateInRange(1796, 1900)`).
  - `export_historic_characters.lua`: 505 `CanGenerateHistoricalCharacter` to `effect.historical_character` spawns.
  - `export_historic_events.lua`: 8 events (`IsTriggerableHistoricalEvent(turn?, key, year, "season_winter")`).
  - `export_missions.lua`: 31 AI-generated mission check and evaluate hooks. These include bogus tables `events.n` and `events.evaluate`, which are export glitches.
  - `export_advice.lua`: 475 advisor triggers.
  - A reimplementation can run these files unchanged if the `conditions` and `effect` tables are provided.
- **Episodic missions** (`trigger_custom_mission(key, faction, type, 0, target, heading_loc, text_loc, reward_loc, 0, "", context, bool, reward...)`). Mission types: capture_city, build_building, recruit_unit, research_technology, forge_alliance, blockade_port, make_peace, make_trade_agreement, infiltrate_garrison, restore_public_order, end_rebellion, liberate_region, sabotage_enemy_building. Rewards are strings such as `money:2000`, `grant_unit:<unit>#settlement:<key>`, `grant_agent:<type>#<region>`, `enable_recruitment:<unit>` and `grant_experience_army:1`.
  - **eur (France):** take Vienna, then Berlin (forces war on Prussia), Warsaw, Königsberg (countdown of 6 turns after Berlin), Moscow (on turn 48 or after fighting Russia 3 times), Madrid (if Portugal is an enemy), and build the Arc de Triomphe after 6 victories. Success triggers time-triggered message events and `force_make_peace(france, austria/prussia)`. VictoryConditionMet sets a flag; the outro movie NCS_08_Eur_Outro.bik plays on the event-movie panel close. There are 19 saved script variables.
  - **mp_eur (Coalition):** multiple-faction missions (Prussia alliances and research, Austria takes Stuttgart and researches poverty laws, Russia takes Finland and Bessarabia, Britain blockades Toulon), plus faction handover of bavaria and wurttemberg to france via `grant_faction_handover`. 13 saved variables.
  - **ita** (Italian campaign): establish supply, liberate Milan and Modena (unlocking new recruitables), take Mantua, defy the Pope (Ancona), invade Venice, victory at Klagenfurt. It also releases scripted Austrian armies ("release_wurmser", "release_alvinczi"). 17 saved variables.
  - **egy:** take Cairo, subdue the Bedouin, adapt to the desert (dromedary cavalry), Suez, crush the Cairo rebellion, take Jaffa, Acre and Damascus, destroy Britain (Cyprus), courier university. 16 saved variables.
  - **spa** (Peninsular, 4 playables): per-faction missions plus difficulty-based start cash (`treasury_mod` 4000/2000/1000) and per-turn AI and economy subsidies via `other_income_mod` (comment: "all factions starting on 500 other income, but need 3000 to break even"). There is extensive `force_diplomacy` locking (714 calls across campaigns) and markers. 30 saved variables.
  - **tut:** a linear tutorial of 13 missions (trade agreement, school, research, foundry, cannon, factory, public order, shipyard, 3rd rate, blockade Olbia, peace with Sardinia, infiltrate Toulon, capture Toulon), with stolen input, camera scrolls, highlighted components, `disable_saving_game(true)` and `disable_end_turn` features.
- **Victory:** the engine evaluates `CAMPAIGN_VICTORY_CONDITIONS` from the ESF and fires `VictoryConditionMet`. The scripts only react with movies and flags. spa's handler is commented out.

---

## 8. UI data format

- **Binary layout files** are extensionless, for example `ui\campaign ui\layout` (132 KB) or `ui\frontend ui\options` (430 KB).
  - The header is ASCII `"VersionNNN"` (10 bytes). Version counts: 039 x136, 033 x29, 032 x5, 029 x2, 030 x1, 028 x1. A reader must support multiple layout versions (CONFIRMED).
  - Then u32 `this` (editor object id), u16 len + ASCII component id (`"root"`), followed by fields (UNKNOWN binary order).
  - Length-prefixed ASCII strings are recognisable throughout: component ids (`veneer_DY`, `hud_left`, `radar`, `map`, `map_overlay`, ...), state names (`NewState`, ...), font specs ("Ingame 12, Normal"), shader names (`normal_t0`), image paths (`UI/Campaign UI/...tga`), attached scripts (`campaign_radar.lua`, `template.map_image.lua`), and event bindings as pairs `OnMouseLClickUp` with `OnLClickUp` (event to Lua function), terminated by the string **`events_end`** (CONFIRMED).
- **`.twui`** (one file) is a text Lua table with the same data, "Layout created with UIEd", `version = 39`. It reveals the field names of a v39 component:
  - Component fields: Visible, Docking, Highlight, RenderWhenDragged, Animations, script, EventScriptFunctions[30], ScriptFileNameOverride, children, UserProperties, offsetx, offsety, RenderIfRoot, ComponentImages[{ComponentVisible, Colour[4], Height, ImagePath, this, Width}], Priority, this, RenderLastOnFocused, AllowVertical/HorizontalResize, ClipChildren, ComponentLevelTooltip, id, Moveable, MaskImage, DefaultState, CreatedFromTemplate(+Version), UseGlobalClicks, DrawMode, TooltipLabel, CurrentState, and States{name → state}.
  - State fields: EditorComponentUniqueID[16], Width, Height, Text, TextLabel, TextLocalised, TextH/VBehaviour, TextH/VAlign, Fontfont, Fontcolour, Fontleading, Fonttracking, ShaderTechnique, ShaderVars[16], TextShaderVars[16], ImageMetrics[{Offsetx, Offsety, Width, Height, Colour, Tile, X/Y_Flipped, DockPoint, rotation_angle, pivot_point, CanResize*, ComponentImage}], Interactive, Disabled, PixelCollision, TransitionMap, Enter/ExitStateFunction, TooltipText, ZDepth, FocusType, Lighting.
  - This is the best available key for decoding the binary layout. A field-by-field binary spec is still UNKNOWN; the next step is to diff the binary layouts that share template ids.
- **Organisation:** 174 layouts. There are 51 campaign screens (layout, diplomacy_panel, government_screens, technology, region_details, character_information, building_browser, popup_pre_battle, popup_battle_results, event_message, load-save_game, ... and 17 `layout_*` event-message variants), 24 battle, 18 common, 40 front end and 40 templates.
  - Behaviour lives in the 497 `.luac` files in matching folders (`<screen>_scripts\*.luac`, `templates\*.luac`). These scripts call the engine via `UIComponent`, `Component`, `CampaignUI`, `BattleUI` and `FrontEnd`.
  - Fonts are `.cuf` in local_en.pack. Font categories are in `ui\fontcategories.fc`. Textures are tga/dds under the same `ui\` tree.

---

## 9. Implications for a 1:1 Rust reimplementation

1. **ESF I/O is the foundation.** Startpos, saves, map data and battle-terrain lists all use ABCE.
   - Implement a streaming reader and a back-patching writer, and keep the name table order (indices).
   - Preserve each record's `version` byte and write the version the 1.3 game writes, for example `CHARACTER` v14 and `SETTLEMENT` v3.
   - Support older versions on load, because shipped startpos files use older ones.
   - For round-trip fidelity, keep unknown fields as an ordered `Vec<Item>` per record ("lossless passthrough") until each record is fully mapped.
2. **The save must persist** the full `CAMPAIGN_MODEL`: world (factions, characters, armies, regions, slots, buildings), queues (construction and recruitment), diplomacy matrices, calendar and turn counter, RNG seed, pending battle setup and results, spying caches, fort damage, AI BDI pools (`CAI_INTERFACE`), pathfinder obstacles, the trade manager, script state (`EPISODIC_RESTRICTIONS` incl. `LUA[]` positional values), camera, and the header plus 605 x 300 ARGB preview.
   - Saves are 28 to 37 MB uncompressed.
   - Object identity is via 32-bit ids (faction, character, force, unit) that cross-reference between records. Our ECS needs a stable id to entity map serialized as these u32/i32 values.
3. **Numbers:**
   - Logical map positions are i32 fixed-point with 20 fractional bits. Renderer and region data use f32 in the same units.
   - The calendar is 24 turns per year with Early/Late halves of each month.
   - Lua numbers are f32.
4. **Scripting layer:**
   - Embed Lua 5.1 (e.g. `mlua` with the `lua51` feature, rebuilt with `LUA_NUMBER=float`; this will be a justified crate in the game, not in this tool).
   - Load `.luac` (5.1, 4-byte float), or decompile it.
   - Expose the `events` tables, `GAME(context)` with the 63 game_interface methods, the 193 `conditions` and 12 `effect` functions, the `CampaignUI`, `BattleUI`, `FrontEnd`, `Component` and `UIComponent` bindings, the `out` logger, and for battles `empire_battle` / `battle_vector` (30 `battle:` methods plus 159 methods on army, unit, controller and vector objects).
   - Fire events with a `context` userdata whose `string` and `component` fields work.
   - `save_value` and `load_value` must stream positional values into `LUA[]` during SavingGame and LoadingGame.
   - The unmodified shipped scripts are the conformance test (export_* rely on exact condition semantics).
5. **Map loader** needs:
   - region polygons, triangle faces and connectivity, plus the point-to-region quadtree (`regions.esf`);
   - settlement and slot positions and footprints;
   - climate and wind maps;
   - ground types plus `metadata.dat`;
   - the pathfinding grid (`pathfinding.esf`) or a rebuild of it;
   - trade route splines;
   - sea grids;
   - the heightmap TGA and supertexture (zlib tiles);
   - spline borders, roads and rivers;
   - lookup TGA for the radar.
   - The startpos `u32` map timestamp should be checked against `regions.esf`'s header timestamp.
6. **UI:** a layout reader for versions 028 to 039 (component tree, states, images, event-to-function bindings) plus a Lua-driven component model with the same component ids, because scripts look up components by id (`m_root:Find("button_tech")`).

---

## 10. Questions

**For Worker 1 (exe):**
1. Where are the ESF type-code dispatch and the record-version upgrade paths (e.g. CHARACTER v12 to v14)? Please confirm the scalar codes 0x02, 0x03, 0x05, 0x09 and 0x0B and any codes above 0x10.
2. What is the u32 at ESF header +0x04? Is it always 0?
3. Locate the `game_interface`, `conditions` and `effect` binding tables. `script_bindings.tsv` covers only the 3 UI registrars. We need the C signatures for the 63, 193 and 12 functions, especially `force_diplomacy` options, `trigger_custom_mission` parameter 4 and 9 ints, and the `effect.trait` points/chance semantics.
4. Is `save_value(number)` stored as i32 truncation or rounding? Can it store strings?
5. What is the meaning of the faction, character and force ids (pointers vs handles)? How are they regenerated on load?
6. How is the UI binary layout `VersionNNN` parsed (field order per version)?
7. What do `metadata.dat` ("MTGI") values mean? What is the `season` derivation (Sep → "Spring" anomaly)?

**For Worker 2 (data):**
1. Which DB tables give `CAMPAIGN_VICTORY_CONDITIONS` kinds, mission types/rewards, diplomacy option keys, `climate` and `ground_type` keys, tech state enums and building level keys?
2. Localisation keys used in ESF (`names_name_names_*`, `start_pos_settlements_onscreen_name_*`, `mission_text_text_*`, `start_pos_factions_description_*`): please confirm they resolve in the `.loc` files.
3. Do you have a format for `.rigid_spline`, `.rigid_mesh` (0x12345678 v5), `.rigid_trees` ("G@M=") and `.markers`? These are needed for the campaign map display.
4. The battle-terrain ABCE lists (`.tree_list`, `.building_list`, `.farm_manager`, `.farm_template_tile`) can be read with my `esf.rs`.

---

## 11. UNKNOWNs
- ESF header u32 at +4. Never-observed type codes (0x02/03/05/09/0B scalars; 0x45/49/4B/4E/4F/50 arrays) are implemented by the regular pattern only.
- The meaning of many positional scalars inside large records (CHARACTER, REGION, DIPLOMACY_RELATIONSHIP, FACTION tail, CAMPAIGN_MODEL tail, `CAMPAIGN_SETUP` u32 51914, LOCOMOTABLE i32 pair, the UNIT i32 = 27/32/41/44).
- `poi.esf` record layout and the `pathfinding.esf` grid_cells encoding (u8[] blobs, boundaries) and the 115 x 65 coarse grid.
- `metadata.dat` value semantics. `stpi` fields beyond the first four. `.stpd` tile ordering.
- Binary UI layout field order (only the header, strings and `events_end` terminator are known).
- How the engine maps ESF `LUA[]` back when the script changes the number of saved values (eur startpos has 17 vs 19 save_value calls). INFERRED: missing values fall back to the `load_value` default.
- The season-string rule. Tech state enum (0/2/4) meaning. Diplomacy attitude array (24 entries) semantics.

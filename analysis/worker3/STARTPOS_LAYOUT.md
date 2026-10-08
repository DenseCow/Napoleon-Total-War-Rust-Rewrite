# Start position and save layout (what `ntw_campaign` reads)

This document describes **where things live** inside `startpos.esf` (root `CAMPAIGN_STARTPOS`)
and `.save` files (root `CAMPAIGN_SAVE_GAME`), so that `crates/ntw_campaign` can turn them into
the simulation's `CampaignModel`. It is about structure only. It contains no bulk game data.

The ESF container itself (type codes, records, record arrays, offsets) is described in
`WORKER3_REPORT.md` §2 and `crates/ntw_formats/src/esf/mod.rs`. A short reminder:
values inside a record have **no names**, only positions. So a field is written here as
`RECORD #n`: the n-th child of that record, counting from 0 and counting child records too.
`NAME[]` is a record array; `{NAME}` is a child record.

Tags: **CONFIRMED** = seen in the bytes of every file listed below, or proven by a cross-check.
**INFERRED** = a strong guess from values and patterns. **UNKNOWN** = not worked out.

Evidence base: all 8 shipped `data\campaigns\<c>\startpos.esf` files (eur, mp_eur, egy, mp_egy,
ita, mp_ita, spa, tut) and the 8 user saves (mp_eur, 1.3.0 build 2081).

## How to regenerate the dumps
Two read-only tools ship with the crate:
```text
cargo run -p ntw_campaign --release --example esf_dump -- schema <file> [max_depth]
cargo run -p ntw_campaign --release --example esf_dump -- tree <file> <PATH> [depth] [items]
cargo run -p ntw_campaign --release --example campaign_summary -- <data dir> <file>...
```
`schema` prints every distinct record path once, with its count, versions and child signature.
`tree` prints one subtree with values, e.g. `CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY[0]/FACTION`.

## 1. Top level (CONFIRMED)
```text
CAMPAIGN_STARTPOS v5                       CAMPAIGN_SAVE_GAME v5
  {BUILD} v0                                 {BUILD} v0
  {SAVE_GAME_HEADER} v1|v2                   {SAVE_GAME_HEADER} v2
  {CAMPAIGN_PREOPEN_MAP_INFO} v4             (absent in saves)
  {CAMPAIGN_ENV} v2                          {CAMPAIGN_ENV} v2
```
* `BUILD`: #0 utf16 build id, #1 utf16 version string.
* `SAVE_GAME_HEADER` v2: #0 utf16 player faction key, #1 utf16 portrait path, #2 u32 turn number
  (1-based), #3 u32 year, #4 utf16 season name, #5 utf16 flag path, #6 `{DATE}`, #7 `MAPS[]`
  (preview image). **v1** (ita, mp_ita) has no `DATE`: #6 is `MAPS[]`. The loader therefore finds
  `DATE` by name.
* `CAMPAIGN_PREOPEN_MAP_INFO` v4 (front-end data, startpos only): `{CAMPAIGN_PLAYERS_SETUP}`,
  `{DATE}`, utf16 campaign key, utf16 map key, `VICTORY_CONDITION_OPTIONS[]`, `FACTION_INFOS[]`,
  `REGION_OWNERSHIPS_BY_THEATRE[]` = {utf16 theatre, `REGION_OWNERSHIPS[]` = {utf16 region key,
  utf16 owner faction key}}. Not loaded into the model; used by the tests as an independent check
  of region ownership.
* `CAMPAIGN_ENV` v2: #0 bool, #1 bool, `{CAMPAIGN_SETUP}` v3, `{CAMPAIGN_SETUP_LOCAL}`,
  `{CAMPAIGN_CAMERA_MANAGER}`, `{CAMPAIGN_MODEL}` v10.
* `CAMPAIGN_SETUP` v3: #0 utf16 campaign key (e.g. `eur_napoleon`, CONFIRMED equal to the folder
  name in all 8), `{CAMPAIGN_PLAYERS_SETUP}` → `PLAYERS_ARRAY[]` → `CAMPAIGN_PLAYER_SETUP` v3 =
  {`{CAMPAIGN_VICTORY_CONDITIONS}`, `{..._INGAME_MODIFIABLES}`, #2 utf16 faction key, #3 bool
  (INFERRED: human), #4 bool (INFERRED: playable; CONFIRMED to be true for exactly the 5 eur
  playables), #5 bool (UNKNOWN)}.

## 2. `CAMPAIGN_MODEL` v10 (CONFIRMED child order)
`{CAMPAIGN_MAP_DATA}`, u32, `{RandSeed}`, `{CAMPAIGN_CALENDAR}`, `{WORLD}`, bool,
`{LOCOMOTION_MANAGER}`, `{PENDING_BATTLE}`, `{HISTORICAL_EVENT_MANAGER}`,
`{HISTORICAL_CHARACTER_MANAGER}`, `{CAI_INTERFACE}`, `{CAMPAIGN_TRADE_MANAGER}`,
`{MARKER_MANAGER}`, `PORTRAIT_ALLOCATOR[]`, `{CAMPAIGN_PATHFINDER}`, `{EPISODIC_RESTRICTIONS}`,
`CULTURE_UNIT_CLASS_MAPPING[]`, bool, bool, bool, `OSMOSIS_CULTURES[]`, u32, f32, u32, u32,
`{TURN_TIMER}`, `CAMPAIGN_DIRECTOR[]`, bool.

| record | layout | tag |
|---|---|---|
| `CAMPAIGN_MAP_DATA` v2 | #0 utf16 map folder, #1 utf16 display folder, #2 utf16 map key (`nap_europe`), #3 bool | CONFIRMED |
| `RandSeed` v0 | #0 u32 | CONFIRMED location. INFERRED: the campaign RNG's current LCG state (it differs per file and per save) |
| `CAMPAIGN_CALENDAR` v2 | #0 u32 turns per year (24), #1 u32 turn in year, #2 `{DATE}`, #3 u32 turns elapsed | CONFIRMED (W3 §3.1; header #2 = #3 + 1 in all 16 files) |
| `DATE` v2 | u32 year, u32 season, u32 month (0 = Jan), u32 half (0 Early, 2 Late) | CONFIRMED |

## 3. `WORLD` v4
`{ANCILLARY_UNIQUENESS_MONITOR}`, u32, u32, `FACTION_ARRAY[]`, `{REBEL_FACTION}`,
`{REGION_MANAGER}`, bool, `SPYING_ARRAY[]`, `{REBEL_SPYING}` (CONFIRMED).

### 3.1 `FACTION` v18 (item of `FACTION_ARRAY[]`, and inside `REBEL_FACTION`)
The child positions are **not fixed**: optional records (`CAMPAIGN_MISSION_MANAGER`,
`CAMPAIGN_SHROUD`, `CAMPAIGN_VICTORY_CONDITIONS`, ...) exist only for some factions (CONFIRMED:
france has the id at #8, minor factions at #7). The loader therefore reads the **plain values**
(children that are not records) in order:

| plain value | type | meaning | tag |
|---|---|---|---|
| 0 | i32 | faction object id; diplomacy, regions and garrisons refer to it | CONFIRMED (cross-references match) |
| 1 | utf16 | faction key (DB `factions`) | CONFIRMED |
| 2 | utf16 | display name | CONFIRMED |

Child records found by name:
* `FACTION_ECONOMICS` v6: `history[]`, #1 i32 **treasury**, u8[], u32. CONFIRMED values
  (france 6500, austria 7500, britain 7000, minors 5000); INFERRED meaning "treasury".
* `GOVERNMENT` v1: i32, utf16, i32, `POSTS_ARRAY[]`, `GOV_IMP[]`. The single `GOV_IMP` item is a
  record whose **name** is the government type: `GOVERNMENT::ABSOLUTE_MONARCHY` /
  `CONSTITUTIONAL_MONARCHY` / `REPUBLIC` (CONFIRMED). The rebel faction has no `GOVERNMENT`.
* `DIPLOMACY_MANAGER` v3: `DIPLOMACY_RELATIONSHIPS_ARRAY[]`, u32 x4. One
  `DIPLOMACY_RELATIONSHIP` v14 per other faction (eur: 41 x 40 = 1640):
  #0 i32 target faction id, #1 `DIPLOMACY_RELATIONSHIP_ATTITUDES_ARRAY[]`, #2 bool, #3 i32,
  #4 utf16 **stance** (`neutral` / `war` / `allied` / `protectorate` / `patron`), then many
  UNKNOWN fields, `REGULAR_PAYMENTS[]`, `ALLIED_IN_WAR_AGAINST[]`, u32[], u32, a second utf16
  stance (INFERRED: previous stance), ... (CONFIRMED structure). In every shipped startpos the
  stored stances are symmetric (CONFIRMED for eur by `World::diplomacy_is_symmetric`).
* `CHARACTER_ARRAY[]` (see 3.2) and `ARMY_ARRAY[]` (see 3.3).
* Not loaded (UNKNOWN or not modelled yet): `ANCILLARY_UNIQUENESS_MONITOR`,
  `CAMPAIGN_PLAYER_SETUP`, `ARMY_REINFORCEMENT_MANAGER`, `FAMILY`, `CAMPAIGN_MISSION_MANAGER`,
  `NAME_ALLOCATION_DETAILS` x10, unit-name allocators, `FACTION_TECHNOLOGY_MANAGER`, `MORGUE[]`,
  `CAMPAIGN_SHROUD`, `FORT_UPGRADE_MANAGER` x2, `PRESTIGE`, `FACTION_FLAG_AND_COLOURS` x2,
  `CAMPAIGN_BONUS_VALUES` x2, `CHARACTER_RECRUITMENT_MANAGER`, religion key, `RECENT_PROPOSALS[]`
  and about 30 loose bools/ints.

`REBEL_FACTION/FACTION` is a short `FACTION` v18: `{ANCILLARY_UNIQUENESS_MONITOR}`,
`{CAMPAIGN_PLAYER_SETUP}`, `{FACTION_ECONOMICS}`, `{ARMY_REINFORCEMENT_MANAGER}`, i32 id,
utf16 key (empty), utf16 name ("NOT FOR DISPLAY (REBEL FACTION)"), bools, `CHARACTER_ARRAY[]`,
`ARMY_ARRAY[]`, `MORGUE[]`, ... (CONFIRMED). Empty in startpos files; saves hold rebel characters
there.

### 3.2 `CHARACTER` (v12 in shipped startpos, v14 in 1.3 saves)
The fields used are at the same positions in both versions (CONFIRMED).

| child | type | meaning | tag |
|---|---|---|---|
| #0 | `{LOCOMOTABLE}` v2 | position and movement, below | CONFIRMED |
| #1 | `{CHARACTER_DETAILS}` v3 | traits, names, birth date, portrait, faction key, ancillaries, attributes | CONFIRMED structure, not loaded |
| #2 | i32 | character id | CONFIRMED (= `MILITARY_FORCE` #1 of its army) |
| #3 | utf16 | type | CONFIRMED strings: `General`, `colonel`, `admiral`, `captain`, `minister`, `gentleman`, `rake`, `assassin`, `Eastern_Scholar`, `catholic_missionary`, `orthodox_missionary`, `Protestant_Missionary`, `guerilla` |
| #4 | u32 | id of the force this character commands (0 = none) | INFERRED (equal to `MILITARY_FORCE` #0 in the eur samples checked) |
| #5 | u32 | unit id of the general's own bodyguard unit | INFERRED (equal to `UNIT` #4 of the `Gen_*` unit in the eur samples checked) |
| #6..#38 | mixed | UNKNOWN | |

`LOCOMOTABLE` v2: #0 i32 x, #1 i32 z (20-bit fixed point, CONFIRMED), #2/#3 i32 x/z again
(INFERRED: destination or previous position), #4..#7 f32, #8 i32 and #9 i32 (INFERRED: current
and maximum movement points, e.g. 26/30 for a general, 90/66 for an admiral), #10 bool, #11/#12
i32, #13 angle, #14/#15 u32, #16 f32, #17/#18 u32[] (UNKNOWN). Ministers sit at (0, 0) or at the
capital.

### 3.3 Armies and navies (items of `FACTION/ARMY_ARRAY[]`)
Each item holds one record, `ARMY` v2 or `NAVY` v1 (CONFIRMED).
* `ARMY` v2: #0 `{MILITARY_FORCE}`, #1 `UNITS_ARRAY[]`, u32[], u32[], i32 (= the force id again),
  u32, bool, u32, bool.
* `NAVY` v1: #0 `{MILITARY_FORCE}`, #1 `UNITS_ARRAY[]`, u32[], u32[], u32,
  `{THEATRE_TRANSITION_INFO}`, u32.
* `MILITARY_FORCE` v1: #0 u32 force id, #1 u32 commanding character id (CONFIRMED by matching
  `CHARACTER` #2), #2 u32[] (UNKNOWN). Every force in all 16 files has a commander and at least one
  unit (CONFIRMED by the tests).
* `UNITS_ARRAY[]` items: `LAND_UNIT` v1 = {`{LAND_RECORD_KEY}` {utf16}, `{UNIT}`, u32} or
  `NAVAL_UNIT` v1 = {`{NAVAL_RECORD_KEY}` {utf16}, `{UNIT}`, u32, bool, bool, `{SHIP_DAMAGE_INFO}`}.
* `UNIT` v3: #0 `{UNIT_RECORD_KEY}` {utf16 unit key, DB `units`}, #1 `{UNIT_HISTORY}`
  {`{DATE}` raised, u32, u32}, #2 `{COMMANDER_DETAILS}`, #3 `{TRAITS}`, #4 i32 unit id, #5 u32 men,
  #6 u32 max men, #7 i32 (UNKNOWN: 44 general, 41 cavalry, 32 line infantry, 66 ship),
  #8..#9 u32, #10 u32 (= commanding character id for the general's unit, else 0), #11..#12 u32,
  #13 u8, #14 `{CAMPAIGN_LOCALISATION}` regiment name, #15 i32, #16 f32 (1.2 everywhere).
  Structure CONFIRMED; "men / max men" INFERRED (60/60 cavalry, 24/24 general's staff, 344/344
  for a 3-decker = crew).

### 3.4 Regions (`REGION_MANAGER` v1 → `REGIONS_ARRAY[]` → `REGION` v5)
| child | type | meaning | tag |
|---|---|---|---|
| #0 | utf16 | region key (DB `regions`) | CONFIRMED |
| #1 | `{POPULATION}` v1 | #0 `{REGION_FACTORS}` (classes, religion breakdown), #1 u32 **total population**, #2 u32 (= 2 x #1, UNKNOWN), #3 u32 (= #1), #4 i32 | CONFIRMED (Paris 17,386,000) |
| #2 | `{TRAITS}` | region traits | not loaded |
| #3 | `{REGION_SLOT_MANAGER}` v1 | `REGION_SLOT_ARRAY[]`, `{ROAD_SLOT}`, `{FORTIFICATION_SLOT}` | CONFIRMED |
| #4 | i32 | region object id | CONFIRMED unique; INFERRED role |
| #5 | `{SETTLEMENT}` v2 (startpos) / v3 (save) | below | CONFIRMED |
| #6..#19 | mixed | UNKNOWN (#9..#14 look like per-turn economy figures) | |
| #20 | u32 | **owner faction id** (same 32 bits as the faction's i32 id) | CONFIRMED: matches `CAMPAIGN_PREOPEN_MAP_INFO` ownership for every region of all 8 campaigns (293 regions) |
| #21 | `{LINE_OF_SIGHT}` | | not loaded |
| #22 | u32 | UNKNOWN id (equals the single value in the owner's `FACTION` u32[1]; INFERRED governorship/theatre link) | |
| #23 | utf16 | theatre (`europe`) | CONFIRMED |
| #24, #25 | utf16 | rebel faction key and display name | CONFIRMED |
| #26 | utf16 | climate key | CONFIRMED |
| #27 | `{REGION_RECRUITMENT_MANAGER}` | land recruitment queue | CONFIRMED (saves) |
| #28..#42 | mixed | resources (`RESOURCES_ARRAY[]`), forts, onscreen-name loc key, ... | not loaded |

* `SETTLEMENT`: #0 `{SIEGEABLE_GARRISON_RESIDENCE}`, #1 `{CAMPAIGN_LOCALISATION}`, #2 i32,
  #3 utf16 settlement key `settlement:<region>:<town>`, #4 i32 (= garrison residence id), #5
  (v2: utf16 key again; v3: bool).
* `SIEGEABLE_GARRISON_RESIDENCE` v1: #0 `{GARRISON_RESIDENCE}` {u32 owner faction id}, #1 u32
  residence id, #2..#9 mixed, **#10 i32 x, #11 i32 z** (20-bit fixed point; CONFIRMED: Paris
  (-212.2088, 2.2951) = `regions.esf`), #12 u32, u32[], u32[], `{LINE_OF_SIGHT}`, bool,
  `FORTIFICATIONS_BLOCK[]`.
* `REGION_SLOT` v3 (items of `REGION_SLOT_ARRAY[]`, and inside `ROAD_SLOT` / `FORTIFICATION_SLOT`):
  #0 `{SIEGEABLE_GARRISON_RESIDENCE}`, #1 `{BUILDING_MANAGER}`, #2 u32 id, #3 utf16 slot key
  `settlement:<region>:<town>:<slot>:<n>`, #4..#9 i32 (#6/#7 fixed-point position), ...
  Port slots also hold a `{REGION_RECRUITMENT_MANAGER}` (naval queue).
* `BUILDING_MANAGER` v1: bool, optional `{BUILDING}`, bool (or just bool, bool when empty).
  In saves a `{BUILDING_CONSTRUCTION_ITEM}` {u32, bool, u32, u32, u32, utf16 key} can appear.
* `BUILDING` v1: #0 u32 health (100), #1 utf16 building level key (DB `building_levels`),
  #2 utf16 faction key, #3 utf16 government key (CONFIRMED).
* `REGION_RECRUITMENT_MANAGER` v1: `REGION_RECRUITMENT_ITEM_ARRAY[]`, bool, i32. Items:
  `RECRUITMENT_ITEM` v2 → `{LAND_UNIT_RECRUITMENT_ITEM}` or `{NAVAL_UNIT_RECRUITMENT_ITEM}` →
  inner `RECRUITMENT_ITEM` v2 = {i32 id, i32, i32, #3 u32 (INFERRED: turns remaining, 1..3),
  #4 u32 (INFERRED: cost), bool, #6 utf16 unit key (CONFIRMED), u32 (= #4), u32, bool, i32, u32,
  bool}. Empty in every startpos.

## 4. Save differences that matter to the loader (CONFIRMED)
* Root `CAMPAIGN_SAVE_GAME` v5 = {`BUILD`, `SAVE_GAME_HEADER`, `CAMPAIGN_ENV`}: no
  `CAMPAIGN_PREOPEN_MAP_INFO`.
* `CHARACTER` v14 and `SETTLEMENT` v3 instead of v12 / v2; every field the loader reads is at the
  same position.
* Recruitment queues, construction items, rebel characters and pending-battle data appear.
* The user's 8 saves reference 271 to 650 unit keys that are not in the installed `units` table
  (e.g. `Cav_Light_British_Hussars`, `Gen_Earl_Uxbridge`), even the turn-1 save. The keys do not
  occur anywhere in the installed `data.pack` units table. INFERRED: the saves were made with a mod
  or DLC content that is no longer installed. The loader keeps those units and reports
  `LoadWarning::UnknownUnitKey`.

## 5. What the loader produces (per campaign, CONFIRMED by `tests/real_install.rs`)
Factions exclude the rebel faction (which the loader also adds, see `LoadedCampaign::rebel_faction`).

| campaign | factions | regions | characters | armies | navies | units | start |
|---|---|---|---|---|---|---|---|
| eur_napoleon | 41 | 72 | 531 | 69 | 18 | 441 | Early Jan 1805 |
| mp_eur_napoleon | 41 | 72 | 539 | 69 | 18 | 440 | Early Jan 1805 |
| egy_napoleon | 5 | 30 | 113 | 14 | 1 | 97 | Early Jun 1798 |
| mp_egy_napoleon | 5 | 30 | 116 | 16 | 1 | 107 | Late Sep 1798 |
| ita_napoleon | 12 | 25 | 166 | 33 | 0 | 137 | Early Apr 1796 |
| mp_ita_napoleon | 12 | 25 | 215 | 25 | 0 | 121 | Late Apr 1796 |
| spa_napoleon | 4 | 31 | 135 | 42 | 6 | 198 | Late Mar 1811 |
| tut_napoleon | 4 | 8 | 82 | 2 | 2 | 6 | Early Jan 1778 |

The DB `regions` table has 159 rows because it lists the regions of every theatre; one campaign
uses only its own theatre's regions (72 for Europe). Every loaded region, faction and (startpos)
unit key exists in the DB.

## 6. Not representable yet (UNKNOWN / TODO, skipped without error)
**Update (campaign-data worker):** character details, government posts, governorship taxes, capitals, the full
diplomacy records and the script `save_value` slots are now loaded: see `analysis/campaign/CAMPAIGN_DATA.md` §3-4.
The tax rate is in `GOVERNORSHIP_TAXES` (CONFIRMED), so the PLACEHOLDER below is gone for every faction with a
governorship.
Population classes and religion, region traits, road and fortification slots, construction
queues, resources, character details (traits, ancillaries, names, ages), the general's
bodyguard-unit link (`CHARACTER` #5), unit experience and ship damage, technologies, government
posts and ministers' offices, families, prestige, victory conditions, missions, shroud, espionage
data, the whole campaign AI (`CAI_INTERFACE`, about 18 MB), trade (`CAMPAIGN_TRADE_MANAGER`),
pathfinding (`CAMPAIGN_PATHFINDER`), script state (`EPISODIC_RESTRICTIONS`, including the Lua
`save_value` slots), historical events and characters, pending battles, the camera, and the setup
options. Where the tax rate is stored is UNKNOWN; the loader gives every faction the PLACEHOLDER
`PLACEHOLDER_TAX_RATE_PCT` (50).

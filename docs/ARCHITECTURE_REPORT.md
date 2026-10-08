# Napoleon: Total War — Game Architecture Report (Phase 1)

Written by the manager from the reports of Worker 1 (exe/engine), Worker 2 (packs/DB/battle data) and Worker 3
(ESF/campaign/Lua/UI/campaign DB). The detailed evidence lives in:
- `analysis/worker1/WORKER1_REPORT.md`, `analysis/worker1/DB_BUILDERS.md`
- `analysis/worker2/WORKER2_REPORT.md`, `analysis/worker2/schemas.md`
- `analysis/worker3/WORKER3_REPORT.md`, `analysis/worker3/DB_CAMPAIGN_TABLES.md`

**Tags:**
- **CONFIRMED**: seen directly in bytes or decompiled code.
- **INFERRED**: a strong conclusion from several pieces of evidence.
- **UNKNOWN**: not determined yet.

**✔ = the manager independently re-checked it against the original files.** Pseudocode in the worker reports is
reconstructed and is *not* Creative Assembly source code.

---

## 0. The 30-second summary (for beginners)

Napoleon: Total War is a 32-bit Windows program written in C++. Almost everything you see and every rule number lives
in big archive files (`.pack`), not in the program itself:
- The **program** (`Napoleon.exe`) holds the *rules engine*: how morale changes, how fatigue builds up, how random numbers are made.
- The **packs** hold the *numbers and assets*: unit stats, factions, buildings, text, 3D models, sounds.
- Each campaign's **starting world** is stored in a separate kind of file (`.esf`), and so are save games.

We now understand all three file types well enough to read them byte-for-byte, plus a large part of the rules engine.

---

## 1. Executables

| Item | Finding | Tag |
|---|---|---|
| Main program | `Napoleon.exe`, 17.9 MB, **32-bit x86**, PE32 | CONFIRMED ✔ |
| Build | **2023-05-12 rebuild** (version 1.3.0, "Build 2081 (Curator)"), not the 2010 original | CONFIRMED ✔ (save files carry the same build string) |
| Compiler | Visual Studio 2019 (linker 14.21), static C runtime | CONFIRMED ✔ |
| Entry point | 0x0126E7D0 → CRT → `wWinMain` 0x0048DA70 → init + run 0x0048C1B0 | CONFIRMED ✔ (entry) |
| Protection | No DRM wrapper or packer. /guard:cf, a stack cookie, and an IGameExplorer access check | CONFIRMED |
| Debug info | PDB path `s:\branches\napoleon\curator\napoleon\binaries\napoleon.retail.pdb` (the PDB itself is not shipped) | CONFIRMED ✔ |
| C++ class info (RTTI) | **Disabled** (/GR-). Only 24 runtime-library classes exist, and no game classes | CONFIRMED |

## 2. DLLs and third-party libraries

| Library | Purpose | Tag |
|---|---|---|
| Direct3D 9 + debug D3DX9 (`D3dx9d_40.dll`) | rendering (no D3D10/11) | CONFIRMED |
| DirectInput 8 | input (no gamepad/XInput) | CONFIRMED |
| Miles Sound System (`mss32.dll` + `miles\` plugins) | audio | CONFIRMED |
| Bink (`binkw32.dll`) | video | CONFIRMED |
| Intel TBB 2.1 | multithreading | CONFIRMED |
| Steamworks (2017 SDK) + raw WinSock | multiplayer/LAN | CONFIRMED |
| Statically linked | **Lua 5.1** ✔, zlib 1.2.3, SpeedTree RT | CONFIRMED |
| Absent | GameSpy, Havok, PhysX, Scaleform, FMOD, Wwise | CONFIRMED |

## 3. Engine architecture

The source-tree names (CONFIRMED, from 170 embedded file paths) show these modules:

```
Empire          — application, front end, mode switching
EmpireBattle    — battle model: units, soldiers, morale, fatigue, missiles, naval, battle AI, autoresolve
EmpireCampaign  — campaign model: factions, regions, characters, armies, buildings, triggers
EmpireCampaign/CAI — campaign AI (belief-desire-intention "BDI" architecture)
EmpireCommon    — game core, multiplayer base
EmpireUtility   — databases (483 record types), shortcuts, terrain generation
Warscape        — renderer/engine: D3D9 managers, scene nodes, terrain, sea, weather, vegetation
UtilityDLL      — Lua wrapper, TWEAKER config values, offline data
UiComponentLib  — UI component system (layouts + Lua scripts)
Sound           — Miles wrapper, sound banks
```

**Key architectural facts:**
- **Model vs display (CONFIRMED).** Battle *model* code is separate from *display* types (`ENTITY_DISPLAY`). Our
  design copies this split (see `docs/DESIGN.md`).
- **Lockstep determinism (CONFIRMED).** Multiplayer battles compare simulation state per tick ("DESYNC DETECTED AT TICK %d").
  The simulation must therefore be deterministic.
- **Command queue (CONFIRMED).** Campaign actions are queued commands (`CCQ_END_TURN`, `CCQ_SET_GOVERNORSHIP_TAX_RATE`, ...).
- **Lazy database (CONFIRMED).** Each table loads the first time it is used ("Loading database: %s").
- **Tunables (CONFIRMED).** 567 `TWEAKER` values with defaults, and 110 `preferences.script` keys.

## 4. Game loop

1. Startup init order (CONFIRMED from strings; INFERRED that they execute in this order):
   VFS (packs) → Steam → `load_release` → database → advisor → shortcuts → multiplayer → Warscape → sound → campaign heightmap.
2. The main loop (0x00485B90) pumps Windows messages and runs the current **mode handler**: front end, campaign, battle,
   replay, and so on (CONFIRMED).
3. **Battle tick = 0.1 s (10 Hz)**: INFERRED (high). `seconds = tick × 0.1`.
4. **Battle speeds are exactly {pause, 0.4, 1, 2, 4}** and cycle in that order (CONFIRMED).
5. **Per-unit update order** is known (0x0057F070). Each unit's full morale check runs only when `unit_id % 5 == tick % 5`,
   which is every 0.5 s, staggered across units (CONFIRMED).
6. The exact frame order (render vs simulation) and render interpolation are UNKNOWN.

## 5. Campaign systems

| System | What we know | Tag |
|---|---|---|
| Calendar | 24 turns/year: "Early" and "Late" of each month. `turn_in_year = month×2 + late` | CONFIRMED |
| Map coordinates | Positions are i32 fixed-point with 20 fractional bits (Paris = −212.2088, 2.2951) | CONFIRMED ✔ |
| Start state | Read from `startpos.esf` per campaign. The Europe start has 41 factions, 72 regions, 531 characters, 69 armies, 18 navies and 441 units, in Early January 1805 | CONFIRMED |
| Factions | Treasury, government type (absolute monarchy / constitutional / republic), technologies, diplomacy matrix, colours | CONFIRMED (structure) |
| Diplomacy | Per-pair stances: neutral / war / allied / protectorate / patron, with 24 "attitude" entries | CONFIRMED structure; attitude meanings UNKNOWN |
| Regions/settlements | Population classes, building slots, roads, forts, resources, climate | CONFIRMED (structure) |
| Buildings | `building_levels`: chain, level, build turns, cost, 4 prestige values | CONFIRMED (exe column order) |
| Technology | Cost, research building, tree position; state enum 0/2/4 | CONFIRMED (table); enum meaning INFERRED |
| Movement | Pathfinding grid in `pathfinding.esf` (2.0-unit cells); terrain multipliers in `campaign_ground_types`; road factors 0.67/0.6/0.5/0.4 | CONFIRMED (data); algorithm UNKNOWN |
| Economy | 121 global `campaign_variables` (tax efficiency, happiness, looting, ...) | CONFIRMED (values); **formulas UNKNOWN** |
| Victory | Stored in `startpos.esf` (required regions, region count, deadline), evaluated by the engine | CONFIRMED |
| Events | 168 script events, e.g. FactionTurnStart, CharacterCompletedBattle | CONFIRMED |
| **Turn phase order** | The events exist, but their **order is UNKNOWN** | UNKNOWN |

## 6. Battle systems

| System | What we know | Tag |
|---|---|---|
| Unit stats | One big `unit_stats_land` table (89 columns) holds men, armour, melee attack, charge, defence, morale, accuracy, reload, ammo and more. **There are no separate weapon/armour tables** | CONFIRMED (exe + data) |
| Example | Austrian fusiliers: 160 men, armour 3, accuracy 40, melee 6, charge 10, defence 6, morale 6 | CONFIRMED (data) |
| Projectiles | 35 columns. A flintlock musket has range 80; a 12-lb round shot has range 600 | CONFIRMED |
| Movement physics | Infantry walk 1.4 m/s; horses run 10 and charge 11.5 m/s | CONFIRMED (data) |
| **Morale** | 8-state machine with hysteresis (impetuous → eager → confident → steady → shaken → wavering → broken → shattered). Modifiers for flank/rear attack, cavalry, casualty ladders (20/40/60/80/90%) and kill "blood" bonuses | CONFIRMED (code); state names INFERRED |
| **Fatigue** | 6 states (fresh → ... → exhausted), per-action rates, slope multipliers at gradient 0.05/0.1/0.2, rain/snow terms | CONFIRMED (code) |
| **Missile range/accuracy** | Walls modifiers, ×0.8 range state, +20/+30/+15 accuracy bonuses, shot spread formula | CONFIRMED (code) |
| **Missile hit chance** | Uses `missile_distance_for_half_chance_hit` (= 50) | key CONFIRMED; formula UNKNOWN (Worker 1 is working on it) |
| **Melee hit/kill** | Uses melee attack vs defence, armour divisors, flank 8 / rear 18 factors, bayonet +15 | inputs CONFIRMED; **formula UNKNOWN (Worker 1 is working on it)** |
| Autoresolve | Kill-rate formula with tweak defaults (0.2 / 0.2 / 0 / 0 / 2 / 0.1 / ...) | CONFIRMED |
| Wind | 5 levels with exact thresholds | CONFIRMED |
| Terrain/weather | Ground types and climates are in the DB; rain/snow affect fatigue | partial |
| Sieges, naval | Code and tables exist (ship damage, buoyancy, fire) | CONFIRMED to exist; not yet analysed |

## 7. AI

| Kind | What we know | Tag |
|---|---|---|
| Campaign AI | A BDI architecture with 30 behaviour modules (war & peace, region defence, taxation, ...), per-personality tunables (245), and AI state stored in saves | CONFIRMED (structure + data) |
| Difficulty | Concrete handicaps, e.g. AI research −12 and upkeep +18 at one level | CONFIRMED (data) |
| Battle AI | `BattleAI`, `HighLevelPlanner`, `MeleeManager` analysers, tactics such as double envelopment. **No battle-AI tuning table** | CONFIRMED (structure); behaviour UNKNOWN |

## 8. Data formats (all readable byte-exactly)

| Format | Summary | Tag |
|---|---|---|
| `.pack` | `PFH0` header, type (0 boot / 1 release / 2 patch / 4 movie), then an index of {size, path}, then uncompressed payloads back-to-back. Load order: boot → release → patch (patch wins) | CONFIRMED ✔ (sizes add up exactly) |
| DB tables | Optional `FC FD FE FF`+version, u8 flag, u32 rows. 310 tables. Column layouts come from the exe's own loaders | CONFIRMED |
| `.loc` text | `FF FE "LOC\0"`, version, count, {UTF-16 key, UTF-16 text, flag}. Keys are `<table>_<field>_<key>` | CONFIRMED |
| `.esf` | Magic `0xABCE`, typed nodes, versioned records, absolute offsets, a name table at the end. Used for start positions, saves and map data | CONFIRMED ✔ |
| Saves | ESF `CAMPAIGN_SAVE_GAME`: the full campaign model plus queues, pending battles, AI state and script variables | CONFIRMED |
| Lua | Campaign/battle scripts are plain source. UI scripts are Lua 5.1 bytecode with **32-bit float numbers** | CONFIRMED |
| UI layouts | `VersionNNN` binary (versions 028 to 039); the header and strings are known | field order UNKNOWN |
| 3D models/textures/sounds | `.rigid_model` (magic 0x12345678 v5), `.dds`/`.tga`, Miles banks | identified; not decoded |

## 9. Important functions (selection)

| Address | Purpose | Tag |
|---|---|---|
| 0x0048DA70 | wWinMain | CONFIRMED |
| 0x00485B90 | main loop / mode dispatch | CONFIRMED |
| 0x01051340 | VFS init (pack mounting) | CONFIRMED |
| 0x00E730D0 | generic DB table loader | CONFIRMED |
| 0x00987070 | campaign load (startpos or save) | CONFIRMED |
| 0x00988B00 | campaign Lua environment setup | CONFIRMED |
| 0x0057F070 | per-unit battle tick | CONFIRMED |
| 0x00584020 / 0x0053C720 | morale state machine / morale modifiers | CONFIRMED |
| 0x00670F40 / 0x00671230 | fatigue accumulation / state machine | CONFIRMED |
| 0x005646A0 / 0x006D8B00 / 0x006A5CE0 | missile range / accuracy / dispersion | CONFIRMED |
| 0x0078D200 | autoresolve kill rates | CONFIRMED |
| 0x005F0830 etc. | RNG helpers | CONFIRMED |

## 10. The random number generator (CONFIRMED ✔)

`state = state × 214013 + 2531011` (32-bit wrapping), output `state >> 16`. This is the game's own generator, inlined
732 times ✔, and each owner (battle, campaign, ...) keeps its own state. Matching it exactly is the basis for 1:1 battle
outcomes. The battle **seed source** is UNKNOWN.

## 11. Important structures and relationships

```
CAMPAIGN_MODEL
 ├─ CALENDAR, RandSeed
 ├─ WORLD
 │   ├─ FACTION ×41 ── DIPLOMACY_RELATIONSHIP ×40 each, TECHNOLOGY_MANAGER, GOVERNMENT, treasury
 │   │    ├─ CHARACTER (traits, ancillaries, position, movement points)
 │   │    └─ ARMY / NAVY (MILITARY_FORCE) ── UNIT (unit key → units → unit_stats_land → projectiles)
 │   └─ REGION ×72 ── SETTLEMENT, REGION_SLOT ── BUILDING (→ building_levels), POPULATION
 ├─ CAI_INTERFACE (campaign AI memory)
 ├─ CAMPAIGN_TRADE_MANAGER, PATHFINDER, PENDING_BATTLE
 └─ EPISODIC_RESTRICTIONS (Lua script state)

Battle:  BATTLE(tick, rng) ── ALLIANCE ── ARMY ── LAND_UNIT(morale component, soldiers) ── SOLDIER(fatigue, action)
Data:    units ──► unit_stats_land ──► projectiles / gun_types / mounts   (foreign keys, CONFIRMED)
Text:    any DB text column ──► .loc key "<table>_<field>_<primary key>"
```

Objects reference each other by **32-bit ids** stored in the ESF (CONFIRMED). Whether these are pointers or handles is UNKNOWN.

## 12. Confidence summary and open questions

**Fully specified (ready to implement 1:1):**
- RNG
- pack, DB, .loc and ESF reading
- calendar, map coordinates
- morale state machine and modifiers
- fatigue
- missile range, accuracy and dispersion
- autoresolve kill rates
- battle speeds and the morale stagger
- the full Lua API surface (names)

**Biggest UNKNOWNs, in priority order:**
1. Melee hit/kill formula (Worker 1 is on it now)
2. Missile hit chance
3. Campaign turn phase order
4. Economy/tax/population formulas
5. Battle RNG seeding
6. Morale timer formulas (waver/rout duration)
7. Battle locomotion and pathfinding algorithms
8. UI layout binary field order
9. 3D model, animation and sound formats
10. Meaning of the diplomacy attitude entries and several ESF scalars

**Manager's conflict resolutions:**
- W2 vs W3 DB schemas → resolved by Worker 1's exe loaders (`DB_BUILDERS.md`), which are authoritative.
  Byte-guessing failed because empty strings look like two false bools.
- No other conflicts were found. Workers 1, 2 and 3 agree on the build (1.3.0 / 2023) and on Lua 5.1.

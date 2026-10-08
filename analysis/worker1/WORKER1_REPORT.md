# WORKER 1 REPORT: Binary & Engine Reverse Engineering of Napoleon: Total War (Steam build)

Scope: the PE binaries in `C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War`. The install was opened read-only. Ghidra imported the exe from its 8.3 short path, because the `(x86)` in the long path breaks `analyzeHeadless.bat`. The Ghidra project lives in `worker1\ghidra_project` and all later runs used `-readOnly`.

Confidence tags:
- **CONFIRMED**: seen directly in the bytes or in Ghidra output.
- **INFERRED**: a strong conclusion drawn from several pieces of evidence.
- **UNKNOWN**: not determined.

Every pseudocode excerpt is **Ghidra reconstructed pseudocode or my own paraphrase. It is not the original source.**

Addresses are virtual addresses (ImageBase 0x00400000) unless they are marked as file offsets.

## 0. Provenance of results (tooling)

| Result | Produced by |
|---|---|
| `pe_report.txt`, `strings_all.tsv`, `strings_categorized.txt` | **Earlier Python scripts** (`pe_parse.py`, `strings_extract.py`), written before the no-Python rule. The files are kept unchanged. Their results match the Rust outputs. |
| `pe_report_rs.txt`, `strings_all_rs.tsv`, `strings_categorized_rs.txt`, `rtti_*.txt/tsv`, `class_names_from_strings.txt`, `tweakers.tsv`, `script_bindings.tsv`, `kv_layout.tsv`, `kv_usage.tsv`, `kv_getter_reads.tsv`, `rng_constants.txt`, `rng_lcg_sites.txt`, `import_refs.txt`, `xref_results.txt`, `db_table_names.txt`, `preferences_keys.tsv` | **Rust crate** `worker1\re_tools` (std only, no external crates). Subcommands: `pe`, `strings`, `classes`, `rtti`, `xref`, `imm`, `iat`, `impref`, `func`, `tweak`, `tweakuse`, `luabind`, `kvmap`, `kvuse`, `getteruse`, `findptr`. |
| `ghidra_out\functions.tsv` (46,968 functions), `ghidra_out\decomp1..7.c` | Ghidra 12.1.4 headless, using the Java postScripts `ghidra_scripts\ExportFunctions.java` and `DecompTargets.java`. Auto-analysis took 436 s. |

---

## 1. Executables

### 1.1 Napoleon.exe (17,895,424 bytes)

| Item | Value | Confidence |
|---|---|---|
| Machine | i386, PE32 (32-bit), LargeAddressAware=1 | CONFIRMED |
| TimeDateStamp | 0x645E917F = **2023-05-12 19:20:31 UTC**. This is a 2023 rebuild, not the 2010 original. | CONFIRMED |
| Linker | **14.21** (VS2019 16.1). Rich header: Linker1400/Cvtres1400/Export1400 build **27702**. | CONFIRMED |
| Subsystem | WINDOWS_GUI 6.0, OS 6.0 | CONFIRMED |
| ImageBase / EntryPoint | 0x00400000 / RVA 0xE6E7D0 (VA 0x0126E7D0, in `.text`) | CONFIRMED |
| DllCharacteristics | 0x8140: ASLR, NX, TS_AWARE | CONFIRMED |
| Load config | size 0xA4, SecurityCookie 0x0146C4A4, GuardCFCheck 0x01307670, GuardFlags 0x100 (CF instrumented, /guard:cf) | CONFIRMED |
| TLS | present (start/end at 0x0144F... / index 0x01841..., no callbacks) | CONFIRMED |
| Exception dir | none (normal for x86, which uses SEH) | CONFIRMED |
| Debug dir | CodeView RSDS GUID `E7B6EEEE-46FB-44DA-9C94-6A758958B750` age 1, PDB **`s:\branches\napoleon\curator\napoleon\binaries\napoleon.retail.pdb`**. Also VC_FEATURE and POGO (`GCTL`) entries. The POGO entry lists the COFF groups: `.text$mn` 0x44B10+0xE9C830, `.CRT$XCU` 0xF076F4+0x44FC (about 4,415 static initializers), `.bss` 0x106D438+0x3D6AA0, and others. | CONFIRMED |
| Version resource | FileVersion "1, 3, 0, 0", ProductVersion "3.0.1.0", OriginalFilename **"Napoleon.zIntelUnityRelease.exe"**, CompanyName The Creative Assembly Ltd | CONFIRMED |
| Export table | Named `napoleon.retail.exe`, 53 exports: UI file helpers (`UIFileIn`, `UIFileOut`, `VersionedFileOut`, `UIPoint`, `UIRect`), `?uif@@` factory, three `COPA::CB_BASE_DESTROY<LEADERBOARD_*>` functors, and **`?use_compiled_scripts@@3V?$TWEAKER@_N@UTILITYDLL@@A`** (a global TWEAKER<bool>) | CONFIRMED |

**Sections.** Entropy was computed by `re_tools pe`.

| name | VA | VSize | RawSize | flags | entropy |
|---|---|---|---|---|---|
| .text | 0x00001000 | 0xF058FC | 0xF05A00 | CODE R X | 6.52 |
| .rdata | 0x00F07000 | 0x146236 | 0x146400 | R | 6.10 |
| .data | 0x0104E000 | 0x3F5ED8 | 0x1F600 | R W (mostly .bss) | 4.07 |
| _RDATA | 0x01444000 | 0x2608 | 0x2800 | R | 6.08 |
| .rsrc | 0x01447000 | 0x1A368 | 0x1A400 | R | 7.74 (icon/bitmap data) |
| .reloc | 0x01462000 | 0x88A64 | 0x88C00 | R discard | 6.68 |

**Packing and DRM.** There is no `.bind` section, so this is **not SteamStub-wrapped**. `.text` entropy is a normal 6.5, and the entry point is the plain MSVC `__scrt_common_main_seh` path. There are no packer signatures. The only high-entropy section is `.rsrc`. The 2023 build has no DRM wrapper. It calls `SteamAPI_Init` directly (see 4.13). Confidence: CONFIRMED.

**Rich header** (prodid, build, count). Product IDs are decoded with the public mapping tables, and the build numbers are mapped to Visual Studio releases.
- 0x0102 Linker1400 b27702 (VS2019 16.1), 0x00FF Cvtres1400 b27702, 0x0100 Export1400 b27702
- 0x0105 Utc1900_CPP: b27702 ×212, b27521 ×66, b26715 ×235. 0x0104 Utc1900_C: b27702 ×42, b27521 ×39, b26715 ×44. The mix of VS2017 (26715) and VS2019 (27521/27702) objects means a prebuilt static library was compiled with VS2017.
- 0x0103 Masm1400 (×29, ×36, ×1), 0x0101 Implib1400 b26715 ×37
- 0x00DD Implib1200 **b40629** (VS2013 Update 5) ×2: matches steam_api.lib (steam_api.dll is linker 12.0)
- 0x0093 Implib900 b21022 (VS2008 RTM) ×2: matches d3dx9 import lib (D3dx9d_40.dll is linker 9.0)
- 0x007B b50727 (VS2005) ×6: binkw32/mss32/tbb import libs (all VS2005 builds)
- 0x0001 Import0 ×458

The game code itself was compiled with the VS2019 C++ compiler. Confidence: CONFIRMED.

**Imports.** There are 21 DLLs and 391 functions. The full list is in `pe_report_rs.txt`.

| DLL | # | Notable |
|---|---|---|
| KERNEL32 | 159 | QueryPerformanceCounter/Frequency, CreateThread, SetThreadAffinityMask, GetProcessAffinityMask, file-mapping APIs (MapViewOfFile), FindFirstFileW, GetDiskFreeSpaceA, CreateMutexW (single-instance check) |
| USER32 | 57 | PeekMessageW, DispatchMessageW, CreateWindowExW, ChangeDisplaySettingsW, GetAsyncKeyState, GetKeyboardState, SetWindowsHook (UnhookWindowsHookEx) |
| d3d9 | 3 | Direct3DCreate9, D3DPERF_Begin/EndEvent. **D3D9 only; no D3D10 or D3D11.** |
| d3dx9d_40 | 15 | D3DXCreateEffect / EffectCompiler / EffectPool, texture load/save. Note that this is the **debug** D3DX DLL. |
| DINPUT8 | 1 | DirectInput8Create. **No XInput import.** |
| DSOUND | 1 | ordinal 11 (DirectSoundCreate8) |
| mss32 | 52 | AIL_* (Miles: 3D samples, streams, filters, file callbacks) |
| binkw32 | 20 | BinkOpen, BinkDoFrameAsync, BinkOpenMiles, BinkSetSoundSystem |
| tbb | 11 | task_scheduler_init::initialize/terminate/default_num_threads, task::spawn_and_wait_for_all, allocate_root/child/continuation |
| steam_api | 16 | SteamAPI_Init/Shutdown/RunCallbacks, Register(Call)Callback(Result), SteamInternal_CreateInterface/ContextInit, SteamGameServer_* (game server for MP) |
| WS2_32 | 22 (ordinals) | socket, bind, sendto, recvfrom, select, gethostbyname, WSAStartup, and similar. These are raw sockets, used for LAN. |
| WINMM | 11 | timeBeginPeriod/timeEndPeriod, timeGetTime, mixer* (voice chat microphone) |
| others | | ADVAPI32 (registry), COMDLG32, OLEAUT32 (#2/#6/#9 = SysAllocString/SysFreeString/VariantClear), ole32 (CoCreateInstance for IGameExplorer), RPCRT4 (UuidToStringW), SHELL32/SHLWAPI (paths), VERSION, GDI32 |

All of the above is CONFIRMED.

### 1.2 Other executables (listing only)

- `redist\vcredist_x86-sp1.exe`: VC++ 2005 SP1 redistributable. It is a wextract SFX, linker 7.10, 2004. TBB needs it, because tbb.dll imports MSVCR80/MSVCP80. CONFIRMED.
- `redist\directx\`: DXSETUP.exe, DSETUP.dll, dsetup32.dll, and about 120 cabs (d3dx9_24…_40, d3dx10_33…_40, XACT, XAudio, X3DAudio, XInput, MDX1, BDA, Feb2005–Nov2008). The game imports only d3dx9d_40, which ships beside the exe. CONFIRMED.

## 2. DLLs

| File | Arch | Timestamp | Linker | PDB / notes | Exports |
|---|---|---|---|---|---|
| binkw32.dll | x86 | 2009-01-14 | 8.0 (VS2005) | `C:\devel\projects\bink\build\binkw32.pdb` (RAD Bink 1.x). Custom sections BINKY12/BINKY16/BINKP8/BINK16/BINK32/BINK/BINKBSS/BINKDATA. BINKDATA entropy 7.58 is codec tables. Imports mss32 (19 AIL functions) and DSOUND. | 72 |
| mss32.dll | x86 | 2009-04-23 | 8.0 | `C:\devel\projects\mss\build\win\mss32.pdb` (Miles Sound System). Section MSSMIXER. | 355 |
| miles\*.flt/*.asi | x86 | 2009 | 8.0 | mssdolby, mssds3d, mssdsp, msseax, msssrs (filters); mssmp3, mssogg, mssvoice (ASI decoders). Each exports one function (RIB provider). | 1 each |
| tbb.dll | x86 | 2009-02-01 | 8.0 | `z:\itt\branch_tbb21\...\fxeowin09icc10_1_021_32_vc8_..._release\tbb.pdb`. **Intel TBB 2.1**, built with Intel C++ 10.1 against the VC8 runtime. Imports MSVCR80/MSVCP80. | 161 |
| tbbmalloc.dll | x86 | 2009-02-01 | 8.0 | TBB 2.1 scalable allocator | 8 |
| steam_api.dll | x86 | 2017-12-15 | 12.0 (VS2013) | `c:\buildslave\steam_rel_client_win32\...\steam_api.pdb`. Authenticode-signed (6,944-byte overlay). Flat API (`SteamAPI_ISteam*`). | 859 |
| D3dx9d_40.dll | x86 | 2008-10-08 | 9.0 | **Debug** D3DX9 (Nov 2008 SDK, `d3dx9d_40.pdb`). Signed. Imports msvcrt. | 336 |

All of the above is CONFIRMED. None of the DLLs is packed.

## 3. Compiler, runtime and third-party libraries

| Component | Evidence | Confidence |
|---|---|---|
| MSVC 2019 16.1 (v142), static UCRT | Rich header, linker 14.21, `minkernel\crts\ucrt\...` string, `__scrt_*` CRT startup in Ghidra | CONFIRMED |
| Dinkumware STL (MSVC) | "Copyright (c) by P.J. Plauger, licensed by Dinkumware" @0x0146D088. RTTI shows std::locale and ctype only. | CONFIRMED |
| **RTTI disabled for game code (/GR-)** | Only **24** type descriptors exist, and all are CRT/STL/exception types plus `IdvFileError` and `IdvRuntimeError` (needed for throw). No CA game class has RTTI. | CONFIRMED |
| /guard:cf, /GS | LoadConfig GuardFlags 0x100, security cookie | CONFIRMED |
| Lua **5.1** (statically linked) | `$Lua: Lua 5.1 Copyright (C) 1994-2006 Lua.org, PUC-Rio $` @0x013ECCA8. The `UTILITYDLL::LUA::State` wrapper appears in `LuaState.cpp`. | CONFIRMED |
| zlib **1.2.3** | `deflate 1.2.3` @0x0142DF30, `inflate 1.2.3` @0x01431530 | CONFIRMED |
| **SpeedTree RT** (CSpeedTreeRT, CTreeEngine, CBranch, CLeafGeometry) | 44 strings, for example `CSpeedTreeRT::Compute` and `RigidModels/Vegetation/Wind/SpeedWind.ini` | CONFIRMED |
| Intel TBB 2.1 | imports; tweaker `tbb_naval_proxy_model_updated_grain` | CONFIRMED |
| Bink video, Miles audio | imports, `BinkVideoController.cpp` | CONFIRMED |
| Steamworks (2017 SDK) | imports, `COPA` leaderboard namespace, `SteamFriendsList`, Steam VOIP | CONFIRMED |
| Intel CPU topology code | `common\CALibs\src\WIN\Intel\cpu_topo.c` | CONFIRMED |
| Mersenne Twister (std::mt19937) | 0x6C078965 in 0x0117F480 (vectorized SSE4.1 seeding) and 0x9908B0DF in 0x01193450 (twist). Called from 0x01128F80, 0x011AE680 and 0x0118CAF0, which sit in the library/STL code region. | CONFIRMED that it exists; its gameplay use is UNKNOWN (probably not gameplay) |
| GameSpy | **0 strings**. Multiplayer is Steam plus raw WinSock (LAN). | CONFIRMED absent |
| Havok, Scaleform, PhysX, FMOD, Wwise, Granny | no strings or imports | CONFIRMED absent |
| Boost | no Boost identifiers. The "boost" string hits are gameplay words. | INFERRED absent |
| Anti-tamper | IGameExplorer access check in WinMain (CoCreateInstance and the "IGameExplorer could not verify access" string). No SteamStub. `IsDebuggerPresent` is used by the CRT only. | CONFIRMED |

## 4. Engine architecture, by subsystem

Source-tree layout comes from `__FILE__` strings (170 paths). Every path has the root `s:\branches\napoleon\curator\common\`. The modules are:
`Empire` (app/front end), `EmpireBattle`, `EmpireCampaign`, `EmpireCommon`, `EmpireUtility` (databases, terrain generation), `Warscape` (renderer/engine), `UtilityDLL` (Lua, tweakers, offline data), `UiComponentLib`, `Sound`, `CALibs`. All CONFIRMED.

C++ namespace names come from `__FUNCSIG__` and template diagnostics, because there is no RTTI (`class_names_from_strings.txt`): `EMPIRE`, `EMPIREBATTLE`, `EMPIRECAMPAIGN`, `EMPIRECAMPAIGNAI`, `EMPIRECOMMON`, `EMPIREUTILITY` (479 names, mostly `*_RECORD`), `WARSCAPE`, `UTILITYDLL`, `UTILITYLIB`, `CA`, `CA_STD` (custom allocators `CACHE_ALLOCATOR` and `MEMORY::CACustomHeapAllocator<1>`). All CONFIRMED.

| Subsystem | Evidence | Key types / functions | Confidence |
|---|---|---|---|
| **App bootstrap / WinMain** | CRT `FUN_0126e654` calls `wWinMain` = **0x0048DA70**. WinMain does CoInitialize, the IGameExplorer check, parses `-no_exception_handler`, calls the app init+run at **0x0048C1B0**, then shuts down. | 0x0048DA70, 0x0048C1B0 | CONFIRMED |
| **Init sequence** | 0x0048C1B0 references these strings in code order: `PRE_DB_APP_INIT` → `Finished run_vars` → `VFS started` → `Steam and legals` → `VFS::load_release` → `Finished VFS load_release` → `Loading database` → `Loading advisor` → `Loading shortcuts` → `POST_DB_APP_INIT` → `Creating Multiplayer` → `Creating Debug Menu Manager` → `Creating Debug rendering` → `Finished WarScape init` → `Creating sound` → `Finished sound init` → `Creating campaign heightmap` (`heightmaps/default.tga`) → `Init completed`. It also uses CreateMutexW for a single instance. | | CONFIRMED (string order); INFERRED (that the execution order matches) |
| **Main loop / mode handlers** | **0x00485B90** (6.2 KB) is the only DispatchMessageW user. It uses PeekMessageW, TranslateMessage and PostMessageW. It switches on a mode: `front_end`, `campaign`, `campaign_load`, `campaign_edit`, `campaign_replay`/`CAMPAIGN_REPLAY`, `battle`, `replay`, `debug_front_end`, `cinematic_editor`, `main`. `bink_use_thread` is checked here. The handler switch is 0x0048A650 ("About to create new handler", "Created new handler"), which reads QueryPerformanceCounter. | 0x00485B90, 0x0048A650 | CONFIRMED |
| **Timing** | QPC users are 0x0048A650, 0x004CD220, 0x010A0210 and two CRT functions. `timeBeginPeriod` is called at 0x010A1DD0. The pref `gfx_frame_rate <int>` and `cap_frame_rate` exist. | | CONFIRMED |
| **Battle simulation tick** | Battle object field `+0x58` is an integer tick counter. 0x00600A00 converts it as `seconds = tick * 0.1`. Morale updates stagger on `tick % 5`, also from `+0x58` (0x0057F070). Desync/sync logs are keyed by tick ("SYNC LOG FOR TICK %d", "MULTIMODEL DESYNC DETECTED AT TICK %d"), and MP ticks map onto ranges of battle ticks. The simulation is a deterministic lockstep, and **one battle tick is 0.1 s (10 Hz)**. | 0x00600A00, 0x0057F070 | INFERRED (high) |
| **Battle time control** | Lua `TickPeriod` (0x005D3AA0) and `CycleBattleSpeed` (0x005D02A0). Speed multipliers are exactly {0 (pause), 0.4, 1, 2, 4}. Cycling goes 0→0.4→1→2→4→0. Tweak "Sets the custom time control speed" defaults to 1.0. | | CONFIRMED |
| **Resource / VFS / packs** | 0x01051340 is VFS init: `*.pack`, `non_pack`, boot pack, release packs, patch packs, bink packs, data-dir scan, mods. Errors are reported through a bitmask (1 = too many boot packs, 2 = boot pack has dependencies, 4 = could not load boot pack, …). 0x01095100 enumerates `*.pack` (FindFirstFileW). It also calls GetDiskFreeSpaceA and checks disk geometry. Error strings include "vfs init failed". | 0x01051340, 0x01095100 | CONFIRMED |
| **Database** | `UTILITYLIB::DATABASE_TABLE<EMPIREUTILITY::X_RECORD::BUILDER, CA_STD::CACHE_ALLOCATOR…>`. There are 483 record/builder type names and 679 `*_tables`/`*_table` names (`db_table_names.txt`). Tables are loaded lazily through getters, for example 0x00E20560 for `kv_rules_table`, which logs "Loading database: %s". | `EMPIREUTILITY::*_RECORD` | CONFIRMED |
| **Tweakers / config** | `UTILITYDLL::TWEAKER<T>` statics: **567 registrations** recovered (`tweakers.tsv`). Constructor addresses by type: int pref 0x454300, float pref 0x454380, string pref 0x454400, bool pref 0x454480, int tweak 0x454670, float tweak 0x454730, bool tweak 0x4548C0, int(-1) tweak 0x4545B0. The 110 "environment variable" entries are the **preferences.script keys** (`preferences_keys.tsv`), for example `gfx_frame_rate <int>`, `x_res <int32>`, `battle_difficulty <int>`, `campaign_unit_multiplier <float>` = 0.75. Float getter is 0x0045C180 and int/ptr getter is 0x00586430. | | CONFIRMED |
| **Rendering (Warscape)** | D3D9 with D3DX effects (.fx: `Textured_Rigid.fx`, `weighted.fx`, `Ocean.fx`, `HDR_to_screen.fx`, `ssaox.fx`, `shroud.fx`, `CampaignTerrain.fx`, …). Source dirs `Warscape\Source\platform\` (Engine, TextureManager, BufferManager, FXManager, MeshManager), `scene\nodes` (RigidNode, AnimatedNode, VariantNode), `scene\views` (FrameView, ShadowMapView, HDR_TO_FRAME_VIEW), `systems\` (terrain, campaignterrain, sea, weather, Grass, Vegetation (SpeedTree), Cloud, Imposter, Projectiles, effects, variant, weighted, rigid, Naval, Video). Prefs include `gfx_hdr`, `gfx_low_quality_shaders` (SM2/SM3). | | CONFIRMED |
| **Audio** | Miles (`EmpireSoundManager.cpp`, `Sound\Source\sound\SoundPack.cpp`, `SoundDatabase.cpp`, sound banks `banks/sound_bank_*.xml`, `sounds_packed\sound_bank_database`, `sound_events`). AIL_startup is called at 0x01004AF0. Battle and campaign SoundTrackers exist (Unit, Naval, Terrain, Weather). | | CONFIRMED |
| **Video** | Bink via `Warscape\...\Video\BinkVideoController.cpp`. BinkOpen is called at 0x01215C10. Movies include `NTW_Intro.bik` and `SEGA_logo_sting_HD.bik`. | | CONFIRMED |
| **Input** | DirectInput8 (DirectInput8Create is reached through a thunk at 0x0126DD70) plus Win32 key state. A shortcut system exists (`EMPIREUTILITY::SHORTCUT_HANDLER`, `text/default_keys.xml`). No gamepad support. | | CONFIRMED |
| **UI** | `UiComponentLib` (component.cpp, ComponentTemplateLibrary.cpp; "Failed to open the templates library"). Lua drives the UI. There are **499 script bindings** in 3 tables (`script_bindings.tsv`): front end (registrar 0x4587A0, 112), battle UI (0x59EB40, 129) and campaign UI (0x998C50, 258). Each entry has a name, a C handler VA and a description. | `Component`, `CustomControl` | CONFIRMED |
| **Scripting** | Lua 5.1 with the `UTILITYDLL::LUA::State` wrapper and `operator<< <T>` pushers for 47 bound types (section 6). Scripts include `data/all_scripted.lua`, `campaigns/%S/scripting.lua`, `data/battle_scripted.lua`. Package path `?.lua;` and `/?.lua`. `use_compiled_scripts` is exported as TWEAKER<bool>. Scripting env setup is at 0x00988B00. | | CONFIRMED |
| **Campaign engine** | `EmpireCampaign\Source\...`: model (MarkerManager, PendingBattle), Regions (RegionPopulation, RegionSlots), Character traits/ancillaries, Triggers, controllers, views. Lua-bound types include CAMPAIGN_MODEL, FACTION, REGION, SETTLEMENT, CHARACTER, MILITARY_FORCE, UNIT, BUILDING, FORT, SIEGE, GARRISON_RESIDENCE, REGION_SLOT, RECRUITMENT_ITEM, REGION_RECRUITMENT_MANAGER, CAMPAIGN_COMMAND_QUEUE, CAMPAIGN_THEATRE. Commands are prefixed `CCQ_` (e.g. CCQ_END_TURN, CCQ_SET_GOVERNORSHIP_TAX_RATE). Events: FactionRoundStart, FactionTurnStart/End, RegionTurnStart/End, CharacterTurnStart/End, SlotTurnStart, SlotRoundStart, UnitTurnEnd. | | CONFIRMED |
| **Campaign AI** | `EmpireCampaign\Source\CAI\`: a BDI architecture (EmpireCampaignAIBDI, BDIProperties, BDIMission, BDIRecruitment, ReactiveMissionGoals, BasicGoals). Analysers: Terrain, ThreatAndSupport, RegionTransitionPath, Target. Manager types include END_TURN_MANAGER and DO_DIPLOMACY_AND_END_TURN_MANAGER. Other pieces are the CDIR (campaign director) unit-balance tables and `EMPIRECAMPAIGNAI::NEGOTIATION` and `DIPLOMATIC_ACTION`. | | CONFIRMED (structure) |
| **Battle engine** | `EmpireBattle\Source\model\`: entity (Entity, witness/CombatWitness, action/EntityActionMove), engine (artillery: EngineCrewSlotPush, EngineActionShoot), naval (ship, cannon, Buoyancy, ShipDamage), GroupFormation (`GroupFormations.bin`), locomotion/Locomotive, collisionhard, visibility, wind, misfire, Rules, alliance/VictoryCondition, BattleLog. Also `tick\EmpireBattleTick.cpp`, `RuleSystem\UnitCombatResultQuery.cpp` and `AutoResolver\EmpireAutoresolverStatBased.cpp`. | | CONFIRMED |
| **Battle AI** | `AI\BattleAI.cpp`, `AI\HighLevelPlanner\HighLevelPlanner.cpp`, `AI\MeleeManager\MeleeAnalysers\*` (AI_MELEE_ATTACK_ANALYSER, AI_MELEE_MISSILE_ANALYSER, AIMeleeNavalBoardingAnalyser), tactics such as AI_TACTIC_DOUBLE_ENVELOPMENT and AI_TACTIC_ATTACK_BATTLEGROUP_NAVAL, and log `battle_ai_melee_log.txt` (0x007496E0). | | CONFIRMED |
| **Pathfinding** | Campaign: `/pathfinding.esf`, `/sea_grids.esf`, `cai_pf_debug.txt`. Battle: locomotion and hard collision. Algorithm is UNKNOWN. | | CONFIRMED (data); UNKNOWN (algorithm) |
| **Networking** | Steam lobby and matchmaking (`MPHasLobby`, quick-battle matchmaking), Steam game server, raw WinSock for LAN, `multiplayer/region_qos.xml`, deterministic lockstep with desync detection (`mp_network_sync_logging`, `mp_max_queued_mp_ticks_unhandled`), MP campaign drop-in. | | CONFIRMED |
| **Save/load** | ESF: `data/campaigns/%S/startpos.esf`, `save_games\`, `save_games_multiplayer\`, autosave/quicksave. The campaign-load function **0x00987070** logs "*Opening savegame built with '%S' ('%S')" and "Creating campaign model from savegame...". Battle replays use `.replay` and `battle.replay`. | 0x00987070 | CONFIRMED |
| **Mods** | VFS handles "Could not load the mods." and `*.pack` scanning. Debug option "0 - off, 1 - mod-user, 2 - dev". | | CONFIRMED (details of mod ordering: UNKNOWN) |
| **Threading** | TBB `task_scheduler_init` and parallel tasks (grain tweaker), CreateThread at 0x004C3EF0 and 0x010A6F30, async Bink, Miles thread. | | CONFIRMED |

## 5. Game loop evidence

1. `entry` (0x0126E7D0) calls `__security_init_cookie`, then `FUN_0126e654` (the CRT `__scrt_common_main_seh`: initterm over `.CRT$XCU`, which runs the TWEAKER static constructors). That then calls **wWinMain 0x0048DA70** with hInstance 0x400000. CONFIRMED.
2. WinMain does CoInitialize, then the IGameExplorer access check (it exits if access is denied), then checks `-no_exception_handler`, and finally calls **0x0048C1B0**. CONFIRMED.
3. 0x0048C1B0 runs the init sequence in section 4 and calls the mode loop **0x00485B90** (call site 0x0048CC90). CONFIRMED.
4. 0x00485B90 pumps PeekMessageW/DispatchMessageW. It creates the mode handler for front end, campaign or battle through 0x0048A650 (QPC timing), which drives "Warscape rendering". CONFIRMED (structure). The exact per-frame order is UNKNOWN because the decompile was truncated after 400 lines.
5. The battle model advances in **fixed 0.1 s ticks** (INFERRED high). Render frames interpolate between ticks (UNKNOWN). The battle speed multiplier scales tick rate (0.4/1/2/4).
6. **Per-unit battle tick 0x0057F070** (CONFIRMED order):
   1. `FUN_0056cbf0(0)`
   2. `FUN_005857a0`
   3. `FUN_005821b0`
   4. `FUN_005828a0`
   5. A visibility/event check (`FUN_006011f0`)
   6. **Morale update `0x00582540(0, unit_id%5 == battle_tick%5)`**. The full morale evaluation (0x00584020) runs only on the unit's slot every 5th tick. Otherwise only `FUN_00585be0` (the light update) runs.
   7. Unit averages over soldiers: `+0xC74` = average of soldier vfunc+0xE4, and `+0xC70` = average of vfunc+0xE0. INFERRED: fatigue and one other per-man value.
   8. If the unit is not active (`+0xAA0 == 0`), every soldier's fatigue (`+0x370`) and fatigue state (`+0x374`) are reset to 0.
   9. Commander/ability handling, then `FUN_00582830`.

## 6. RTTI class catalogue summary

- **The RTTI scan is complete and the result is negative.** Napoleon.exe contains only 24 `.?AV`/`.?AU` type descriptors (`rtti_classes.txt`, `rtti_namespaces.txt`):
  - std: 18
  - global: `type_info`, `_com_error`, `IdvFileError`, `IdvRuntimeError`
  - stdext: 2
  
  14 of these have vtables and CHDs (for example `std::ctype<char>` vtable 0x0144663C, 11 methods). There are **no game classes**, because the game was built with /GR-. CONFIRMED. `rtti_vtables.tsv` and `rtti_gameplay_trees.txt` are therefore empty of gameplay content.
- **Substitute catalogue** (`class_names_from_strings.txt`, from `__FUNCSIG__` and template strings). Counts by namespace:
  - EMPIREUTILITY 479 (almost all `*_RECORD` / `*_RECORD::BUILDER`)
  - EMPIRECAMPAIGN 21
  - WARSCAPE 16 (`TEXTURE`, `FX`, `IPARTICLE_EFFECT_GROUP`, `RIGID_RECORD`, `ANIMATED_RECORD`, `EQUIPMENT_THEME_RECORD`, …)
  - CSpeedTreeRT 18
  - EMPIREBATTLE 8 (`UNIT`, `SHIP`, `PROXY`, `ENTITY_DISPLAY`, `NAVAL_OBJECT_DISPLAY`, `BATTLE_SETUP_INFO`, `ENTITY_ANIMATION_ACTION_TABLE`, `ENTITY_DISPLAY_SHARED_POSE_TABLE`)
  - UTILITYDLL 5 (`LUA::State`, `LUA::Pointer`, `I3D_INSTANCE`, `I3D_MOUSE_DELEGATE`)
  - CA 4 (`UniString`, `ImageType<Pixel8888>`)
  - EMPIRECOMMON 4 (`GAME_CORE`, `EMPIRE_MP_BASE`, `EMPIRE_MP_CAMPAIGN_DROP_IN`, `EMPIRE_ONLINE_PRESENCE`)
  - EMPIRE 2 (`FRONT_END`, `EMPIRE_MP`)
  - EMPIRECAMPAIGNAI 2
  - UTILITYLIB 2 (`DATABASE_TABLE`, `ILIST`)
  - CA_STD 2
  - AI-analyser classes: `AI_MELEE_ATTACK_ANALYSER`, `AI_MELEE_MISSILE_ANALYSER`, `AI_TACTIC_DOUBLE_ENVELOPMENT`, `AI_TACTIC_ATTACK_BATTLEGROUP_NAVAL`, `AUTO_GENERATOR`, `FARM_AUTO_GENERATOR`, `TERRAIN_DISPLAY`, `VEGETATION_FIELD`, `GOVERNMENT`, `WS_LOADING_SCREEN_IMP`, `BATTLE_ENV`, and others.
- **The 47 Lua-bound types** are the best map of the gameplay object model (CONFIRMED):
  - EMPIREBATTLE: UNIT, SHIP, PROXY, BATTLE_SETUP_INFO, ENTITY_DISPLAY, NAVAL_OBJECT_DISPLAY
  - EMPIRECAMPAIGN: CAMPAIGN_MODEL, FACTION, REGION, REGION_SLOT, SETTLEMENT, CHARACTER, MILITARY_FORCE, UNIT, BUILDING, FORT, SIEGE, SIEGEABLE_GARRISON_RESIDENCE, GARRISON_RESIDENCE, RECRUITMENT_ITEM, REGION_RECRUITMENT_MANAGER, CAMPAIGN_COMMAND_QUEUE, CAMPAIGN_THEATRE, CAMPAIGN_CAMERA_MANAGER, EPISODIC_SCRIPTING_ENV, POST_BATTLE_NAVAL_UNIT, POST_BATTLE_NAVAL_INTERFACE
  - EMPIRECAMPAIGNAI: NEGOTIATION, DIPLOMATIC_ACTION
  - EMPIRECOMMON: GAME_CORE, EMPIRE_MP_BASE, EMPIRE_MP_CAMPAIGN_DROP_IN, EMPIRE_ONLINE_PRESENCE
  - EMPIRE: FRONT_END, EMPIRE_MP
  - EMPIREUTILITY: EMPIRE_DATABASES, EVENT, EVENT_RECORD, UNIT_RECORD, TECHNOLOGY_RECORD, SHORTCUT_HANDLER, KEYBOARD_SHORTCUT_DESCRIPTION
  - UI: Component, ComponentState, ComponentImageMetrics, CustomControl

## 7. Important functions

| Address | Evidence | Interpretation | Confidence |
|---|---|---|---|
| 0x0126E7D0 | PE entry | CRT entry | CONFIRMED |
| 0x0126E654 | `__scrt_*`, initterm, call to 0x0048DA70 | `__scrt_common_main_seh` | CONFIRMED |
| 0x0048DA70 | IGameExplorer, `-no_exception_handler` | **wWinMain** | CONFIRMED |
| 0x0048C1B0 | init-phase strings, CreateMutexW | App::init_and_run | CONFIRMED |
| 0x00485B90 | DispatchMessageW, mode names | main loop and mode dispatch | CONFIRMED |
| 0x0048A650 | QPC, "Created new handler" | mode-handler switch | CONFIRMED |
| 0x01051340 | "*.pack", boot/release/patch/bink/mod errors | VFS::init (pack mounting) | CONFIRMED |
| 0x01095100 | FindFirstFileW + "*.pack" | pack directory enumeration | CONFIRMED |
| 0x00987070 | "Creating campaign model from savegame...", startpos.esf | campaign load (new game or ESF save) | CONFIRMED |
| 0x00988B00 | `data/all_scripted.lua`, `campaigns/%S/scripting.lua`, CampaignName, LocalFaction, CommandQueue | campaign Lua environment setup | CONFIRMED |
| 0x00E20560 / 0x00E203C0 / 0x00E202F0 / 0x00E20490 | "Loading database: %s" with kv_rules/kv_morale/kv_fatigue/kv_naval_morale | lazy KV-table getters (cached at db+0x6CC, …) | CONFIRMED |
| 0x00F42950, 0x00F41F90, 0x00F41D00, 0x00F424B0 | key string lists | KV holder builders, one TWEAKER per key, stride 0x60 (`kv_layout.tsv`) | CONFIRMED |
| 0x00F3A830 / 0x00F3A9B0 / 0x00F3A8F0 | `cvttss2si` / float | add KV key as **int (truncated)** / float / special | CONFIRMED |
| 0x0045C180 / 0x00586430 | called with `this` = tweaker | TWEAKER float `get()` / TWEAKER int `get()` (returns pointer) | CONFIRMED |
| 0x0057F070 | per-unit calls; `%5` stagger | battle land-unit tick | CONFIRMED |
| 0x00582540 → 0x00584020 | kv_morale reads | **unit morale evaluation and state machine** | CONFIRMED |
| 0x0053C720 | was_attacked_in_*, casualty ratios | morale modifiers (attack direction, casualties, blood) | CONFIRMED (code); INFERRED (key names in chained branches) |
| 0x00670F40 / 0x00671230 | kv_fatigue reads | soldier fatigue accumulation / fatigue state machine | CONFIRMED |
| 0x006F8020 → 0x006FBC40 / 0x006AC860 | kv_naval_morale | ship morale | INFERRED |
| 0x005646A0 | projectile range (+0x60), fire_on_walls_range_modifier | unit missile range (48 callers) | CONFIRMED |
| 0x006D8B00 | fire_on_walls_accuracy_modifier | unit accuracy getter | CONFIRMED |
| 0x006A5CE0 | *_projectile_calibration_target_area | shot dispersion (calibration) and a range histogram | CONFIRMED |
| 0x00639E40 | firing_drill_* / fire_and_advance / fire_on_walls reload modifiers | reload-time computation | CONFIRMED (keys used) |
| 0x00534440, 0x0054DB50 | bayonet_melee_attack_bonus | bayonet fix/unfix adjusts the melee attack stat | CONFIRMED |
| 0x0078D200, 0x00759860, 0x0078E8A0, 0x0078EE10 | unit_combat_query_* tweakers | autoresolve kill rates and the engagement simulation | CONFIRMED |
| 0x00535B30 | `wind_level_0..4` | battle wind init (seeded random) | CONFIRMED |
| 0x005F0830, 0x007ADD80, 0x005F0880, 0x005FC070, 0x00A9D9C0, 0x00A9DA10, 0x00BBD6A0, 0x00BBD6E0 | 214013/2531011 | **game RNG helpers** (12.7) | CONFIRMED |
| 0x0127FB28 | `>>16 & 0x7fff`, per-thread state | CRT `rand()` | CONFIRMED |
| 0x0115FEE0 | 1103515245/12345, XOR | byte-stream XOR descrambler with an LCG keystream (seekable) | CONFIRMED (code); purpose UNKNOWN |
| 0x005D3AA0 / 0x005D02A0 | Lua TickPeriod / CycleBattleSpeed | battle speed | CONFIRMED |
| 0x008A3E10 / 0x008A3E90 | condition handlers | RegionTaxTownWealthGrowthReduction (= region+0xDC × 100) / RegionTownWealthGrowth (= region+0xD8) | CONFIRMED |
| 0x00FFC090 / 0x00FFC070 | SteamAPI_Init / RunCallbacks | Steam init / per-frame callbacks (caller 0x00FFA9F0) | CONFIRMED |

## 8. Structures and relationships

There are no RTTI inheritance trees (see section 6). Layouts recovered from code are below. All are partial. Offsets are CONFIRMED as accessed; their meaning is INFERRED unless stated.

**KV holder** (kv_rules, kv_morale, kv_fatigue, kv_naval_morale):
- `+0x04..+0x0C` vector header.
- Then `TWEAKER` slots of 0x60 bytes each: slot *i* is at `0x10 + 0x60*i`, in the key order of `kv_layout.tsv`.
- Code reads a value as `lea ecx,[holder+slot]; call 0x586430` (int) or `call 0x45C180` (float).

CONFIRMED.

**Unit morale component** (the `this` of 0x00584020, an `int[]`):

| Field | Meaning |
|---|---|
| `[1]` | owning unit pointer |
| `[0x8]/[0x9]` | count/array of transient morale effects, 12 bytes each `(id, value, ?)` |
| `[0xA]` | **morale state** (0..7) |
| `[0xB]` | behaviour mode: 0 normal, 2 routing, 3 shattered, 4 special |
| `[0xC]` | **current morale value (int)** |
| `[0xD]` | persistent morale bonus |
| `[0xF..0x10]` | linked list of active effects (value at node+0x10) |
| `[0x17]` | surprise timer |
| `[0x18]` | broken/rout timer |
| `[0x19]` | waver timer |
| `[0x1A]` | charge-bonus timer |
| byte `+0x51` | "suppress shaken clamp" flag |
| byte `+0x5A` | number of times routed |

**Land unit** (the `this` of 0x0057F070):

| Field | Meaning |
|---|---|
| `+0x14` | battle context (`→+8` = battle; `battle+0x58` = **tick counter**) |
| `+0x180` | morale stat added to morale_base |
| `+0x18A` | impetuous-allowed flag |
| `+0x194` | formation/type class used by the fighting_cavalry scaling |
| `+0x2CC/+0x2D0` | soldier count/array |
| `+0xAA0` | active flag |
| `+0xC70/+0xC74` | per-soldier averages |
| `+0xC80/+0xC84` | under artillery / under projectile fire flags |
| `+0xCB4` | total casualty ratio |
| `+0xCBC` | recent casualty ratio |
| `+0xCC0` | kill ratio ("blood") |
| `+0xCC4` | extended casualty ratio |
| `+0xD08` | morale-modifier category (1..7) |
| `+0xD48` | unit index (byte) |
| `+0xDC4` | → projectile/weapon record (`+0x60` = range, int) |

**Soldier/entity** (fatigue):

| Field | Meaning |
|---|---|
| `+0x1B8` | current action enum (0..0x4E) |
| `+0x1A0` | ground gradient |
| `+0x1F0` | → unit |
| `+0x370` | fatigue (int) |
| `+0x374` | fatigue state 0..5 |

**Autoresolve query** (0x0078D200):

| Field | Meaning |
|---|---|
| `+0x08/+0x0C` | unit A/B (`+0xC8` = starting men) |
| `+0x18/+0x1C` | current men A/B |
| `+0x24` | signed outnumbering ratio r |
| `+0x28` | signed missile modifier r2 |
| `+0x30/+0x34` | melee potential A/B |
| `+0x38/+0x3C` | missile potential A/B |

**RNG object**: `u32 state`. It is embedded at several offsets: `this+0` in several helpers, `+0x71C` in one owner and `+0x60` in another, plus battle object `+0x50`. CONFIRMED.

## 9. Hints for Worker 2

- **DB tables**: 679 table identifiers are in `db_table_names.txt` (for example `unit_stats_land_tables`, `projectiles_table`, `fatigue_effects_table`, `kv_*`). There are 483 `EMPIREUTILITY::*_RECORD(::BUILDER)` record types in `class_names_from_strings.txt`. The DB name-to-struct mapping uses `UTILITYLIB::DATABASE_TABLE<…BUILDER…>::record_index`.
- **KV tables**: `_kv_rules`, `_kv_morale`, `_kv_fatigue` and `_kv_naval_morale` have key lists and value types in `kv_layout.tsv`. **Most keys are truncated to int when loaded** (`cvttss2si` at 0x00F3A899). Floats are kept only for `melee_height_delta_*`, `relative_melee_height_delta_divisor`, `attackpower_*_range_multiplier`, `bayonet_ring_reload_time_penalty`, `projectile_damage_distance_multiplier`, `misfire_*`, `*_calibration_target_area`, `broadsides_damage_modifier`, `ship_bonus_close_mod`, `ship_penalty_far_mod`, `magazine_explosion_interrupt_chance` and `ship_repair_rate`. The `special_ability_*` keys use a third adder (0x00F3A8F0). A Rust loader must apply the same truncation.
- **File formats referenced**: `.pack` (boot, release, patch, bink, mod packs; `non_pack` overrides), `.esf` (`startpos.esf`, `regions.esf`, `pathfinding.esf`, `trade_routes.esf`, `poi.esf`, `sea_grids.esf`, `BorderPoints.esf`, saves), `.rigid_model`, `.animatable_rigid_model`, `.unit_variant` (`.soldier.unit_variant`, `.musician.unit_variant`), `.anim`, `.anim_sound_event`, `.logic` (Verlet items), `.tai` (texture atlas), `.fx`/`.fx_fragment`, `.xml` (`CampaignMap.xml`, `LandBattle.xml`, `sound_settings.xml`, `ui.xml`, `text/default_keys.xml`, battle map `definition.xml`), `GroupFormations.bin`, `battle_setup_info.dat`, `metadata.dat`, `.replay`, `.ai_history.xml`, `.cdir.txt`, `.script.txt`, `.dds`/`.tga`/`.jpg`, `.bik`, `.mp3`/`.wav` (advisor `sounds_advisor_*.pack`), and the heightmap `heightmaps/default.tga`.
- **Lua**: 499 C bindings with descriptions are in `script_bindings.tsv`. Script entry points are `data/all_scripted.lua`, `campaigns/%S/scripting.lua` and `data/battle_scripted.lua`. Campaign condition functions such as `RegionTownWealthGrowth()` are registered with handler pointers, using the pattern at 0x0041CD90.
- **Preferences**: 110 `preferences.script` keys with types, defaults and descriptions are in `preferences_keys.tsv`.
- **Data-loading paths**: VFS init 0x01051340. KV getters 0x00E202F0..0x00E20560 (other table getters sit next to them in the same 0x00E1xxxx–0x00E2xxxx region, for example `fatigue_effects_table` at 0x00E1C890). Campaign start/save load 0x00987070.

## 10. Open questions and UNKNOWNs

1. **Exact per-frame order** inside 0x00485B90 and the campaign handler: render/sim interleave, and whether render interpolates between battle ticks. UNKNOWN.
2. **RESOLVED, see §12.9.** **Real-time melee hit and kill resolution**: the code that consumes `relative_melee_experience_multiplier`, `armour_/defense_melee_*_divisor`, `factor_attackdir_*` and `melee_xholds_*` was not located. Those reads did not go through the direct `lea ecx,[holder+slot]` pattern; the holder is probably cached in a member. Next step: search for `mov reg,[x]; add reg,0x670 / 0x6D0 / 0x7F0 / 0x850` near `call 0x586430`, or find the callers of the kv_rules getter (20 callers of 0x00E20560 are listed in decomp5.c). UNKNOWN.
3. **RESOLVED, see §12.10.** **Missile hit chance**: `missile_distance_for_half_chance_hit*` is read in 0x0053C720's region and in other places. The formula is not yet extracted. UNKNOWN.
4. **Campaign turn phase order**: FactionRoundStart, FactionTurnStart, RegionTurnStart, CharacterTurnStart and UnitTurnEnd exist. Event classes share ~0xBC-byte vtables around 0x01356300 (the name getter is at 0x008BDE70 for FactionRoundStart). Next step: locate where those vtables are instantiated. UNKNOWN.
5. **Economy and tax formulas**: only the getters were found (region+0xD8 = town wealth growth, +0xDC = tax growth-reduction fraction). UNKNOWN.
6. Meaning of `+0xD08` morale categories 1..7 (fixed modifiers +6/+4/+2/−4/−8) and of `FUN_0055c9a0(7)` (the range ×0.8 state). UNKNOWN.
7. **RNG seeding** for battles: each RNG owner is seeded separately. `FUN_004aa8f0`→`FUN_0049d800` supplies a seed in 0x004B8700. GetTickCount seeds only 0x004A1A70 (front-end/background) and 0x004B9580/0x004B9790. The MP-synchronised battle seed source is UNKNOWN.
8. Purpose of the LCG-XOR descrambler at 0x0115FEE0 (no direct callers, so it is probably reached through a vtable). UNKNOWN.

## 11. Output files (all in `worker1\`)

`WORKER1_REPORT.md` (this file), `pe_report_rs.txt`, `strings_all_rs.tsv`, `strings_categorized_rs.txt`, `class_names_from_strings.txt`, `rtti_classes.txt`, `rtti_namespaces.txt`, `tweakers.tsv`, `preferences_keys.tsv`, `script_bindings.tsv`, `db_table_names.txt`, `kv_layout.tsv`, `kv_usage.tsv`, `kv_getter_reads.tsv`, `rng_constants.txt`, `rng_lcg_sites.txt`, `import_refs.txt`, `xref_results.txt`, `ghidra_out\functions.tsv`, `ghidra_out\decomp1..7.c`, `ghidra_scripts\*.java` and `targets*.txt`, `run_ghidra.ps1`, `re_tools\` (Rust source).

---

## 12. Fidelity spec (implementable)

Notation: `u32`/`i32` wrap; `f32` is single precision. "slot X" means a KV key value (int-truncated unless marked float).

### 12.1 RNG (CONFIRMED)

```
struct CaRng { state: u32 }
fn next16(&mut self) -> u32 { self.state = self.state.wrapping_mul(214013).wrapping_add(2531011); self.state >> 16 }  // 0..=65535, NOT masked to 0x7fff
```

Helpers, exactly as compiled:

- **uniform_below(n)** @0x005F0830 (20 callers). This is rejection sampling that rejects *low* values:
  ```
  loop { r = next16(); if r > (0xFFFF % n) { return (r % n) & 0xFFFF } }
  ```
  The comparison is `r <= 0xFFFF % n → retry`.
- **percent_0_100()** @0x007ADD80 (17 callers): `loop { r = next16(); if r >= 88 { return r % 101 } }`.
- **int_range(lo, hi)** @0x005F0880 (and copies with state at owner+0x71C @0x00A9D9C0 and owner+0x60 @0x00BBD6A0):
  ```
  span = (hi - lo) as u32; r = ((span + 1) * next16()) / 0xFFFF   // u32 math
  return lo + min(r, span)
  ```
- **unit_float()** @0x005FC070: `next16() as f32 * 1.5259022e-05` (= 1/65535, so the range is [0, 1] inclusive).
- **float_range(a, b)** @0x00A9DA10, 0x00BBD6E0: `next16() as f32 * 1.5259022e-05 * (b - a) + a`.
- **Stateless hash use** (e.g. wind @0x00535B30): `((seed*214013 + 2531011) >> 16) as f32 * 6.103609e-05` (= /16384, range 0..4).
- The same LCG is inlined at **732 sites** across about 286 functions (`rng_lcg_sites.txt`). Each owner keeps its own state, and battle-wide state lives at battle+0x50 (0x006A5220). CONFIRMED.
- CRT `rand()` (0x0127FB28; `(s>>16)&0x7FFF`) is used by only 3 callers. std::mt19937 exists but sits outside the gameplay code (INFERRED).

### 12.2 Battle timing (INFERRED high / CONFIRMED where marked)

- `dt_tick = 0.1 s`. Battle time is `tick_counter * 0.1` (0x00600A00).
- Time multipliers are {0, 0.4, 1, 2, 4}. Cycle order is 0→0.4→1→2→4→0 (CONFIRMED, 0x005D02A0).
- The morale full update runs when `unit_id % 5 == tick % 5`, so each unit is evaluated every 0.5 s (CONFIRMED, 0x0057F070).

### 12.3 Morale (land), 0x00584020 (CONFIRMED structure; slot names from read order)

```
if unit.active_state(+0xAA0) != 1 or flags(+0x55|+0x56|+0x57): return
morale = slot(morale_base) + unit.morale_stat(+0x180) + persistent_bonus
apply sub-evaluators in order: 0x53BC70, 0x53CBD0, 0x53C720 (12.4), 0x53E450, 0x53BB40, 0x53B970, 0x53B7B0, 0x53BC20 (column formation)
for each transient effect (id,val): add_effect(id,val)                 // 0x54E3C0
if unit.under_projectile_fire(+0xC84): add_effect(0x24, slot ume_concerned_attacked_by_projectile)
if unit.under_artillery_fire(+0xC80):  add_effect(0x23, slot ume_concerned_attacked_by_artillery)
if surprise_timer >= 0:                add_effect(0x28, slot ume_concerned_surprised); suppress=true
if charge_timer > 0:                   add_effect(0x0E, slot charge_bonus)
morale += sum(active effect values)
if suppress and !FUN_53e980(): suppress=false
if state==5 && morale < ums_wavering_threshold_lower && !suppress && waver_timer>0: morale = ums_wavering_threshold_lower
if state==6 && morale < ums_broken_threshold_lower: state = 7
hysteresis (all comparisons strict, ints):
 0 impetuous: morale < impetuous_lower        -> 1
 1 eager:     morale > eager_upper && unit.flag(+0x18A)==1 -> 0 ; morale < eager_lower -> 2
 2 confident: > confident_upper -> 1 ; < confident_lower -> 3
 3 steady:    > steady_upper -> 2 ; < steady_lower -> 4
 4 shaken:    > shaken_upper -> 3 ; < shaken_lower -> 5 (fires event)
 5 wavering:  only if waver_timer < 0: > wavering_upper -> 4 ; < wavering_lower && FUN_532370() -> 6
 6 broken:    only if rout_timer < 0: > broken_upper -> 5 ; < broken_lower -> 7
state 6/7 -> mode = (7 ? 3 shattered : 2 routing) [special case mode 4]; else mode 0
on entering 5: waver_timer = FUN_53e4d0() ; on mode->2: rout_timer = FUN_53a720(), rout_count++
```

The state names follow the `ums_*` key prefixes (INFERRED). The timer formulas (0x0053E4D0, 0x0053A720) are UNKNOWN; they probably use `waver_base_timeout` and `broken_finish_base_timeout`.

### 12.4 Morale modifiers, 0x0053C720 (thresholds CONFIRMED; key names INFERRED from the threshold numbers)

- **Category `unit+0xD08`**: 1 → effect 5 = +6; 2 → +4; 3 → +2; 6 → effect 0x17 = −4; 7 → −8. These are hard-coded.
- **Per attacker**:
  - If the attacker is not in the excluded set: `v = slot(was_attacked_in_front)`, doubled if `FUN_55ac20()`.
  - Formation adjust: if the unit class (`+0x194`) is in 0/1 then −4, if 2 then −2, or +2 under another condition.
  - Then `add_effect(0x18, v)`.
- Any flank attacker gives `add_effect(0x19, slot was_attacked_in_flank)`. Any rear attacker gives `add_effect(0x1A, slot was_attacked_in_rear)`.
- Fighting cavalry gives `v = slot fighting_cavalry`, scaled by unit class: 0/1 → full, 2 → /2, 3 → /4 (signed shift), 4/5 → none. Then `add_effect(0x25, v)`.
- **Total casualties** (`+0xCB4`, strictly greater): >0.9 → total_casualties_penalty_90, >0.8 → _80, >0.6 → _60, >0.4 → _40, >0.2 → _20. Effect 0x20.
- **Recent casualties** (`+0xCBC`):
  - If `≥ recent_casualties_shock_threshold * 0.01`, set the shock/suppress flag.
  - ≥0.5 → _50, ≥0.33 → _33, ≥0.15 → _15, ≥0.10 → _10, ≥0.06 → _6. Effect 0x21.
- **Extended casualties** (`+0xCC4`): ≥0.8 → _80, ≥0.5 → _50, ≥0.33 → _33, ≥0.15 → _15, ≥0.1 → _10. Effect 0x22.
- **Blood** (`+0xCC0`): ≥0.125 → blood_bonus_12, ≥0.075 → _7, ≥0.05 → _5. Effect 6.

### 12.5 Fatigue, 0x00670F40 and 0x00671230 (CONFIRMED structure)

```
delta = slot[kv_fatigue action-key for entity.action(+0x1B8)]           // big switch on action enum
if idle-type action: delta = slot idle (+ slot idle_rain if raining | + slot idle_snow if snowing)
if moving on slope (flag +0x118 ...): g = entity.gradient(+0x1A0)
     g>0.2 -> m=very_steep ; g>0.1 -> m=steep ; g>0.05 -> m=shallow ; else none
     delta = (m * delta) / 100        // integer
delta adjustments from unit (+0x1F0): -1 if unit flag +0x18E; climate terms from battle (+0x2C,+0x30) unless unit flags +0x18F/+0x190
fatigue(+0x370) += delta + terms
state machine (0 fresh..5 exhausted), strict compares:
 0: fatigue > active -> 1
 1: fatigue < active -> 0 ; fatigue > winded -> 2
 2: < winded -> 1 ; > tired -> 3
 3: < tired -> 2 ; > very_tired -> 4
 4: < very_tired -> 3 ; > exhausted -> 5
 5: fatigue < exhausted -> 4
clamp fatigue to [threshold_fresh, threshold_max]
```

A unit's (or ship's) fatigue is the integer mean of its soldiers' values (0x006FB080).

### 12.6 Missile range and accuracy (CONFIRMED)

- **range** (0x005646A0):
  ```
  if !has_missile || !weapon: 0
  elif on_walls: weapon.range + slot fire_on_walls_range_modifier
  elif state(7): weapon.range * 0.8
  else weapon.range
  ```
  The weapon range is an int.
- **accuracy** (0x006D8B00):
  ```
  acc = unitrec(+4→+0x70) f32
  if on_walls: acc += slot fire_on_walls_accuracy_modifier
  then mode(+8): if flag(+0xC)==0 { mode1 → +20 ; mode2 → +30 } else { mode1 → +15 }
  ```
  The mode meaning is UNKNOWN (perhaps an ability or experience tier).
- **shot dispersion** (0x006A5CE0):
  - `A` = calibration_target_area chosen by projectile category: default; artillery; naval when the weapon class is 2; land mortar when class is 3.
  - `aspect` = 1.0, or 0.70710677 when the owner type (`+0x4C`) is 0 or 0xB.
  - `D` = weapon calibration distance (FUN_00DAB9D0). `R` = distance to target (`+0x50`).
  - `w = sqrt(A / D)`.
  - `spread_x = 2 * (0.5 / aspect) * w / R`.
  - `spread_y = 2 * w * aspect * 0.5 / R`.
  - These are passed to 0x006A5BD0 together with `w`.

### 12.7 Autoresolve kill rates (CONFIRMED), 0x0078D200

`Kmel` = 0.2, `Kmis` = 0.2, `Mmel` = 0, `Mmis` = 0, `Mnav` = 2, landKillMult = 0.1, fuzz = 0.2, rout = 0.4, shaken = 0.9 (tweakers).

```
pot(P, mult, men, start) = P*mult*men/(start*start) ; if 0 -> 1
r=+0x24, r2=+0x28
multA_mel = r<0 ? 1+Mmel*|r| : 1 ;  multB_mel = r>0 ? 1+Mmel*r : 1
pA = pot(meleeA, multA_mel, menA, startA) ; pB = pot(meleeB, multB_mel, menB, startB)
killA_mel = Kmel*(pA/pB)*(r<=0 ? 1 : 1-|r|) ; killB_mel = Kmel*(pB/pA)*(r>=0 ? 1 : 1-|r|)
missile: same with Mmis, missileA/B; then if r2<0: pA*=1-|r2| ; if r2>0: pB*=1-r2 (before zero check)
```

**Engagement loop** (0x00759860, simplified):
- Each side's per-step losses = `remaining_enemy * killrate * landKillMult`, clamped to `[0.05, remaining]`.
- Iterate until one side's losses reach its rout threshold: `(1-start fraction)`, using base_rout_point / shaken (0.9).
- The function returns a result code (0, 1 or 2) and the casualty fractions of both sides.
- The precise argument roles are INFERRED.

### 12.8 Misc constants (CONFIRMED)

- Wind (0x00535B30): `wind_level` thresholds on |v| are 0.58317894, 0.68722874, 0.82029885 and 0.91646695. Speed `= clamp(rand4 + f(|v|) - 2, 2, 17)`. Max wind tweak is 400.
- Shot range histogram bins are 25, 50, 75, 100, 150, 200, 300, 400, 500, 600 and 1000 (statistics only).
- `campaign_unit_multiplier` default = 0.75. Famous battles: max 4, minimum force 1000 men per side, 8 units, 1000 casualties.
- Ship tweaks:
  - incendiary chance low 0.8 / medium 10 / high 100
  - combustion_modifier 0.005
  - num_critical_fires 10
  - buoyancy intake 0.5 / downflow 0.5 / upflow 0.1
  - reload_display_variance 1.05
  - cannon_impulse 1000, projectile_impulse 25

The full table is in `tweakers.tsv`.

### 12.9 Real-time melee hit / kill resolution (resolves §10.2)

All pseudocode in this section is reconstructed from Ghidra output and the disassembly (`ghidra_out/melee2..10.c`, `dis_dab5f0.txt`). It is not the original source.

**Where the formula came from.** The binary still contains the developers' combat-log format strings at 0x01393D80–0x013957FC, for example "Calculated Hit Number", "Kill Chance = hn(%d)>=-6 = 154+(hn*13)" and "Result = roll(%d)<kc(%d)+xholds[%d][0](%d) = KNOCKDOWN". These strings led directly to the functions below. Where a log string and the code disagree, **the code is authoritative**.

**Call chain**
- 0x00664E80 → **0x006AFE20** picks the fighting pair.
- That calls **0x00DAA290** to resolve one melee blow.
- 0x00DAA290 calls **0x00DAB5F0** for the hit number (hn) and **0x00DADA40** to turn hn into a kill chance (kc).

**Interface objects**
- The rules adapter has vtable **0x01337A28**. Method `+4*i` returns kv_rules key *i* (key order as in `kv_layout.tsv`). I verified this by decoding each getter (`mov ecx,[ecx+4]; add ecx,SLOT; call get`).
- The combatant info object (A = attacker, D = defender) has vtable **0x0133A52C** and wraps one soldier entity. `unit` below is entity+0x1F0, the runtime battle unit.
- The encounter object E has vtable **0x0133A5F4**.

**Combatant stat sources.** The runtime unit holds a copy of the record's stat block at unit+0x158, which is record+0x138 shifted by 0x20. I matched both through the UI panels 0x005CD370 (runtime) and 0x00E02260 (record), and the copy from the BUILDER is in 0x00E8F1D0. Column numbers are unit_stats_land columns from DB_BUILDERS.md.

| combatant method | meaning | runtime field | unit_stats_land column (BUILDER offset) | conf. |
|---|---|---|---|---|
| +0x18 | melee attack | unit+0x170 | col 34 (+0x17C) | CONFIRMED (UI label "Melee") |
| +0x1C | charge bonus | unit+0x174 | col 35 (+0x180) | CONFIRMED (UI "Charge") |
| +0x0C | armour | unit+0x158 | col 11 (+0x6C) | INFERRED (part of the UI "Defence" sum; the +0x20 shift is consistent across all labels) |
| +0x14 | shield | unit+0x178 | col 36 (+0x184) | INFERRED |
| +0x10 | melee defence | unit+0x17C | col 37 (+0x188) | INFERRED |
| +0x28 | bonus vs cavalry | unit+0x184 | col 51 (+0x1E4) | INFERRED |
| (UI only) accuracy / reloading / morale / ammunition | | unit+0x160 / +0x164 / +0x180 | cols 26 (+0x134), 27 (+0x138), 42 (+0x1BC), 31 (+0x160) | CONFIRMED (UI labels) |
| +0x08 | per-soldier fatigue level (entity vfunc +0xE0) | | | INFERRED |
| +0x20 entrenchment, +0x24 environment, +0x2C, +0x34 | constant 0 (stub functions) | | | CONFIRMED (stub bytes `33 c0 c3` etc.) |
| +0x30 "piercing weapon" | **always true** (`mov al,1`) | | | CONFIRMED |
| +0x38 | anti-charge / spear: class 4 or 8, or class 3 with action 0x3E..0x40, or entity flag +0x36C / class 0xC/0xD | | | CONFIRMED (logic); INFERRED (meaning) |
| +0x40 charging (entity action == 0xD), +0x44 braced (entity+0x5C), +0x48 cavalry, +0x4C infantry, +0x50 (category 1 variant), +0x54 in square, +0x58 on walls, +0x5C in building | | | | CONFIRMED (code); meanings INFERRED |

- **Attack and charge get two modifiers** (0x006A8290, 0x006B30E0):
  - `attack = round(unit.melee * FE.attack_mult)` and `charge = round(unit.charge * FE.charge_mult)`.
  - FE comes from `unitrec.table[unit.fatigue_state]` (0x00649520): it reads record+0xE4 + 4×unit+0xC70, and uses FE+0x1C for attack and FE+0x14 for charge. INFERRED: this is the fatigue_effects row.
  - Then **attack += 4 or 8** from a per-army "level" (0x006AD6E0): army+0x224 is a flag and army+0x234 a level (0..2). For flag 0, after battle tick 6 the level drops by (my_strength − enemy_strength)/5 when this side is stronger. Flag set: +4 only at level 1. Flag clear: +4 at level 1, +8 at level 2. INFERRED: a difficulty handicap.

**Hit number** (0x00DAB5F0; every term CONFIRMED from the disassembly):

```
hn  = R.relative_melee_fatigue_multiplier * (D.fatigue - A.fatigue)
hn += R.factor_attackdir[E.dir]      // E.dir: 0 -> factor_attackdir_front, 1 -> flankleft, 2 -> flankright, 3 -> rear
hn += A.attack
if A.charging:
    cp = A.charge
    if D.braced and E.dir == 0: cp = cp / 2          // C integer division, truncates toward 0
    hn += cp
if A.braced and E.dir == 0 and A.anti_charge and !A.charging and D.charging:
    hn += D.charge / R.melee_charge_factor_power_divisor                  // "charge reflect"
hn += A.environment(E.ground)                         // = 0 for land combatants
cat = 0
if A.cavalry and (D.infantry or D.f50):
    if !D.in_square:  cat = R.hnbonus_melee_cavalry_v_infantry
    elif E.dir == 0:  cat = R.hnbonus_melee_cavalry_v_squareinfantry
elif A.infantry and D.cavalry:
    cat = -(A.in_square ? R.hnbonus_melee_cavalry_v_squareinfantry : R.hnbonus_melee_cavalry_v_infantry)
if D.cavalry: cat += A.bonus_vs_cavalry
if A.anti_charge and D.cavalry: cat += R.hnbonus_bayonet
hn += cat
hn -= D.armour / R.armour_melee_piercing_divisor      // "piercing" is always true for land combatants
if E.dir in (0, 1): hn -= D.shield + D.defence / R.defense_melee_piercing_divisor
elif E.dir == 2:    hn -= D.shield
                                                       // rear (3): no shield, no defence
hn -= R.melee_entrenchement_level_multiplier * D.entrenchment   // entrenchment = 0
hn -= round(clamp(E.height_delta, R.melee_height_delta_min, R.melee_height_delta_max) / R.relative_melee_height_delta_divisor)
if A.on_walls or D.on_walls:         hn += 10
elif A.in_building or D.in_building: hn += 20
```

- All arithmetic is int32 except the height term, which is float and then rounded (x87 default rounding).
- `E.height_delta` (0x006CCAB0): `D.z − A.z` normally. If either side is charging it becomes `(D.z − A.z) / horizontal_distance²` (squared, not square-rooted, CONFIRMED).
- `relative_melee_experience_multiplier` (key 0) is **not used** in this function.

**Kill chance** (0x00DADA40, CONFIRMED from the bytes; the log strings 154/112/76 are stale):

```
kc = hn >= -6  ? 254 + 13*hn
   : hn >= -12 ? 184 + 6*hn
   :             125 + 3*hn
kc = clamp(kc, 1, 990)                                 // 0x00DAC2A0
n = E.attackers_on_target                              // encounter vfunc +0xC
if n > 1 and kc > 0: kc = clamp(kc + (n-1)*round(kc*0.5), 1, 990)
```

**Outcome** (0x00DAA290, CONFIRMED):

```
xi = hn < R.melee_hn_to_xholds_0_max ? 0 : hn < R..._1_max ? 1 : hn < R..._2_max ? 2 : (hn >= R..._3_max ? 4 : 3)
roll = 1 + min(999, (next16() * 1000) / 0xFFFF)       // 0x00DADB20; RNG state = battle+0x50 (12.1 LCG)
if   roll < kc:                                  KILL (5)
elif roll < kc + R.melee_xholds_knockdown_{xi}:  KNOCKDOWN (3)
elif roll < kc + R.melee_xholds_knockback_{xi}:  KNOCKBACK (2)
elif roll < kc + R.melee_xholds_stepback_{xi}:   STEPBACK (1)
else:                                            MISS (0)
```

The xholds are cumulative offsets above kc, so the data should increase within each tier.

**Pair selection and RNG count per exchange** (0x006AFE20, CONFIRMED; meanings INFERRED):
1. Candidate attackers are the entities with the highest priority. Priority (0x006AF3B0) is 0 if the entity is not charging (action ≠ 0xD). When charging: 1 for entity type 0, and 2 for types 1–2 (3 if FUN_0055C210 passes).
2. Each candidate's weight is `float(unit.melee)` (or entity record +0x14 when there is no unit), ×2 for entity types 1–3. One roll `u = next16()*1.5259022e-05*total_weight` selects the attacker by binary search over cumulative weights. **RNG roll #1.**
3. The defender is chosen as `idx = min(N-1, (next16()*N)/0xFFFF)` over the whole list, **repeated until it is on the other alliance**. **Rolls #2..k.**
4. `E.attackers_on_target` = the number of list members on the attacker's side.
5. Blow resolution: **one roll.**
6. If the attacker was charging (priority ≠ 0) and the result is MISS, the defender immediately strikes back with a second resolution. **One more roll** (no re-pick).
7. Each resolved blow increments a statistics counter at unit+0xBF0.

### 12.10 Missile hit chance and projectile kill (resolves §10.3)

**Chance to hit** (0x00DAB9D0, called from 0x006A5CE0; CONFIRMED). A = shooter info, P = shot info, R = the rules adapter above.

```
marks = max(0, A.core_marksmanship(+0x10) + P.marksmanship_bonus(+0x4))
marks *= A.control(+0x28) * P.visibility(+0xC) * P.angle_judgement(+0x10)
Dh = R.missile_distance_for_half_chance_hit                                   // rules +0xE4
if A.is_land_artillery(+0x4): Dh = R.missile_distance_for_half_chance_hit_artillery   // +0xE8
elif A.is_naval(+0x8):        Dh = R.missile_distance_for_half_chance_hit_naval       // +0xEC
cth = (marks / dist^2) * (Dh*Dh*0.01)            // dist = P.f0, the distance to target
if P.target_in_cover(+0x14): cth -= 0.2
cth = clamp(cth, 0.01, 1.0)
```

In 0x006A5CE0, cth feeds the aim-dispersion ellipse (`w = sqrt(calibration_area / cth)`, see 12.6). Whether a man is struck is decided by the simulated projectile path, and the impact is then resolved below.

**Projectile impact** (0x00DAADF0; CONFIRMED):

```
dmg = P.damage (float)
if dmg <= 0:   MISS
if dmg >= 1.0: return (P.horizontal_distance > P.effective_range) ? MISS : KILL
kc = has_attacker ? round(60*dmg) + round(attacker.accuracy * 0.6666667) : round(100*dmg)
kc -= D.shield  / R.projectile_damage_shield_divisor        // integer divisions
kc -= D.armour  / R.projectile_damage_armour_divisor
kc -= D.defence / R.projectile_damage_defense_divisor
kc -= round(R.projectile_damage_distance_multiplier * P.horizontal_distance / P.effective_range)
kc = clamp(kc, 14, 94)                 // the code uses 0x5E = 94; the log string says 95
r  = min(100, (next16() * 101) / 0xFFFF)          // 0..100, RNG = the passed state (battle+0x50)
d = kc - r :  d >= 7 -> KILL (5);  1 <= d <= 6 -> KNOCKDOWN (3);  else MISS (0)
```

`missile_xholds_*` are **not** used in this function. Their users are UNKNOWN.

**Still open:**
- Exact semantics of encounter `dir` (computed by 0x006AD890 from the defender's facing table at 0x0176CFF8).
- The runtime fatigue_effects record layout that feeds the attack/charge multipliers.
- The meaning of the army level used for the +4/+8 attack bonus.

# NapoleonRust — complete backlog to a finished 1:1 game

Created 2026-10-03 at the user's request: "add everything else to the backlog that's left before the game is complete".
The goal is a **complete 1:1 remake**. Everything below must match the original game as closely as possible: its formulas, data,
visuals, UI and middleware. The one exception is the original engine's hard limits, which are removed. These include fixed
counts of factions, regions, religions and cultures, and the caps on units per army, unit size and battle size. Defaults stay at
the original's values. See the Goal section of `CLAUDE.md`.

## Progress

Updated 2026-10-08. The manager updates this table after every merge from `bash tools/backlog_count.sh` (never by hand).

Tags left in the code (`bash tools/tag_count.sh`; done means zero): 2026-10-07 start: 1,745; 2026-10-07 end: 1,758 (626 PROVISIONAL, 104 PLACEHOLDER, 1,028 INFERRED; up because reviews tagged guesses and placeholders that had been untagged), plus 581 UNKNOWN.

| Section | Done | Partly done | To do |
|---|---|---|---|
| 0. Cross-cutting fidelity | 77 | 0 | 31 |
| 1. File formats and data | 11 | 0 | 0 |
| 2. Graphics and rendering | 0 | 11 | 29 |
| 3. Battle gameplay | 1 | 11 | 19 |
| 4. Naval battles | 0 | 1 | 5 |
| 5. Campaign gameplay | 4 | 13 | 12 |
| 6. AI | 0 | 1 | 5 |
| 7. User interface | 1 | 7 | 8 |
| 8. Video | 2 | 1 | 0 |
| 9. Audio | 0 | 2 | 8 |
| 10. Multiplayer | 0 | 0 | 5 |
| 11. Modding | 0 | 1 | 6 |
| 12. Platform, release and quality | 0 | 0 | 6 |

§0 counts individual pieces; the other sections still count whole features, so their numbers move
more slowly. Split a big item into sub-checkboxes when work on it starts.

**How to use this file:**
  - `[x]` done in `main`. A note in brackets names anything still PROVISIONAL.
  - `[ ] **PARTLY DONE:**` partly done (GitHub shows it as an empty box).
  - `[ ]` not done yet.
  - `(running: <worker>)` means a worker is on it now.
- One line per item, kept short. Evidence, round history and long explanations go in the notes file
  named in brackets, not here.
- The manager ticks items off when a branch is merged, and adds new items as they are found.
- Each item's evidence and open questions live in the notes named in brackets.
- Rough order of priority inside each section: top first.

---

## 0. Cross-cutting fidelity
Each area keeps a notes file with a resolved/open table (question, answer, tag, code); read its "Where I am"
first. Evidence and round history live there; the pre-2026-10-07 long form of this section is in
`docs/archive/BACKLOG_0_long_form_2026-10-07.md`.

### 0-A battle rules (paused) — [analysis/fidelity/BATTLE_FIDELITY.md]
- Open items moved to §3 "Battle rules from §0-A" (2026-10-07, so §2 can start; the user: "most efficient").
- [x] Reload
- [x] Morale: timers, sub-evaluators, casualty-ratio ring buffers, start values
- [x] Unit attributes, strength potentials and card terms
- [x] Soldier hit points and the death dispatch
- [x] Chance to hit: control, visibility, angle judgement, woods cover, accuracy
- [x] Cartridge pool
- [x] Charges only at impact; melee exchange timing
- [x] Fatigue effects on speed, charge, control and attack
- [x] Ground speed columns and slope
- [x] Battle clock and time-out
- [x] Generals, reinforcement entry, units leaving the map
- [x] Battle Lua scripts in live battles (0 unknown calls; TUT_Land runs)
- [x] Skirmish, deployables (placement, contact kills, cover), building garrisons, special abilities, shot types
- [x] Map-preset weather
- [x] Which soldiers fire per firing drill (`volley_plan`, CONFIRMED in Ghidra)
- [x] Experience: melee has no experience term; `unit+0xD48` is the level and drives waver/rout timers and the fatigue bonus; land/naval bonus tables; battle-file `unit_experience` reaches every unit
- [x] kv_rules slot = list position

### 0-B campaign rules — [analysis/fidelity/CAMPAIGN_FIDELITY.md]
- [x] Campaign variables, taxes, GDP and town wealth (exact, 238 regions)
- [x] Trade, exact in all 9 original saves: nodes, routes, blockades, supply split, prices
- [x] Bankruptcy and the desertion gate
- [x] Land autoresolve (no retreat, CONFIRMED); naval autoresolve (inputs CONFIRMED on 71 ships)
- [x] Public order (1180/1184 classes)
- [x] Recruitment and construction queues (CONFIRMED); construction cost tech chains (328/330); construction and repair cost, affordability, charge and cancel refund traced in the exe (CONFIRMED, merged `47323d4`, user-checked 2026-10-07)
- [x] Research rate (CONFIRMED)
- [x] Effects wired in; region adjacency; sea-route cap
- [x] Capture choice (occupy / loot / liberate) and repairs
- [x] End Turn speed; turn order (CONFIRMED)
- [x] Diplomacy rules and computed factors (506/506); allies called into wars; treaty money
- [x] Religion conversion and drift (341/341)
- [x] Growth (72/72 eur, 31/31 spa) and the region recompute chain
- [x] Fortification slots, build / upgrade / repair; fort options (candidate rule INFERRED)
- [x] Militia rule (CONFIRMED)
- [x] Government drift on a government change (drift turn INFERRED)
- [x] Recruited unit size (3174/3174 save units), used for new recruits
- [x] Demolish command (refund PROVISIONAL)
- [x] Experience-adjusted recruitment cost (recruitment use INFERRED; upkeep untouched)
- [ ] Region transfer in deals (`TransferRegion` rejected: needs the `0x00B449F0` flags)
- [ ] Peace terms: regions and techs as deal items (`0x00B449F0`)
- [ ] Recruitment details (`0x00AECEE0`, `0x00AED220`)
- [ ] The importer limit (`0x00BB5730`)
- [ ] The 4 desertion-exempt unit classes
- [ ] Naval PROVISIONAL details: capture share weighting, captured crew base, gun counts of 8 ship models (needs a probe)
- [ ] Trace the affordability rules of recruitment, the unit pool and treaties (ours: their own checks in commands.rs ~1067, pool.rs ~309/484, treaties.rs ~467), and their charging arithmetic (ours: saturating/plain; overflow behaviour untraced; upkeep economy.rs ~525 and script treasury changes ntw_script game.rs:211 also bypass treasury.rs)

### 0-C middleware — [analysis/fidelity/MIDDLEWARE_VERIFY.md, BINK.md]
- [x] Miles mixing rules and loudness (CONFIRMED against the exe); game-speed sfx rule; UI sound kinds
- [x] Bink intro: order, once per start, full-screen sizing
- [x] Battle animation ACTION table contract (CONFIRMED against the dumps); cue dispatch closed exe-side
- [ ] The sound bank query
- [ ] Animation cue → slot dispatch (INFERRED)
- [ ] The movie skip rule
- [ ] The headphones multiplier

SpeedTree leftovers are in §2.

### 0-D units, animation, terrain, trees — [analysis/fidelity/UNITS_TERRAIN_FIDELITY.md]
- [x] Animation slot table, per-man clip selection, gait levels
- [x] Sim-driven fire, reload, melee, death and knockdown clips
- [x] Unit and horse LOD switching
- [x] `groupformations.bin` and the default deployment (incl. the guerrilla group)
- [x] Tree list format, heightfield normalisation, training-level order
- [x] Standard bearers draw their flag: cloth geometry, pole and bone CONFIRMED; verlet solve in `ntw_sim::battle::cloth`
- [x] `slots_art` / `slots_templates_models` load; region fort model `fFort` level n → `fort_lvl<n+1>`
- [x] Dependent factions' flag key = last segment of `factions.flag_path` (77/77; the exe call PROVISIONAL)
- [ ] Tree scale byte decode `u8 / 128`, clamp 0.5–1.4 (PROVISIONAL after review)
- [ ] The enum behind `unit->+0x1B0` (value set INFERRED; static attempt closed)
- [ ] Flag wind speed and solver substep / iteration / damping constants (PROVISIONAL)
- [ ] What string names a flag record (`+0x38`; needs a debugger watchpoint)
- [ ] Whether the bearer's cloth reads `flags.tai` (UNKNOWN); the exe's limp-flag shape

### 0-E campaign map, campaign UI, front end — [analysis/fidelity/UI_FIDELITY.md, analysis/campaign/CAMPAIGN_MAP.md §10]
- [x] UI scale for 720-high windows (CONFIRMED)
- [x] Radar map; government, technology, objectives, lists and diplomacy screens
- [x] Building browser and tree; capture screen; Load Game page; tooltips; credits; text entry
- [x] Custom battle setup: settings, armies, save/load, start
- [x] Campaign trees, Bezier splines, river ribbons, coastal surf, close-up supertexture, town/port facing
- [x] Settlement panel: upgrades, build/cancel/repair, recruit/cancel, building permissions, restricted buildings
- [x] Naval and infrastructure tabs; demolish button; experience-priced recruitment cards
- [x] Negotiation object (20 methods)
- [x] `panel_manager` (the shipped `panelmanager.luac`); `enlist_commander` panel opens, position CONFIRMED
- [x] Agent action popups end to end: Assassinate, Sabotage, Duel
- [x] Address representation matches the original (`Pointer<T>`, `CHARACTER` in `tostring`)
- [x] River scroll rates (CONFIRMED 0.02 / 0.05); border shader named; roads are textured (4 materials)
- [x] Panels can't be closed: merged `8aad50d`, checked in game by the user 2026-10-07
- [x] Side-by-side UI fixes (user screenshots 2026-10-07), merged `53361ec`: technology links from the exe's link step (no line over the title); Army | Recruitment tabs by the exe's rule (generals and admirals; colonels none, CONFIRMED by the user); general portraits in the army bar and Lists; Lists docking in the scripts' 1280x960 frame (debugger sitting, CONFIRMED at 1920x1080). Open parts are their own §0 items (generated-character portraits, army recruitment contents)
- [x] Wall building: merged `28471b5`, checked in game by the user 2026-10-07 (the walls are the last construction card; they appear on the map when finished)
- [ ] Map forts (the `fFort` chain, separate from settlement walls): `BuildFort`/`UpgradeFort` levels PROVISIONAL; nothing in the shipped exe builds a map fort (UI_FIDELITY.md)
- [ ] Agent attribute icons: merged `25a6b21` (`agent_attributes` icons, main attribute, `PrimaryLevel` CONFIRMED);
  waits for its in-game check (HANDOFF). Left: rank PROVISIONAL for non-agents, UI skin folders not modelled
- [ ] Region labels under the HUD; region details title "XXX Details"
- [ ] Diplomacy negotiation playable; region exchange rows
- [ ] Campaign save naming
- [ ] The five agent options actions via `MoveIntoTarget` (contract known; needs the model's agent order queue)
- [ ] `AgentRogueSabotageArmy` (no army target list)
- [ ] Fort as its own selection

### 0-F effects system — [analysis/fidelity/EFFECTS_FIDELITY.md]
- [x] Techs, buildings, traits (CONFIRMED level rule), ancillaries, government, ministers, difficulty; query API wired into the economy

### 0-G characters and agents — [analysis/fidelity/CHARACTERS_FIDELITY.md §11-§12, CHARACTER_UI_HOOKS.md]
- [x] Trait and ancillary gain; natural death
- [x] Commander succession (CONFIRMED), royal family, vacated posts, female leaders
- [x] Minister dismissal, appointment and the spare pool
- [x] Agent actions: rolls and chances (CONFIRMED), assassination, duels, sabotage, tech stealing, forced-success flags, script events
- [x] Diplomatic reactions
- [x] Sight and shroud (CONFIRMED), hidden characters, spying, spy networks, the stealth test (118/118)
- [x] Recruitment pools, historical characters, HireGeneral (cost CONFIRMED), HireAdmiral, PromoteUnit / CharacterPromoted
- [x] Turn-end counters; CharacterCreated / CharacterPromoted events
- [x] Character UI hooks wired: commander hire, promotion, target pickers, fog layer
- [x] Promotion price key: agent type + the human faction's subculture (CONFIRMED); naval promotion free
- [x] No general limit in the vanilla data (CONFIRMED negative)
- [x] Fog: one `fog_state_at` definition; explored settlements keep their labels
- [ ] Promotion price number: one debugger sitting by the user (`analysis/fidelity/debugger/0g_promotion_probe.cdb.txt`)
- [ ] Field promotion menu (`CCQ_PROMOTE_COMMANDER`): no shipped layout, needs our own context menu

### Cross-cutting
- [x] Determinism audit (no hash-map order, clocks, threads or OS randomness in the model crates); twice-run harness identical
- [x] Comparison harness built: `image_diff`, `NAPOLEON_BATTLE_TRACE`, `docs/COMPARE_WITH_ORIGINAL.md`
- [ ] First real side-by-side run against the original (needs the user)
- [ ] Battle `B.WindowsTime` returns os.clock() (process CPU time, fractional); the exe gives whole wall-clock seconds (0x009FB0B0) (ntw_script/src/ui/battle_prelude.lua:143)
- [ ] Portraits of generated characters: the recruitment pool's hires and promoted generals (`promote_unit`) get empty CHARACTER_DETAILS (ntw_sim pool.rs), so their army card and Lists row keep the unit card (PLACEHOLDER, ntw_script ui/campaign.rs `unit_entry` / `character_details`); trace how the exe picks a new character's portrait and give it in the model.
- [ ] Army recruitment tab contents: `0x009FE7B0` reads the tab's own manager (commander `+0x34` → `+0x124`), not traced; ours opens it empty (PLACEHOLDER, ntw_script ui/campaign.rs `generate_current_tab`). Evidence: the original's army Recruitment tab in an own region has an Options row (4 cards, a "2/1" badge, cost 472) and a 10-slot Queue (user screenshot `ntw-evidence\screens\2026-10-07_original_wellesley_army_recruitment_tab.png`); the naval tab's port path is untraced too (PROVISIONAL).
- [ ] Battle HUD scripts' frame: the campaign HUD's scripts work in the root's 1280x960 layout frame (debugger sitting 2026-10-07); the battle HUD is untraced, so ours keeps screen geometry there (ntw_script ui/host.rs `set_script_frame`).

AI (§6) is paused by the user's section order; its open questions stay in `analysis/ai/AI_RESEARCH.md` §7.

## 1. File formats and data
**Status: COMPLETE** (2026-10-04 ~10:55: all 11 items done. Save compatibility with the original game was REMOVED from scope by the user after NR-14/NR-15 crashed the original; our game saves and loads its own saves and reads every vanilla save. History: SAVE_COMPAT.md). Markers are strict here: `[x]` = nothing left, `[ ] **PARTLY DONE:**` = partly done or still being
worked on, `[ ]` = not started.
- [x] packs/Vfs, DB, loc, ESF, rigid_model, DDS, TGA, UI layouts, fonts, unit variants, anim, weighted meshes, battle terrain,
      vegetation lists, campaign map, preferences, sound banks
- [x] Bink `.bik` video: container, video (73/73 movies) and audio (229 tracks) decoders plus the player, merged d5251e6 [BINK.md].
      (The player's skip and loop rules and turning `--intro` / `--frontend-movie` on by default need a comparison with the original;
      that's §0/§8.)
- [x] SpeedTree `.spt`: all 229 files parse to the end; generator recreated (merged 7ffc1d7).
- [x] **`pathfinding.esf` and `sea_grids.esf`** (closed 2026-10-04, round 7 of pathfinding-ports; residual unknowns below). Done: readers (incl. nap_spain's value cipher), the movement grid
      from the original polygons, polygon kind = flags & 0xF (CONFIRMED), the original's cell flood search and its mover/kind table
      found; **the original's path search found and ported** (`ntw_sim::campaign::polypath`: A* over (cell, polygon) nodes, CONFIRMED
      costs: the cell header bytes are per-direction cost bytes, roads cost `road_level_N_action_point_cost` per region, border strips the
      cheaper region; fleets use the same search on sea kinds; the `grid_data` u32 = polygon count; same-component pre-check); reachable
      range = cost within the remaining AP; rivers = kind 3 strips crossed only at roads/kind 7; flag bits 24..31 = direction mask.
      Path smoothing found (corridor, triangles, taut line) and ported (`polysmooth`); zones of control decoded and ported (`zoc`: enemy
      armies block a 6-unit walking zone, navies 12, others their core). Ports / landing / embarking ported
      (`ntw_sim::campaign::embark`: port and landing nodes, landing ends the turn, Embark / Disembark commands; a fleet lands an army
      on Corsica in the install test) [PATHFINDING_PORTS.md].
      Round 2: polygon point, landing valid-position check and port relation (own / at war) CONFIRMED; harbour exit through the
      port footprint INFERRED (data fits all 67 ports). Zone-of-control core = 1-unit circle (CONFIRMED).
      Round 3 (pathfinding-ports): run-time cutter decoded to the polygon clipper (shape kinds per mode, kind link table CONFIRMED;
      not ported: zones still block whole polygons); mover families CONFIRMED (family A unless inside a building) and the kind-7
      rule applied to land movers (`movers`, all settlements reachable); the saved transport link (NAVY #4 / ARMY #7) CONFIRMED
      and loaded.
      Round 4 (pathfinding-ports, `zoc` now owned there): zone limits fixed (army 6, fleet 12; saved boxes match, swapped 0);
      the run-time cutter ported (`rtcut`: overlap kinds decoded in `0x00B13520`, exact core clipping, kind link table; zones
      re-kind their polygons; `polypath::Overlay`/`View`; `plan_path` searches the cut map); the second kind 7 rule
      (`0x00B167B0`) decoded, kind 7 rules applied to every mover incl. fleets; garrison core = 0.25 triangle; obstacle record
      API for the save writer (`zoc::obstacle_record`, box and cell ranges CONFIRMED on every start-position obstacle).
      Round 5: the original's obstacle storage decoded on vanilla data (pieces, cell versions and their flags rule, grid
      nodes, pair lists, slots = modes) and `ntw_campaign::grid_obstacle::add_character_obstacle` writes a complete obstacle
      for a new character (passes `save_check`; same cells and rings as the original for 68 of 95 start-position obstacles).
      Round 7 (closing): search mode 2 / 3 CONFIRMED (the order's target obstacle re-kinded to 10 / 11: walk in and end there,
      never cross) and ported; family A never enters its target footprint by search (CONFIRMED; the order code snaps it in,
      ours walks in, tested); the zone flood's cost pairs and node points decoded in structure.
      **Residual unknowns (none blocks load, save or play; tested safe behaviour in place):** how the target is named to the
      query (3-unit stand-in); the snap step's AP; 26 of 69 army zone boxes one cell off (flood entry points `0x00B011B0`);
      the in-port fleet's search type (harbour exit INFERRED, fits all 67 ports); the embarked commander's saved position
      (no vanilla example; ours keeps him with the fleet); the pieces' exact shapes; modes 2..4 in written obstacles
      (wiring new obstacles into the save writer is save-compat's)
      [PATHFINDING.md, PATHFINDING_PORTS.md, CAMPAIGN_DATA.md §1, §11].
- [x] Startpos/save fields the loader skipped: **done:** names, portraits, traits, ancillaries, government posts (leader, ministers),
      governorship taxes, capitals, full diplomacy records, the scripts' `save_value` slots (loaded and written) [CAMPAIGN_DATA.md §3-4].
      Mission records: layout CONFIRMED from the exe, reader + writer `ntw_campaign::missions` (s1-missions-ui) [S1_MISSIONS_UI.md].
      Diplomacy: all 29 `DIPLOMACY_RELATIONSHIP` fields named from the exe (24 attitude factors, 14 force_diplomacy slots CONFIRMED)
      [S1_LEFTOVERS.md §1]; #16/#21/#22 CONFIRMED unused (only loaded, saved and copied). Mission targets are object ids re-linked
      through the id map (CONFIRMED) and missions load into the model (542c263). `other_income_mod`: INFERRED never saved (§11).
- [x] `.markers`, `.farm_fields_tile_texture` and the farm files (battle map extras): readers and install tests; the grass-texture chunks
      are TGA files (all 720 decode). Farm records typed (`FarmManager`, `FARM_COLLISION` radii CONFIRMED 387/387) [S1_LEFTOVERS.md §2].
      Collision bool, the run-time centre rule and the two farm lists (inside / reaching outside the playable area) CONFIRMED; the
      stored centres are INFERRED leftovers. Piece pairs = the walls on the farm's edge, {wall index, side = the wall's slot for the
      farm} (CONFIRMED: data 86/86, and the exe's copy for a farm in both lists flips the side); the owner's second u32 = the tile
      template index (CONFIRMED; 0 because no manager has 2 templates). Wall instances settled: #1 = the boundary kind the generator
      draws from the farm's kind list (CONFIRMED), the {u32, i32, u32} list = the bordering farms (CONFIRMED), the last list
      CONFIRMED unused (empty everywhere) [S1_LEFTOVERS.md §2 updates].
- [x] The 23 non-mount `.variant_weighted_mesh` files: all 286 parse (equipment attachments, headerless agent and testdata layouts) [CAMPAIGN_DATA.md §7]
- [x] UI templates and layout fields: `uied.templates` all 126 entries read (v<6 legacy fix; the header u32 is the entry offset).
      UI layout `unknown_da` = ClipChildren (CONFIRMED), `unknown_e5` = UseGlobalClicks (behaviour CONFIRMED in every use: takes
      clicks anywhere without consuming them; the name comes from the editor's field list, as for ClipChildren), `unknown_140` = an inherited
      DrawMode (inheritance CONFIRMED; it picks one of three sprite-batch / text render variants, CONFIRMED). State +0x60/+0x64 =
      TextXOffset/TextYOffset (CONFIRMED, applied in the front end with the exe's text layout rules); the +0xF0 pair is editor-only
      (s1-missions-ui). DrawMode 0/1/2 CONFIRMED (0 = scaled with the UI, 1 = 1:1 window pixels, 2 = full screen). State +0xD0/+0xD4
      CONFIRMED unused at run time (loaded only; no reader in the UI code) [UI_LAYOUT_FORMAT.md].
- [x] The 31 older-layout `testdata` anim clips (28-byte keys; the 4 oldest without bone names): all 3,814 `.anim` parse [ANIM_FORMAT.md]
- [x] The 2 exotic DDS formats (A16B16G16R16; fourCC 63 = Q8W8V8U8 used as plain RGBA): all 8,239 DDS decode
- [x] All installed languages for loc text, plus the language setting: `--language xx` or our own `language.txt` in the NapoleonRust
      user folder, else the install's `language.txt`; any installed `local_XX` can be opened (only `local_en` ships here) [CAMPAIGN_DATA.md §10]

## 2. Graphics and rendering
**Shaders and materials**
- [ ] The original shaders recreated: `Textured_Rigid.fx`, the unit/skin shader, the terrain shader (not in `fx\`; it's in the exe),
      water, and the sky. This includes normal, gloss and specular maps, and the faction colour-mask combine.
- [ ] Per-faction texture atlases for units, if the exe uses them instead of the loose textures [ANIM_FORMAT.md]
- [ ] LOD switching for buildings, units and horses, using the original distances.
- [ ] Shadows, lighting, HDR and tone mapping, bloom and depth of field as the original does them (graphics options included).

**Trees and vegetation**
- [ ] **PARTLY DONE:** **SpeedTree 1:1.** Near trees, shrubs, LOD fade and wind are merged (7ffc1d7); still to do: branch/frond/leaf LODs, normal-mapped lighting, leaf rocking, weather wind, per-tree rotation. Still open: the `.spt` decoder, the branch/frond/leaf generator with
      SpeedTree's own RNG, LOD fade to billboards, wind and lighting.
- [ ] **PARTLY DONE:** Shrubs, drawn with SpeedTree (7ffc1d7). Open: the same SpeedTree leftovers as above (LODs, lighting, leaf rocking).
- [ ] **PARTLY DONE:** Billboard trees (the stand-in; the atlas UV bug is fixed).
- [ ] Grass (`fx\grass.fx`, `grassmap.tga`, `.tai` atlases).

**Battle terrain and battlefield**
- [ ] **PARTLY DONE:** Terrain heights, colour maps, buildings, deployment zones.
- [ ] Detail, blend, cliff and rock maps, and the lightmap [BATTLE_TERRAIN.md]
- [ ] **PARTLY DONE:** Water: rivers, lakes and sea, with reflections and shorelines. Done (sandbox port 06d6a7e): battle-map sea surface (ocean.fx port, sea level 0 m CONFIRMED on 60 maps), camera above the sea; foam off by default (PROVISIONAL). Not checked in game. Open: rivers, lakes, shorelines, reflections, campaign water [WATER.md].
- [ ] Sky dome and clouds, time of day, and the `.environment` lighting in full.
- [ ] Weather: rain, snow, fog, wind (visual and gameplay effects).
- [ ] Fences, walls, bridges, fords and other placed battlefield objects.
- [ ] Building damage and destruction (the `_destruct` models), and fire.
- [ ] Generated battlefields from campaign positions (terrain tiles), for campaign battles that aren't historical maps.

**Battle effects**
- [ ] **PARTLY DONE: Musket smoke, cannon smoke, muzzle flashes, dust.** Ported from the old sandbox
      branch (s2-effects) onto `next` and finished: the effects database reader (`ntw_formats::effects`
      for `effects\landbattle.xml` and `effects\unit_dust_parameters.txt`, 283 emitters / 152 groups
      CONFIRMED on the install), the deterministic particle world (`battle::fx`, `CaRng` off the
      battle seed), the per-unit dust timer at the shipped entity frequency, the draw layer
      (`battle::fx_draw`, `particle.wgsl`) with the original's own effect textures. Three draw-layer
      bugs fixed on port: the 2D HUD camera was used for the billboards, the buckets were matched by
      query position (a `Query` has no order), and every frame added a new mesh to `Assets<Mesh>` (an
      unbounded leak). Round 2: **which group a muzzle plays is now the shipped data** —
      `projectiles` column 31 is the fire effect group itself (120 of 144 rows set it, every value a
      `landbattle.xml` group), so `fx::fire_group` reads it and the 9-pound guess is only a fallback;
      `FIRE_GROUPS` is now exactly the eleven groups the data names. Round 3: **sprite facing now
      comes from the file too** — the reader names all four shipped `sprite_facing_mode` values
      (CONFIRMED over all 435 emitters: `CAMERA_FACING` 401, `BILLBOARD` 18, `LOCAL_Y_AXIS` 14,
      `WORLD_Y_AXIS` 2) and the draw layer honours the two Y-axis modes, so a shell's earth and
      debris stand upright instead of tipping at the camera; `VELOCITY_FACING` is a clean negative
      and `align_to_velocity` is false everywhere. Still PROVISIONAL: the muzzle height and
      position, the dust group choice, the light one uniform, `RENDER_METHOD_DISTORTION` drawn flat.
      Not checked in game [BATTLE_EFFECTS.md].
- [ ] Projectiles: musket balls, round shot, shells, canister, rockets, with visible flight. Not
      started: our model resolves a volley at once, so there is no flight to draw. **Finding for 0-A:**
      whether the original inserts a delay between fire and impact is a battle-rule question, so it is
      Claude's to answer, not ours — inventing flight time here would be a fiction, not a port.
- [ ] **PARTLY DONE: Explosions, impacts, craters and decals.** Blood on a man, the air burst and the
      ground scorch play the shipped group names (`blood_gen`, `AirExplosion_sml/med/lrg`,
      `Cannon_Groundimpact_explosive`). Round 2 closed the size rule from data: the air burst and the
      scorch are the shot's own row in `db\projectiles_explosions`, read by the new
      `ntw_formats::projectile_fx` with no schema at all (the 0/0/17 byte-gap run is unique to a row
      and occurs 35 times, the header's count). The size is **authored per row, not computed**:
      `shell_12lb` -> `AirExplosion_med` but `shrapnel_12lb` -> `AirExplosion_sml`, so the 6/12-pound
      rule is only a fallback. Two negatives: the row's third string column is a fragment
      **projectile** (`projectiles` key), not a group; and **craters/decals are not driven by the
      effect system** — no shipped effect file names `decal.fx` and there is no `RENDER_METHOD_DECAL`,
      though the decal textures ship. Still open: `projectile_impacts`' surface column order (so the
      blood group stays INFERRED) and the numeric columns' names.
- [ ] **PARTLY DONE: projectile trails.** Round 3 **closed the one open naming thread from rounds 1-2**:
      a shot's trail is a *pair*, and the two halves live in two different columns.
      `projectiles` column 32 (`trail`) is the trail's **effect group** in `landbattle.xml` — all 7
      distinct values over 9 rows are groups — and **column 6** (`trail_texture`), already decoded and
      already named in `analysis/worker2/schemas_battle.md`, is the foreign key into
      `projectile_trails`: all 5 distinct values over all 144 rows are that table's 5 keys. Round 2's
      negative was real but was the wrong question; column 32 was never the foreign key. The table
      now reads whole with no schema guess (`ntw_formats::projectile_fx::TrailTable`, 5 rows,
      `ssffffffffff` = key + blend mode + 10 floats). Named from the data: the second string is the
      **blend mode**, and floats **4-7 are an 8-bit RGBA quadruple** (white at alpha 128 on three
      rows, grey at alpha 100 on the rocket, all zero on `none`). **Still open, on a negative:** the
      other six floats — one table row serves projectiles spanning 4..250 m/s of muzzle velocity and
      50..750 m of range, so no float in it can be a per-shot duration or length. **Not drawn**:
      trails need visible flight, which is a 0-A battle-rule question (see above).
      [BATTLE_EFFECTS.md §7]
- [ ] Unit flags and standards: the cloth mesh and simulation. The flag cloth is missing entirely today [CAVALRY.md]
- [ ] Selection rings, movement and order markers, formation drag, and destination ghosts as the original draws them.

**Units and animation**
- [ ] **PARTLY DONE:** Soldiers, cavalry, command figures, DB clip choice, GPU skinning. Open: the flag cloth, the per-man pick RNG, the original unit shader (paused on `work/shaders`) [ANIM_FORMAT.md §7, CAVALRY.md §7].
- [ ] Combat, charge, death, reload, firing, turning, idle-variation, transition and dismount clips, all played.
- [ ] Per-man clip alternatives (more variety than 3 kits per unit).
- [ ] Artillery pieces, limbers, horse teams and crews, animated.
- [ ] Ships (see §4).
- [ ] Generals on the battlefield, and Napoleon's own model.
- [ ] Equipment on/off per clip (sword drawn, musket shouldered) [ANIM_FORMAT.md]

**Campaign map graphics**
- [ ] **PARTLY DONE:** Terrain, borders, rivers, roads, settlements, army markers.
- [ ] Forts, ports, resources (mines, farms, logging), minor settlements and their growth levels.
- [ ] Trees on the campaign map, the coastline mesh, and textured border, river and road ribbons.
- [ ] Finer supertexture levels near the camera.
- [ ] Army, navy and agent figures as the original shows them (not markers), and ships at sea.
- Moved from §0-D/§0-E (2026-10-07; they need the shaders):
- [ ] Settlement walls mesh: drawn and swapped on build/demolish; level 1 seen appearing on completion in game (user, 2026-10-07). File level = chain level + 1 and additive drawing are still INFERRED (not compared with the original; looks off until the §2 shaders)
- [ ] Animated rivers (one UV offset in our own material) and campaign sea (shader UNKNOWN; lead: the campaign scene loader)
- [ ] Textured borders (width and V mapping are the exe's)
- [ ] Far-view tree models, resource-slot models, forts on the map
- [ ] Fog of war, seasons (winter snow), and campaign weather.
- [ ] **PARTLY DONE:** Region and settlement labels, selection effects, movement arrows and zone-of-control display. Done (sandbox port 06d6a7e, not checked in game): region labels (names/positions CONFIRMED, rendering PROVISIONAL), movement arrows with the original's assets (layout INFERRED, spacing/colours PROVISIONAL). Open: settlement labels, selection effects, zone of control.

## 3. Battle gameplay (land)
**Battle rules from §0-A** (moved 2026-10-07; notes in analysis/fidelity/BATTLE_FIDELITY.md)
- [ ] `volley_plan` runtime confirmation in the battle probe
- [ ] Campaign-battle weather pick: ported, connect once campaign battles start from the map
- [ ] `unit_scale`: decoder done (exe default 0.75); where it is applied is UNKNOWN, not wired
- [ ] Who writes `unit+0xD48` (debugger write watchpoint)
- [ ] Formation radius `+0x670` (probe written)
- [ ] Garrison cap `+0x6C`
- [ ] The `--battle --screenshot` "closed channel" capture
- [ ] Battle restart (R) keeps the same unit ids, so views survive with old corpses and `acts.dead` (napoleon/src/battle/view.rs ~426).
- [ ] Units are drawn in 0.1 s position steps (0.14 m walking, ~0.8 m cavalry): no interpolation between model ticks. Trace in Ghidra whether and how the exe interpolates; visible judder, older than the jitter fix (2026-10-07).
- [ ] Player run orders: our battle input never issues `MoveSpeed::Run` (only scripts do), so units can't be made to run (user, 2026-10-07). Build the original's controls from its layout/scripts and exe input rule (double right-click, the HUD run toggle; trace in Ghidra).

**Formulas, all from Ghidra or the DB**
- [ ] **PARTLY DONE:** Melee, morale, fatigue, missile fire. Done (0-A): who fires per drill (the "half the men" placeholder is gone). Open: target choice, holding fire in melee, other PROVISIONAL values [`ntw_sim` shooting].
- [ ] **PARTLY DONE:** Accuracy and hit chance, reload formula, ammunition, experience and chevrons, weather effects on muskets. Done (0-A): hit chance factors, reload (with the firing drill), the cartridge pool, map-preset weather, which soldiers fire (S3), the campaign-battle weather pick. Experience in battle ported (morale timers, fatigue; melee has none, CONFIRMED). Open: chevrons display, who raises experience, connecting campaign-battle weather once campaign battles start from the map.
- [ ] **PARTLY DONE:** Charge impact, mass and collision, push-through, and cavalry vs square. Done: charges count only at impact (CONFIRMED), charge bonus, defences stop charges. Open: mass/collision and push-through, cavalry vs square checks.
- [ ] **PARTLY DONE:** Routing, rallying, shattered units, and the general's aura and abilities. Done: morale states, rally test, routers leaving the map, general/army morale terms. Open: general abilities (RALLY/INSPIRE), general death/flight timers.
- [ ] Generals as units on the battlefield, with RALLY and INSPIRE (general abilities 0x11/0x12). The AI's GENERAL_SUPPORT tactic is
      decoded and waiting for this [AI_RESEARCH.md].
- [ ] **PARTLY DONE:** Unit abilities in the sim: square, skirmish mode, ability 0x10 (used by DEFEND_ABSTRACT), and group formations (GroupFormations). Done: square flag, skirmish behaviour (CONFIRMED), groupformations.bin default deployment. Open: ability 0x10, AI use of group formations (§6).
      [AI_RESEARCH.md].
- [ ] Unit pathfinding on the battlefield (around buildings, trees, rivers and slopes).

**Formations and abilities**
- [ ] Line, column, square, skirmish, fire by rank, wedge and the rest, exactly as unit abilities define them.
- [ ] **PARTLY DONE:** Special abilities: rally, inspire, rockets, and any others in the DB. Done: perform_special_ability (square, stakes, unlimber recorded), tech-gated abilities. Open: rally, inspire, rockets.
- [ ] **PARTLY DONE:** Deployables, if the DB defines any for NTW. Done: placement geometry (CONFIRMED), drawn with the original models, contact kills (CONFIRMED triggers), cover. Open: defences blocking movement (PROVISIONAL).

**Artillery**
- [ ] Cannon types, shot types (round shot, canister, shell), arcs, ricochet, limbering and unlimbering, horse artillery.

**Buildings and sieges**
- [ ] **PARTLY DONE:** Garrisoning buildings, firing from windows, building cover. Done: the defendable rule, walls, slots per fire line (CONFIRMED rule). Open: the garrison cap +0x6C, entry/exit rules (PROVISIONAL).
- [ ] Forts and walls: siege battles, gates and breaches.

**Battle flow**
- [ ] **PARTLY DONE:** Historical battle armies, deployment phase, victory and defeat, results screen (v1, merged 20df5dc). Open: reinforcements, allied AI, formation drag, victory grading, results-screen gaps [BATTLE_FLOW.md].
- [ ] **PARTLY DONE:** Reinforcements arriving from adjacent armies (campaign battles). Done: entry groups, joining when inside the playable area (CONFIRMED), scripted reinforcements. Open: campaign battles don't launch yet (§5).
- [ ] Time limits, victory points and capture points, where the original has them.
- [ ] Attacker/defender role per side (from the battle file or the campaign), map edges for withdrawing and routing, settlement-capture
      victory conditions, unit visibility, and the battle-difficulty attack bonus on AI armies. The battle AI already reads these
      (defaults to "attacker" until the model has them) [AI_RESEARCH.md §8].
- [ ] Battle speed, pause and the camera modes (free, general, unit) as in the original.
- [x] Historical battle scripts (Lua): every call in the shipped land scripts is implemented (0 unknown stubs, 0 ignored orders); Waterloo, Austerlitz, Dresden, Lodi and TUT_Land run with their scripts (0-A).
- [ ] **PARTLY DONE:** Battle tutorials. Done: TUT_Land's script runs (markers, selection/command/input handlers). Open: unit voices and UI highlights it asks for.
- [ ] Battle replays (save and watch).

## 4. Naval battles
- [ ] Ship models, sails, rigging, and damage models (hull, sails, crew).
- [ ] Ammunition types (round, chain, grape), broadsides and reloading.
- [ ] Wind direction and speed, ship handling, and formations.
- [ ] Boarding, capture, surrender, sinking and fire/explosions.
- [ ] Naval battle UI, deployment and results.
- [ ] **PARTLY DONE:** Naval autoresolve and the campaign fleet hook. Done (0-B round 10): ported and running in the campaign (engagement, kill rates, damage, sinking, capture; SHIP_DAMAGE_INFO CONFIRMED). Open: ship potential, morale and range level from the runtime ship record (PROVISIONAL), ship captures, saving ship damage states (save writer queue).

## 5. Campaign gameplay
**Turn and economy**
- [ ] **PARTLY DONE:** Turn loop, Lua events, income and upkeep, taxes, public order, construction, recruitment (PROVISIONAL formulas)
      [CAMPAIGN_PLAY.md]
- [x] The exact economy formulas: town wealth, GDP, tax efficiency, trade income (0-B: exact against the start positions and all original saves).
- [ ] **PARTLY DONE:** Trade: land trade routes, sea trade theatres, trade ships, blockades. Done: routes, nodes and trade fleets, blockades, prices, supply split, sea cap. Open: the importer-side limit (INFERRED rule in use).
- [ ] **PARTLY DONE:** Research: technologies, gentlemen in schools, and their effects. Done: states, rate (CONFIRMED), schools, completion, gates, tech stealing. Open: the building-chain match (INFERRED), the research UI command wiring.
- [ ] **PARTLY DONE:** Government types, ministers, revolutions, rebellions, emergent factions. Done: royal family and succession, vacated posts, ministers' dismissal/appointment, government classes in public order. Open: revolutions, rebellions, emergent factions, elections.
- [ ] Population, settlement growth, minor settlements, resources.
- [ ] Attrition (winter, desert), seasons, and turns per year per campaign.
- [ ] BUG: Turn steps may drop CharacterMoved / ForceDestroyed events before they reach scripts (ntw_script host.rs fire_all); verify.

**Military**
- [ ] **PARTLY DONE:** Army movement, pathing, intents (move, attack, merge, enter).
- [ ] **PARTLY DONE:** Zones of control, forced march, ambush, river crossings, retreat after battle. Done: zones of control with fog of war (CONFIRMED), river crossings, no retreat after battle (CONFIRMED). Open: forced march, ambush.
- [ ] **PARTLY DONE:** Generals: traits, ancillaries and retinue, command stars, death and wounds. This includes Napoleon's special rules. Done (0-G): traits and ancillaries gained by script triggers, natural death, succession of command, the wounded-and-returns Generals (+0x52C). Open: command stars, retinue limits, battle wounds.
      (e.g. returning when wounded).
- [ ] **PARTLY DONE:** Recruitment pools, replenishment, unit upgrades and experience. Done: the recruitment queue (CONFIRMED), character recruitment pools loaded and saved. Character pools run: refill timers and bonuses (CONFIRMED), HireGeneral (0-G). Open: replenishment, unit upgrades, experience, the pool step's phase (PROVISIONAL).
- [ ] Sieges on the campaign map: turns until surrender, and assaults.
- [x] Occupy, sack and liberate options after capture: the model (preview, damage roll, apply, liberation; CONFIRMED) and the capture screen (0-E).
- [ ] **PARTLY DONE:** Navies: fleets, naval movement, embarking and disembarking armies (done: Embark / Disembark with the
      original's landing rules, PATHFINDING_PORTS.md; open: fleet capacity, AI transport, naval invasions, naval battles).
- [ ] The campaign → real-time battle → campaign loop (the hook exists: `pending_battle` / `apply_battle_result`).
- [ ] **PARTLY DONE:** Autoresolve. Done (0-B): land autoresolve and the no-retreat rule CONFIRMED; naval autoresolve ported. Open: naval ship potential and morale inputs (PROVISIONAL, see §4).

**Diplomacy and agents**
- [ ] **PARTLY DONE:** Diplomacy: treaties, trade agreements, alliances, protectorates, gifts, region exchange, attitudes and the AI's responses. Done (0-B): attitude factors and drift (1,469 steps match vanilla pairs), war/peace/alliance/trade/embargo/access/gifts/payments/protectorate rules, computed factors (religion and government 506/506; leader 484/506), allies called into wars, treaty money each round. Open: the faction-leader factor (France's leader post), region exchange (TransferRegion rejected in review: needs 0x00B449F0 flags), the negotiation UI (0-E; object ported 709dad8, not checked in game), the AI's responses (§6).
- [x] An effects layer, so difficulty handicaps and building/tech effects really apply (GDP, recruitment and upkeep costs, research,
      attrition, policing). Today only the AI's own budget uses the handicaps [AI_RESEARCH.md].
- [x] Region adjacency in the model, from the map's region outlines (0-B). (The AI still uses the 4 nearest settlements: §6.)
- [ ] **PARTLY DONE:** Agents: spies, gentlemen and the others, with their actions (sabotage, assassinate, duel, research) and success formulas. Done: chances and rolls (CONFIRMED), assassinate, duel, spy, sabotage, tech stealing, reactions. Open: conversion, stealth/spotting, the UI and AI hooks.

**Missions, events and victory**
- [ ] Missions, events and rewards (Lua and DB), and the event messages.
- [ ] The advisor, campaign and battle, with speech.
- [ ] Victory conditions for each campaign: Italy, Egypt, Europe, Waterloo, Coalition, and the DLC campaigns if installed.
- [ ] The prologue and tutorial campaign flow, and its unlock progression.
- [ ] Campaign cutscenes and videos.

**Saves**
- [ ] **PARTLY DONE:** Save and load: our saves round-trip and keep script state, economy, characters, research, families, sight and names. Open: compatibility with the original game after AI turns (§1, running).
- [ ] Restricted units (`add_restricted_unit_record`, saved in `EPISODIC_RESTRICTIONS/UNIT_RESTRICTIONS`) live only in the script state (loader → napoleon/src/campaign/scene.rs → `ScriptState::restricted_units`); the AI reads them, the model's recruit command and the recruitment panel don't. Trace where the exe checks them (as `0x009CDA90` for building levels), then give them one home, a `World::restricted_units` that model, UI and AI read (ntw_campaign/src/lib.rs ~210).
- [ ] A save's restricted building levels reach the model before the campaign scripts load (they are in the loaded `World`); the exe's order of restrictions vs. script load is not traced (napoleon/src/campaign/scene.rs ~430).

## 6. AI
- [ ] **PARTLY DONE:** Battle AI v1 and campaign AI v1, merged. The campaign AI plays at End Turn; the battle AI has the original attack FSM thresholds and think rhythm.
- (running: ai2) The permanent AI slot works through the items below.
- [ ] Battle AI in full:
  - artillery use, squares against cavalry, using terrain and buildings;
  - flanking plans, reserves, retreat;
  - sieges and naval battles.
- [ ] Campaign AI in full:
  - diplomacy offers, trade, research (RESEARCH_TECHNOLOGY ported f5e302a: structure CONFIRMED, tech pick and school PROVISIONAL);
  - agents, naval invasions, sieges;
  - taxation.
- [ ] The original's tactic state transitions, think rates and priority constants (Ghidra) [AI_RESEARCH.md §7]
- [ ] Difficulty levels (campaign and battle) and AI handicaps, as the original applies them.
- [ ] Scripted AI controls (`release_control`, scripted battle units, frozen armies).

## 7. User interface
**Front end**
- [ ] **PARTLY DONE:** Main menu, single player, Load Game, Napoleon's Campaigns and Battles, Coalition, Options.
- [ ] **PARTLY DONE:** Custom battle setup (armies, map, settings), army selection and unit purchase. Done: settings page, armies page
      (presets, recruit cards, experience, validation), setup files (save / load, `.sp_default`), starting the battle in our
      engine, text entry (Save army works from the GUI). Open (0-E): the unit size option in battle, the category
      mask, naval custom battles (UI_FIDELITY.md "Where I am").
- [ ] Multiplayer pages (see §10).
- [ ] **PARTLY DONE:** Credits (`BuildCredits`), tutorials, loading screens with art and tips. Done: credits (21 pages from credits.xml). Open: tutorials, loading screens.
- [ ] Graphics options that really change our renderer (resolution, windowed, quality presets, the anti-aliasing and shadow settings
      the original offers).
- [ ] Key bindings page and custom key sets.
- [ ] **PARTLY DONE:** Tooltips, list scrolling and clipping, cursor states, all keyboard shortcuts. Done: tooltips (shared hover path, drawn last), clipping, text entry. Open: list scrolling, cursor states, shortcuts, the hover delay.

**Campaign UI** (v1 merged 6eaf4b6: HUD fixes, army review, construction and recruitment, labels, tooltips, lists)
- [x] The "ygT" text, the 2 HUD script errors and the radar map script error (`template.map_image.lua:197`) are fixed (0-E: RegionsInTheatre and the theatre calls from the exe).
- [ ] **PARTLY DONE:** Unit cards, settlement panel and recruitment panels exist; building browser details are done. Open: the browser tree, and a playability pass of building/recruiting from the settlement panel (user, 2026-10-04: clicking a settlement doesn't do much).
- [ ] Character details (traits, ancillaries), agent actions.
- [ ] **PARTLY DONE:** Diplomacy, trade, research, finance and faction summary panels, objectives, event messages, the pre-battle panel. Done: government (taxes, trade tabs), technology, objectives, diplomacy faction list and details. Open: negotiation, finance and faction summary, event messages, the pre-battle panel.
- [ ] **PARTLY DONE:** Map labels, tooltips, the radar/minimap. Done: radar with owner colours, view outline and click-to-move; tooltips. Open: label details.
- [ ] The in-game encyclopedia/help pages.

**Battle UI** (v1 merged: unit cards, orders bar, speed controls, deployment, results; the items below remain)
- [ ] Unit cards, order buttons, formations and abilities, speed controls, the radar/minimap.
- [ ] Deployment UI, the results screen, kill counts and the heroes list.
- [ ] Unit info panels and tooltips, and the battle advisor.

## 8. Video (Bink)
- [x] A Bink 1 (`.bik`) decoder recreated as our own pure-Rust code from the format spec and Ghidra (73/73 movies, 229 audio tracks; see §1) (the game ships `binkw32.dll`; we
      don't use it).
- [ ] **PARTLY DONE:** Intro videos, the front-end background movie (`Frontend2.bik`), campaign and battle cutscenes, loading movies. Done: intro order (CONFIRMED), front-end movie, full-screen sizing. Open: campaign/battle cutscenes, loading movies, the skip rule.
- [x] Movie audio routed through our audio system with the original movie volumes (merged d5251e6, BINK.md) [AUDIO_FORMAT.md]

## 9. Audio
- [ ] **PARTLY DONE:** Sound banks, decoding, front-end music, UI clicks and hover sounds, 3D volleys, the music state machine, the Miles mixing (0-C), battle music looping without restarting (merged `d6c843b`, user-checked 2026-10-07). Open: bank selection, the anim cue dispatch [AUDIO_FORMAT.md §7, MIDDLEWARE_VERIFY.md §3].
- [ ] **PARTLY DONE:** The Miles mixing rules 1:1: gain law, 2D/3D multipliers, falloff, pan, low-pass, doppler, ducking, speed-of-sound delay. Done (0-C, CONFIRMED vs the exe): all of these and the loudness. Open: the headphones multiplier, low-pass at the output rate (PROVISIONAL).
- [ ] Footsteps and group movement by ground type, projectile impacts, explosions, cannon.
- [ ] Unit voices and officers' commands, battle ambience and weather.
- [ ] Advisor speech, and campaign ambience in full (emitters exist).
- [ ] Music by battle and campaign phase and state, with the original fade timing.
- [ ] Per-soldier firing sounds driven by animation cues (`.anim_sound_event`, cue meanings UNKNOWN).
- [ ] The bank selection rule (Ghidra).
- [ ] Trace in Ghidra the exe's sound start-rule order (repeat limit, probability, at-once, then file/pitch/delay RNG draws); ours is the round-13 order, untraced.
- [ ] Whether the exe's at-once / voice limits count finished one-shots not yet freed and voices fading out (ours: `Player::live` counts both; untraced).

## 10. Multiplayer
- **Decided (user, 2026-10-03): multiplayer is only between copies of NapoleonRust.** No protocol compatibility with the original
      game, and no Steam networking or matchmaking: our own networking layer.
- [ ] Networking layer and lockstep determinism checks.
- [ ] Lobby, matchmaking or direct connect, and chat.
- [ ] Multiplayer land and naval battles, unit purchase screens, ranked/unranked as the original offers.
- [ ] Two-player Coalition campaign, and drop-in battles, if the original supports them.
- [ ] Desync detection and multiplayer replays.

## 11. Modding (goal: original mods work out of the box, plus easy modding)
- [ ] **PARTLY DONE:** Parked on `work/mod-loading`: layered Vfs, `user.script.txt` parser, DB merging, loc rules [MOD_LOADING.md §7]
- [ ] Wire it into the app: `--mods`, `--no-mods`, `--list-mods`, the `mods\` folder with `load_order.txt`.
- [ ] Exact original pack priority and DB merge order (Ghidra) [MOD_LOADING.md §5]
- [ ] Test with real original-game mods (unit packs, overhaul mods, map mods).
- [ ] Optional open formats: glTF models, image heightmaps, hi-res texture overrides.
- [ ] Modding docs and our own tools (pack editor, DB editor, map tools), so modders don't need the original's closed tools.
- [ ] **No engine limits** (user, 2026-10-06; see Goal in `CLAUDE.md`). Defaults stay 1:1.
  - [ ] Audit the code for hardcoded counts and small ID types: `u8`/`u16` ids, fixed arrays, `MAX_*` constants and
        loops over a fixed number of factions, regions, religions or cultures. Make each data-driven.
  - [ ] Record the original's limits in the notes (CONFIRMED from the exe where possible): faction, region, religion and
        culture caps, units per army or fleet, unit size scale, battle unit and soldier caps.
  - [ ] Turn the gameplay caps into settings or data values, defaulting to the original's numbers: the 20-unit army and
        fleet, the unit-size options and the battle unit cap.
  - [ ] Saves, the UI (lists, scrolling, faction colours and flags for many factions) and multiplayer must handle counts
        past the original's limits.
  - [ ] Test with a synthetic mod that goes past every limit (e.g. 300 factions, 1,000 regions, 40-unit armies,
        1,000-man units).

## 12. Platform, release and quality
- [ ] Find the install automatically (Steam library folders) and ask if it's not found. Never write into it.
- **Decided (user, 2026-10-03): no Steam features** (no achievements, no Steam API, no Steam overlay). Ownership: players must own the
      game on Steam, because all assets come from their own install. The game starts only with a valid install and never ships assets.
- [ ] Performance targets on big battles (e.g. 10,000+ men), plus a release-build profile.
- [ ] Logging, crash reports and a settings reset.
- [ ] Packaging: ship only our executable, never Creative Assembly assets.
- [ ] Clippy and cleanup passes, and docs for building and running.
- [ ] File-size guard: a test (like `napoleon/tests/encoding.rs`) that fails when a non-test `.rs` source file passes 3,000 lines, so no file grows like `ui/campaign.rs` (7,985) again. Its allow-list holds only `ui/campaign.rs` until the Polish split, then is empty.

## Polish
Non-blocking review findings, one line each (CLAUDE.md "Done means"). A worker editing a file clears that file's lines.
- [ ] Clear the 28 existing `cargo clippy --workspace --all-targets` warnings, then drop `continue-on-error` from the Clippy step in `.github/workflows/ci.yml` and add `-- -D warnings`.
- [ ] AI table fallback can spend budget on a restricted level, unreachable in game (ntw_ai/src/campaign/mod.rs, construction table fallback ~line 1081).
- [ ] `slot_candidates`' `only` parameter could be a bool helper "is this level a candidate" sharing the match (ntw_sim/src/campaign/commands.rs ~1235).
- [ ] A refused negative construction cost reports `InsufficientFunds { needed: -300 }`; a dedicated error would read better (ntw_sim/src/campaign/commands.rs `construct` ~1370).
- [ ] `region_effect_set` recompiles the two tax-level bundles through the effect mapping on every call; compile them once per rules load (ntw_sim/src/campaign/economy.rs ~687).
- [ ] Lazy layout relies on every `n.rect` read going through `lay_out_if_stale` / `rect_of`; a new read that forgets it sees stale geometry (ntw_script/src/ui/host.rs ~550).
- [ ] `crates/napoleon/src/battle/view.rs` `GroundSpeed::observe`: when one frame spans several model ticks (low frame rate, fast battle speed) and the unit moved in only some of them, its speed is averaged over all of them for that frame (too slow until its next moving tick, at a start or a stop).
- [ ] A restart at tick ≤ 1 that reaches the old tick by the next frame isn't detected: one frame of Run (napoleon/src/battle/view.rs ~130).
- [ ] A teleport (scripted or reinforcement placement) forces Run for its window, not only a high rate (napoleon/src/battle/view.rs ~871).
- [ ] The encoding guard misses files saved in the ANSI codepage (add a `str::from_utf8` check), and `Â°`, `Â·`, `Â` + NBSP (napoleon/tests/encoding.rs ~13, ~54).
- [ ] The encoding guard skips `.opencode/` and `tools/`, and panics with no `.git` (napoleon/tests/encoding.rs ~22, ~31).
- [ ] No test loads a save written by the old clamped construction-cost writer (ntw_campaign/src/save.rs ~1465).
- [ ] HOT PATH (older code, clear first): every volley clones `SoundData` and allocates two `Vec`s (napoleon/src/audio/mod.rs ~1455).
- [ ] Untagged guess (older code, clear first): the 200/500 projectile-distance fallbacks have no tag and nothing logs when the setting is missing (napoleon/src/audio/mod.rs ~206).
- [ ] The projectile kind is a bare 0/1/2 index tied to `PROJECTILE_KINDS` only by order; use an enum (napoleon/src/audio/mod.rs ~1461).
- [ ] The `bad` closure repeats the failed-file test of `any_playable`; a `first_playable` helper would replace both (napoleon/src/audio/mod.rs ~1024).
- [ ] The missing-paths log-once guard checks what `SoundData::new` guarantees; a `debug_assert` would do (napoleon/src/audio/mod.rs ~996).
- [ ] The debug check of `UiWorld::update_appearance` compares the children list by buffer and length, so an in-place reorder of children inside an appearance change goes unnoticed (ntw_script/src/ui/world.rs ~168).
- [ ] Each volley looks its gun's shots up by string hash (gun type, then each projectile key through `Table::get`; ntw_data/src/database.rs `gun_shots`); row numbers would need a cheap check that `projectiles` was not replaced.
- [ ] A duplicate `REGION` id record in a file also gets the first region's recruitment queue when saved (save.rs `write_regions`, matched by region id; malformed files only).
- [ ] Logging is inconsistent: ntw_data reports with `eprintln!` while ntw_campaign uses the `log` crate.
- [ ] Untagged guess (older code, clear first): a new recruitment item whose kind has no manager falls back to manager 0, which can put a land unit in a port's manager or a ship in the land one (now tagged INFERRED; ntw_campaign/src/save.rs `write_region` ~1364).
- [ ] Queued recruitment items whose region has no recruitment manager are logged but still lost from the save (malformed files only; ntw_campaign/src/save.rs `write_region` ~1367).
- [ ] Harnesses that pulse rarely see `CampaignUI.Time()` stand still between pulses (ntw_script/src/ui/campaign.rs ~3436).
- [ ] `Time<Real>` has no per-frame cap, so a stall (end turn, return from battle) skips UI transitions to their end; say so in the PROVISIONAL note or cap it (napoleon/src/campaign/hud.rs ~150).
- [ ] The HUD harness still times its clicks on virtual `Res<Time>` (napoleon/src/campaign/hud.rs ~341).
- [ ] The hover cost check (<500 ms) was removed and nothing replaces it (ntw_script/tests/campaign_ui.rs ~1105).
- [ ] `WindowsTime`'s start resets with each CampaignUi; the exe's never resets (ntw_script/src/ui/campaign.rs ~3443).
- [ ] CAMPAIGN_UI.md ~100 calls it "the HUD's whole-ms clock", but `clock_ms` is fractional and `pulse` floors it.
- [ ] Technology tree: the INFERRED parts of `technology_parent_offsets` ("same column" = same chain, level = `building_levels.level`) carry no PROVISIONAL marker (ntw_script/src/ui/campaign.rs ~3281).
- [ ] `character_details` clones a colonel's whole force for his CommandedUnit card (ntw_script/src/ui/campaign.rs ~2074).
- [ ] The Lists build a full `character_details` table per row (ntw_script/src/ui/campaign.rs RetrieveFactionMilitaryForceLists).
- [ ] Technology parent offsets are rebuilt on every TechnologyPlayerDetails call; compute once per DB (ntw_script/src/ui/campaign.rs ~3363).
- [ ] Harness `selectchar:` takes the first substring match and leaves `demo_target` set (napoleon/src/campaign/hud.rs ~568).
- [ ] The no-portrait fallback covers admirals in the Lists (`character_details`) but only generals in `unit_entry` (ntw_script/src/ui/campaign.rs ~2011).
- [ ] `portrait_card` clones a String per Lists row; better: give portraits in the model when characters are created (ntw_script/src/ui/campaign.rs ~612).
- [ ] `FactionDetails` Leader.Portrait is the bare card path, without the `data/` prefix `unit_entry` and CardImage use (ntw_script/src/ui/campaign.rs ~694).
- [ ] `CursorPosition` (ui_prelude.lua ~281) and CampaignUI's label `ScreenPos` (Labels.lua) give screen pixels while the campaign HUD's Position / MoveTo use the 1280x960 scripts' frame; read what the original returns on a wide screen.
- [ ] Repeated MoveTo on a divorced node stacks offsets (older code; ntw_script/src/ui/host.rs ~1259).
- [ ] Two host flags (`pages_fill_screen`, `script_frame`) allow an invalid combination; one enum would prevent it (ntw_script/src/ui/host.rs ~277).
- [ ] The Enlist docking test loads the whole install twice (ntw_script/tests/campaign_ui.rs ~903).
- [ ] Split `ntw_script/src/ui/campaign.rs` (7,985 lines) into one module per screen; do it in the same branch that clears this file's other Polish lines, so no two workers edit it at once. Pure move, no behaviour change; tests must pass unchanged.
- [ ] `unwrap()` audit on paths that read install or mod data (ntw_formats, ntw_data, pack loading): a malformed file or broken mod must log one error and skip, never panic. Unwraps on the code's own invariants and in tests stay.

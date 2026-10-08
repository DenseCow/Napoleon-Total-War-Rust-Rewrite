# Save compatibility with the original game (BACKLOG §1)

Goal: saves written by NapoleonRust load and play in the original Napoleon: Total War.
Tags: **CONFIRMED** (seen in the bytes / in the exe), **INFERRED**, **UNKNOWN**.
Ghidra: the `NR-spt-ghidra` copy, `analysis/campaign/run_ghidra.ps1` + `ghidra_scripts/SaveDecomp.java`
(output stays out of git). Research tool: `cargo run -p ntw_campaign --example save_audit -- ...`
(`forces`, `new`, `find`, `findall`, `orphans`, `refsites`, `check`, `records`, `paths`, `show`,
`items`, `item`, `diff`, `layout`, `faction_fields`, `commander_fields`, `recruits`, `obstacles`,
`obstacle_refs`, `nan`, `u32s`). Test saves: `nr_test_saves` (§8, §9).
**Evidence files** (all vanilla; the user deleted the original's save folder twice, 2026-10-03 23:29
and 2026-10-04 02:55; nothing is ever restored there): our copies in `target/tmp/` and
`target/evidence/` (not in git): `nr1/nr2/nr3.save` (round 1), `auto_nr1.save` (the original's own
auto_save after loading NR-1 and ending a turn), `original/` = the original's earlier France
`auto_save` (turn 4, from our pre-fix turn-1 save `NR-A`) and `NR-A`; `auto_nr4_t4` (the original
after 3 End Turns of NR-4), `auto_b2b3_0211`, `auto_after_c8`, `orig_over_nr4_0252` (the original's
saves while playing NR-B2/B3 and NR-C8), `orig_fr_t1` / `orig_fr_t1_b` / `auto_orig_spa_0245` /
`orig_fr_may1811` (the user's own new Peninsula campaign, turn 1 and after 3 End Turns), and the
crash dumps in `target/tmp/dumps`. The June Coalition (`mp_eur_napoleon`) Britain saves and
`quick_save` were made with the NTW3 mod: they are no evidence and are kept aside in
`target/ntw3_modded_quarantine` only until the user says they can go (they are the last copies of
the user's files, so not deleted by us). Rules first learnt from them are re-checked on the
vanilla pairs (§19).

## Where I am / what's next
- **Now (2026-10-04, 12:30, round 9, §30): NR-12 / NR-13 failed to LOAD in the AI block** (the
  0x00B979C3 crash is the destructor of the half-built env): our removal cascade shifted the incoming
  link indexes (a pair's value indexes the source's `BLOCK_OWNS` list; CONFIRMED in `0x00CFD790` and
  100 % on the data). Fixed (`cai.rs` renumbers, new `check_block` rules), two cascade sites relabelled
  `subject`, the emulation widened (AI-block link indexes, zero and shape censuses against 11 vanilla
  saves). New saves `NR-14 France AI turns.save` / `NR-15 France recruit merge build.save` in the user's
  folder (copies in `target/tmp/round13`), clean under every check; a cdb loader probe
  (`target/tmp/loader_probe.cdb.txt`, §30) logs every loader step and failure during the user's test.
  Next: the user's load test.
- **Round 8 (2026-10-04, 11:00, §29): the 0x00B1D3E0 crash has a CONFIRMED root cause** (a cell
  version left without its grid node because our removal filtering gave two rows of one node the same
  pair list; the original drops such versions whole), a **loader replay** (`grid_load_check`, in
  `save_check` and `save_audit load_check`) that passes every vanilla save / start position and every
  NR save that loaded and faults every NR save that crashed, and the **writer fix**. New test saves
  from the fixed writer (replay clean, 0 violations): `NR-12 France AI turns.save` (5 End Turns) and
  `NR-13 France recruit merge build.save` in the user's folder (copies in `target/tmp/round11` as
  NR-5 / NR-6). Next: the user's load test of NR-12 / NR-13; then NR-11-style obstacles for new
  commanders (§22) as a further new file.
- **HANDOFF (2026-10-04, ~09:25; the save crash work moved to another worker).**
  - **Latest result:** the user's test shows NR-9 and NR-10 STILL crash on loading, at the
    ORIGINAL site again: fault 0x0071D3E0 (VA 0x00B1D3E0). There are 4 Event 1000 entries at
    09:22 and dumps `Napoleon.exe.14888` / `17868` in `%LOCALAPPDATA%\CrashDumps`. They are not
    yet copied to `target/tmp/dumps` and not yet analysed.
  - **Crash history:**
    - Round 1 (NR-2/3), round 5 (NR-5/6) and round 6 (NR-7/8) failed at 0x00B1D3E0.
      - The chain is `0x00AF89C0` (pathfinder loader) → `0x00B78C60` (re-registers obstacle
        boundaries from the grid nodes) → `0x00B07780` → `0x00B07550` (walks a linked list of
        boundary pieces) → `MOV ECX,[ECX]` on a next pointer of 0x3F800000.
      - Fix (§18): no empty grid nodes, row field x untouched, cell map renumbered.
    - Round 6's NR-7/8 then got further and crashed at 0x00AECFE6.
      - Cause: a grid-row pair list that is not in the `OBSTACLE_BOUNDARY_MANAGER`.
      - Fix (§19): lists filtered independently, manager deduplicated and synced, moved
        obstacles in the original's state.
    - NR-9/10 (round 7) now crash at 0x00B1D3E0 again. So the §19 changes either re-broke
      something in the piece lists, or the §18 fix was never the whole cause.
    - Saves with an EMPTY obstacle plan (no removals or copies) always loaded: NR-4, NR-B2,
      NR-B3, NR-C8. The obstacle plan (`obstacles.rs`: removals, copies, `clear_core`,
      `sync_manager`, `drop_empty_grid_nodes`) is the prime suspect.
    - NR-B1's 0x79421E was the AI-block rebuild (version 12, §5): fixed, CONFIRMED.
  - **Suspects for the next worker, in order:**
    1. `0x00B78C60` / `0x00B07550` walk boundary pieces per grid row: piece id → `OBSTACLE_BOUNDARIES`
       piece list. A row naming a piece that our plan freed or moved, or a piece whose use count or
       links we changed, would give a garbage next pointer. Audit piece ids in grid rows against
       live pieces (use count > 0) and their link fields.
    2. `clear_core` on moved characters: the original's state is CONFIRMED (§18, 13/13 + 8/8), but
       the pieces it frees may still be referenced elsewhere (other rows, the manager).
    3. Bisect as in §18. Write new saves from the same source with: removals only, copies only,
       `clear_core` only, `sync_manager` only. Each goes into the user's folder as a NEW file only
       when the manager asks; never restore or overwrite anything there.
    4. Read the two new dumps with `dump_stack` (example in this crate). Compare the list node and
       row being walked with the save's grid (`save_audit grid_items`).
  - **Rules derived (all are `save_check` rules unless noted; CONFIRMED on the vanilla saves kept):**
    - human flags (§2);
    - object ids (§3);
    - character, force, unit and settlement links (§4);
    - a commander rides on his own force's unit (§23);
    - a post and its holder point at each other; trait and ancillary uniqueness;
    - AI block version 12 kept and its mirrors in step (§5, §10; `cai.rs`, `cai_world.rs`);
    - grid: no empty nodes, row x untouched, the cell map a permutation (§18);
    - every grid-row pair list is in the boundary manager, the manager is a set, list 2 is a
      permutation of list 1, `piece_refs` and `obstacle_consistency` (§19);
    - used pieces have 3+ points and run counter-clockwise (§22);
    - unit name in-use flags match the units (§20);
    - victory conditions not met at once (§17);
    - three shroud trees on one grid (§28).
  - **Save items done:**
    - names of new characters, officers, regiments and ships (§20, §21);
    - traits, ancillaries, deaths, recruitment pools (§23);
    - technology research (§24);
    - header territory picture (§25);
    - governorship taxes (§26);
    - duel counters (§27);
    - shroud trees #0/#1, CHARACTER #17/#22, EXPOSED_CHARACTERS (§28; LINE_OF_SIGHT is not rebuilt
      by the original on save, so it is left as stored);
    - commodity market, embarked armies (§16);
    - economy (§12).
    - Vanilla saves still write back byte-exact: 4 copies in `target/tmp/bytecheck`, run with
      `NTW_SAVE_DIR` set to that folder.
  - **Trial saves:** NR-11 (= NR-9 + obstacles for new commanders, §22) waits in `target/tmp/nr11`.
    Do not copy it to the user's folder, since NR-9 itself crashes. The user's folder holds only
    NR-9 / NR-10; leave them alone.
  - **Evidence saves (original-written, read only):** `target/tmp/bytecheck/*`,
    `target/tmp/{auto_*,orig_*,b2_user,b3_user,nr4_user..nr6_user}.save`. The vanilla rule applies:
    NTW3/modded saves are not evidence.
- **Now (2026-10-04, 03:50, round 7, §19):** round 6 (NR-7/8) got past 0x00B1D3E0 and crashed at
  0x00AECFE6: a grid row's pair list missing from the boundary manager. Fixed with the rules of
  the original's saves (lists filtered one by one, manager a set holding every row list, moved
  obstacles as the original leaves them); new `NR-9 France AI turns.save` and `NR-10 France recruit
  merge build.save` are in the user's folder for the morning (the round-6 NR-7 / NR-8 there are
  obsolete). Every grid rule known from the 7 vanilla originals holds on them.
- **Vanilla only** (user rule): the June Coalition saves were modded (NTW3); quarantined, not
  evidence; the AI-block cascade re-checked on vanilla pairs (§19).
- **Next:** the user's NR-9 / NR-10 result (if a crash: event log + `dump_stack` on the new dump).
  Done: commodity prices, price history, trends and factors written (`trade::write_market`,
  `CAMPAIGN_TRADE_MANAGER` #3-#7; round-trip test `commodity_market_is_written`).
  Regiment and ship names of new units: done (§20). Character and officer names: wired in (§21;
  algorithm CONFIRMED, random stream PROVISIONAL). Obstacles for new commanders: the ports
  worker's `grid_obstacle::add_character_obstacle` is on main; whether to use it is decided with
  evidence: kept "no obstacle" (§22: ours makes 2 clockwise sliver pieces in NR-10, a new
  `save_check` rule CONFIRMED on all 11 vanilla saves; otherwise every rule holds).
- **Shroud and sight (§28, for 0-G):** shroud trees #0 and #1, #17, #22 and EXPOSED_CHARACTERS written from the
  model; LINE_OF_SIGHT left as stored (the original does not rebuild it on save); new rule: three shroud trees
  on one grid. Vanilla saves still byte-exact.
- **Territory picture (§25) and governorship taxes (§26):** the header picture is rebuilt from the owners
  (rule pixel-exact on 14 original saves); tax levels written per governorship from the model.
- **Technology research (§24):** state, progress and researcher written from the model; unchanged saves still
  write back byte for byte (4 vanilla saves).
- **Round 14 (2026-10-04):** a dead commander's force can go to an existing character (0-G's exe order),
  written and checked (new rule: a commander rides on his force's own unit, CONFIRMED on 11 vanilla saves).
  Obstacles for new commanders now pass every rule with the fixed cut; trial save NR-11 (= NR-9 + obstacles)
  is ready in `target/tmp/nr11`, for the user's folder once NR-9 / NR-10 load (§22 update).
- **Characters in play (§23, for 0-G):** traits, points and ancillaries written for every character; a
  dead commander's force goes to a new colonel named after its unit officer; pools written from the model;
  2 new `save_check` rules; a 26-turn run across a year end saves with 0 problems. 0-G is merged into this
  branch so both can be merged together.
- **§1 time-box (2026-10-04):** farm piece pairs = {wall index, side} and the owner's second u32 = the tile
  template index (both CONFIRMED, S1_LEFTOVERS.md §2); UI `unknown_e5` = UseGlobalClicks, behaviour CONFIRMED in
  every use (UI_LAYOUT_FORMAT.md). BACKLOG §1 wording updated; check marks left to the manager.
- Names background (§14): pools, weights, shuffle and builder CONFIRMED (`names_fit`,
  `names_sizes`, `names_refill`). Regiment names (§20): land list class = row index of
  `unit_class` (CONFIRMED); a new unit takes the lowest free name of its class (INFERRED, ~99%).
- Small fix owed: the `World::embarked` doc in `ntw_sim` still says our writer does not store the
  link; it does now (§16).
- Round 1 (NR-1..3): NR-1 loaded but France played itself; NR-2/NR-3 crashed at ~25% of the
  loading bar. Rounds 2-4 (NR-4, NR-B1..B3, NR-5, NR-6; §9, §11, §13): results in §17.
- Done: human player (§2); the AI block kept at version 13 and in step with the world (§10, incl.
  settlement and slot garrisons); REGION economy fields, trade routes' accumulated values and the
  bankrupt-turn counter written (§12); all rules in `save_check`, CONFIRMED on every original save.
- Waiting for: the round-5 results; an original eur_napoleon France pair (turn 1 and after 2-3 End Turns) to
  check §10 against a France campaign.
- PROVISIONAL (§13-§21): the names' random stream (§21), pathfinder obstacles for new commanders (needs the
  original's boundary cutting and grid registration ported; not needed to load, §17),
  CHARACTER v12 → v14 and SETTLEMENT
  v2 → v3 upgrades (layouts known, not needed to load), new mirrors start without beliefs,
  transitive link lists only cleaned, `PENDING_BATTLE` (not reachable: our game autoresolves at once).

## 1. Which saves count as evidence
`ntw_campaign::save::written_by_napoleonrust(file_name, esf)`: a save is ours if its name starts with
`NR-` (any case; the user names our test copies so) or if it is a `CAMPAIGN_SAVE_GAME` that still holds
start-position record versions (`CHARACTER` < v14 or `SETTLEMENT` < v3). The original upgrades those
records when it saves (CONFIRMED: its `auto_save` written from our turn-1 save has only v14 / v3), our
writer keeps the source's records, so every save we write from a start position carries them.
(The tests `user_saves_round_trip_byte_exact`, `user_saves_pass_the_checks` and `loads_user_saves` that read the user's save folder were removed 2026-10-08: loading the original's saves is out of scope.)
The round-trip test compares floats by bits: the eur startpos holds NaN (bits `0xffffffff`) at
`REGION` #42 of 7 regions (CONFIRMED in `startpos.esf`, kept by the original in its own `auto_save`;
not something we introduce).

## 2. The human player (CONFIRMED)
- `CAMPAIGN_PLAYER_SETUP` #3 is the human flag, and a save stores it **twice**: in the setup
  (`CAMPAIGN_ENV/CAMPAIGN_SETUP/CAMPAIGN_PLAYERS_SETUP/PLAYERS_ARRAY[]`) and in each faction's own
  record (`WORLD/FACTION_ARRAY[]/FACTION` #1). A startpos has both false everywhere; every Britain
  save has both true for exactly `britain` (CONFIRMED). The game plays by the **faction's copy**: the
  faction loader `FUN_0087a190` reads it with the shared reader `FUN_008768c0` into the faction at
  +0x638 (the bools land at +0x6e0/+0x6e1/+0x6e2), and ~60 functions test faction +0x6e0 (Ghidra
  `scal:0x6e0`; e.g. the small getters `0x008E2B70`, `0x008CA100`, and the model loader
  `FUN_00872550` itself). CONFIRMED.
- Round 1 set only the setup copy: NR-1 loaded, but France still ended every turn by itself, and the
  original's own `auto_save` written while playing NR-1 kept France's faction copy false (it does not
  copy the setup flag into the faction). CONFIRMED by the user's test and that auto_save.
- **The AI manager type** (CONFIRMED in the saves and the exe): `CAI_INTERFACE` #29 = pairs (faction
  component, manager type), repeated in each `CAI_INTERFACE_MANAGERS[]/CAI_FACTION_MANAGER` {#0
  component, #1 type}; the factory `FUN_00a87f50` reads the type from that table (`FUN_00a88cb0`,
  missing = 13) and builds the manager for it (case 11 = the DB manager `0x00C90960`, case 10 =
  `0x00C91F60`). The component of a faction: first plain value of its `CAI_WORLD_FACTIONS[]` item,
  whose `CAI_FACTION` #6 is the faction id (`FACTION` #8). Startpos: every faction 11 (rebels 2).
  Britain saves: `britain` 10, every other 11. Names by the `CAIMT_*` string order (INFERRED):
  10 HUMAN, 11 DB, 2 REBELLION. So in NR-1 France's turn was also run by the DB AI manager. The type
  is stored, it does not follow the human flag.
- Writer: `write_human_flags` sets both copies, `cai::write_manager_types` gives the human faction
  type 10 (and a former human 11); `mark_human` does both on a tree; `save_check` checks both rules
  (every original save passes; the byte-exact round trip of every original save still holds).
- Also seen (not needed): the human's `FACTION` record has the same layout; its trailing u32
  statistics are non-zero only for the human (battle and economy counters, INFERRED).

## 3. Object ids (CONFIRMED in the exe)
- The id written for an object is a field of the object (`FUN_004ce450` returns `this+0x18`). Ids
  are pointer-like and all differ between a save and the original's next save of the same game
  (every character/force/unit id in its `auto_save` differs from our source): they are reassigned
  in the running game; a file only needs them unique and consistent.
- On load, references are resolved through **one global hash map** id → object
  (`FUN_0105ac60`, open addressing on `DAT_01464b8c`); a missing id gives 0. So ids must be unique
  across all object kinds, not per kind. Our new ids come from `World::alloc_id`: above every
  integer in the source file below `0x7fff0000`, in steps of 8.

## 4. How the original links characters, forces, units and settlements (CONFIRMED in its saves)
- Every force has a commander character (`MILITARY_FORCE` #1); `CHARACTER` #4 = the force it
  commands (0 if none); `UNIT` #10 = the character attached to the unit (the general for his
  bodyguard unit, the colonel/captain for a unit raised alone), and that character's #5 = the unit.
- A unit recruited where no army can take it becomes a new `ARMY` led by a new **colonel**
  (`CHARACTER` type `colonel`, its #4 = the army, #5 = the unit, the unit's #10 = the colonel; e.g.
  `auto_save` france army 728689440 / colonel 819984184 / unit 911044776). Ships: a new `NAVY` led by
  a new **captain**. When such a unit later joins another army its colonel stays attached to it
  (#4 = 0, #5 = the unit). Startpos units mostly have #10 = 0.
- A recruited unit's `UNIT`: `UNIT_HISTORY` date = the turn it was raised, `COMMANDER_DETAILS` = its
  colonel's names and faction, empty `TRAITS`, #5/#6 = full strength, #7 0, #15 -1, #16 1.0.
- **Garrisons:** `SIEGEABLE_GARRISON_RESIDENCE` #12 = the one force garrisoned there (0 = none) and
  that `ARMY` #5 = the residence id (`SIEGEABLE_GARRISON_RESIDENCE` #1 = `SETTLEMENT` #4). Slot
  residences (forts, ports: `REGION_SLOT` #0) work the same way. Navies have no residence link.
  `GARRISON_RESIDENCE` #0 = the owner faction id.
- `CHARACTER_DETAILS` #10 repeats the character id.
- Government posts (`CHARACTER_POST` #2) refer to characters (france's `faction_leader` = Napoleon).
- **Every id reference site** (`save_audit refsites FILE`: each integer equal to a character, force
  or unit id of the file; run on the startpos, the original's `auto_save` and a Britain save of
  1806). Besides the links above, the AI block (§5) and the pathfinder (§6):
  `CHARACTER_RECRUITMENT_MANAGER/{GENERAL,ADMIRAL}_RECRUITMENT` #0 (u32[] candidate generals and
  admirals; always characters of the faction without a force, CONFIRMED in all 8 user saves),
  `CHARACTER` #6 (a force; 1 hit), `MILITARY_FORCE` #2 (u32[] characters), residence #14 (u32[]
  characters in a settlement or slot), `PORT_GARRISON_MANAGER` #0 (the navy in a port),
  `TRADE_SEGMENTS[]/COMMERCE_RAIDS[]` #0 (the raiding character), `PENDING_BATTLE_PARTICIPANT` #0
  (a force), `SPYING_ARRAY[]/CAMPAIGN_SPYING/FORCE_DATA_BLOCK[]` (force and unit ids of a spied
  force; a snapshot, not checked: UNKNOWN whether resolved). A startpos uses only the pools.
  `save_check` checks every site (an existing object); the writer clears references to objects the
  model no longer has (`drop_dangling_refs`; test `removed_characters_leave_no_references`).

## 5. The campaign AI block (`CAI_INTERFACE`, CONFIRMED in the exe)
- `CAI_WORLD` mirrors every character (`CAI_CHARACTER` #3), force (`CAI_RESOURCE_MOBILE` #10), unit
  (`CAI_UNIT` #1), settlement and slot by id. After loading, "CAI: WORLD: Post Load Fixup A"
  (`FUN_00c48b60`) resolves each through the global map; one unresolved id makes it return false and
  the loader (`FUN_00a3ce70`) sets the load-failed flag.
- The loader (`FUN_00a3ce70`, called by the model loader `FUN_00872550` at its "Creating
  CAI_INTERFACE" step, after the world, trade and pathfinder) compares the record's version with 13
  (`FUN_00851540`). Version 13 (and the debug flag `DAT_015c574a` clear): it loads CAI_WORLD, the
  central pool, the managers and the **director BDI pool** (`+0x628`, `FUN_00b8eee0`), then the
  post-load fixups. **Any other version:** the block is skipped (`FUN_00de9320`) and it builds a new
  CAI world (`FUN_00bf2e20`), central pool (`FUN_00c8c4e0`) and managers (`FUN_00aae5c0(.., 0, ..)`)
  **but no director pool**: `+0x628` stays 0 (zeroed in the ctor). The new-campaign constructor
  `FUN_00a3eff0` makes the same three calls **and** the director pool (`FUN_00b8eea0`). The model
  loader then hands `CAI+0x628` on (`FUN_00b94170`, the CDIR interface, "Creating CDIR_INTERFACE"),
  and other code calls through it without a null test (`0x00A4FA77`, `0x00A68D0F`). CONFIRMED code
  paths; **INFERRED: a version-12 block crashes the original on load.** Round 1's writer wrote exactly
  that for every save after End Turns (NR-2, NR-3: both crashed; NR-1, version 13, loaded). Bisect
  save NR-B1 (§9) tests it alone.
- So the round-1 rule ("world changed → version 12, the original rebuilds its AI") was wrong. The AI
  block has to stay version 13 and match the world: done in §10.

## 6. The pathfinder's character obstacles (CONFIRMED in the saves; crash path INFERRED from the exe)
- `CAMPAIGN_PATHFINDER/PATHFINDING_GRID[0]/OBSTACLE_LISTS`: #2 u32[] character ids and
  `CHARACTER_OBSTACLE[]` {`OBSTACLE`, u32 character id}. Exactly the characters that command a force
  (in the field or in a garrison) and some agents on the map have one (3 saves checked); characters
  without a force (generals waiting, colonels inside armies, ministers) have none.
- Every other reference to an obstacle is a pair (character id | 2, boundary index): in its own
  `MANAGED_OBSTACLE_BOUNDARY`, in `OBSTACLE_BOUNDARY_MANAGER/OBSTACLE_BOUNDARY[]` and in the
  `OBSTACLE_BASE_GRID_NODE[]` lists (all pairs checked: tag 2, owner always a character obstacle).
- Loading (`FUN_00afa4f0`) looks each obstacle's character up in the global map and reads a field of
  it; a missing character is a null dereference: **an obstacle of a character that no longer exists
  crashes the original** (INFERRED from the decompiled code).
- So the writer removes the obstacles of characters that died or no longer command a force (every
  pair with that owner; grid-node and manager items left empty are dropped), and gives a new
  commander the obstacle of a source obstacle at the same position (a garrison in the same
  settlement) when there is one. A new commander with no such template gets no obstacle. Obstacles of
  characters that moved keep their old position (the original moves them when the character next
  moves; INFERRED harmless).
- **A commander without an obstacle** (CONFIRMED accepted, §17: NR-B2 loads and plays, and the
  original keeps such commanders without an obstacle over several End Turns): the original's saves never have one (0 in 8 saves), but its runtime handles it. A character
  obstacle is keyed by the character's field `+0xcc | 0x80000000` (`FUN_00ae8a50`, the obstacle
  constructor; load `FUN_00afa4f0` builds the same key). The refresh `FUN_00b1b7a0` (called through
  `FUN_00b1b700` / `FUN_00b795a0` for the characters of a moving group, 12 callers) looks the
  obstacle up (`FUN_00b314d0`, `FUN_00b28f60`, `FUN_00b00800`), removes it only when found
  (`FUN_00b654c0`) and then adds a new one at the current position (`FUN_00b08610` →
  `FUN_00b087a0` → insert `FUN_00b09030`). The other remove path (`FUN_00b54900`) is behind the same
  lookup. So a missing obstacle is created on the next refresh; no null use was found on these paths.
  Building our own obstacle (6-edge boundary, bbox, grid-cell ranges, the grid nodes' boundary
  pairs) is possible later if the test shows it is needed.

## 7. What our game did wrong (our 2-End-Turn save vs its source, `save_audit orphans`)
- 11 new armies with no commander (`MILITARY_FORCE` #1 = 0), cloned from the first army in the file
  (keeping its #5 garrison link and regiment).
- 12 forces merged away: their former commanders still had `CHARACTER` #4 = the removed force, 11
  settlement residences still had #12 = a removed force, the AI block still named them.
- New ids were "largest + 1" per kind (odd, may collide with other kinds' ids in the global map).
- Fixes (model, `ntw_sim::campaign`, faithful to §4): recruited units join the settlement's garrison
  army (up to 20 units) or form a new army led by a new colonel garrisoned there; ships form a navy led
  by a new captain at the port; merging moves the units with their attached characters; a destroyed
  force takes its commander and attached characters with it (post holders escape: PROVISIONAL);
  garrisons are loaded from and written to the residences; ids from `World::alloc_id`.
- Writer (`ntw_campaign::save`, done): `CHARACTER` #4/#5 and `UNIT` #10 from the model, new
  colonels / captains cloned from a template character (traits and ancillaries emptied; names
  PROVISIONAL), new forces as the original raises them, garrison residences, the other id sites
  (§4), the AI block (§5), the obstacles (§6), the human flag (§2).
- `save_check` (all rules CONFIRMED patterns of the original's saves; every user save passes):
  human flag; ids unique across kinds; character ↔ force ↔ unit links; garrisons both ways; posts;
  every reference site of §4; obstacles and their pairs; the AI block when loadable. The tests run it
  on every save we write: startpos saves of the 8 campaigns, saves after 3 (and, ignored test, 10)
  AI End Turns saved twice, the removed-character test, and the generator below.

## 8. The test saves for the user (2026-10-03)
Made by `cargo run --release -p ntw_ai --example nr_test_saves -- DIR`: a new eur_napoleon campaign as
france played as the game plays it (the original's campaign scripts via `ntw_script`, the campaign AI
in the turn loop, written with the scripts' `save_value` slots like `F5`). Before writing, each save
passes `save_check`, re-serialises identically, reloads in our loader to the same world, and keeps the
model rules (every force has a commander of its faction and units; garrisons commanded from inside).
Copied into the user's save folder `%APPDATA%\The Creative Assembly\Napoleon\save_games`:

| file | content | `save_check` |
|---|---|---|
| `NR-1 France turn 1.save` | the new campaign, turn 1, nothing done | 0 violations; AI block kept; every commander has an obstacle |
| `NR-2 France AI turns.save` | after 5 End Turns (turn 6): AI factions moved, recruited, merged, fought | 0 violations; AI block rebuilt by the original; 5 new commanders without obstacle |
| `NR-3 France recruit merge build.save` | france recruited (into the Alsace-Lorraine garrison, a new colonel-led army in Corsica, a brig in Bretagne), built (magistrate, court of justice), merged two armies, moved one, then 3 End Turns (turn 4) and merged a new army | 0 violations; AI block rebuilt; 6 commanders without obstacle |

What each tells us: NR-1 = the human flag (France must wait for the player at turn 1). NR-2 = the
earlier crash after End Turns (commander-less forces, dangling ids, stale AI block) is fixed, and
the AI rebuild path works. NR-3 = our recruited units, new colonel armies / navies, buildings in
progress and merges are understood by the original. Results go here when the user reports them.

User test steps (sent with the saves): start the original Napoleon: Total War, Campaign → Load Game,
load each NR save in turn. Report per save: does it load (or crash / error message, at which point
of the loading bar); is it France's turn and does the game wait for you (no turns passing by
themselves); is the map right (armies, the new Corsica army and Bretagne brig and the two buildings
in progress in NR-3); then press End Turn 2-3 times: do the AI factions move, does anything crash,
do the recruited units / buildings complete. The game's `auto_save.save` after End Turn is useful
evidence too (it is the original's own save of our game).

**Round 1 results (user, 2026-10-03 evening):** NR-1 loaded but the game ended every turn by itself
(France not human: §2, the faction copy of the flag and the AI manager type were missing). NR-2 and
NR-3 crashed the original at ~25% of the loading bar; no crash log. Cause (INFERRED, §5): their AI
block was written as version 12, and the original's rebuild path for other versions leaves the AI
director pool null. The loading bar's steps are not mapped (the section names `FUN_01095dc0` logs are
timings, not the bar); if the bar follows the file position, 25-27% is where `WORLD` ends and
`CAI_INTERFACE` begins in these files (`save_audit layout`: WORLD 2.3-27.4%, CAI 27.4-87.7%), which
fits. The original's `auto_save` written while playing NR-1 is in `target/tmp/auto_nr1.save`.

## 9. Round-2 test saves (2026-10-04)
Made by `nr_test_saves -- DIR round2`; every save passes `save_check` (now with the faction human flag
and AI manager rules), re-serialises identically and reloads in our loader. Copied into the user's
save folder (empty before: the user had cleared it):

| file | content | tests |
|---|---|---|
| `NR-4 France turn 1.save` | new campaign, turn 1; differs from NR-1 only in France's faction human flag and AI manager type 10 | the human fix (§2) |
| `NR-B1 France turn 1 AI rebuilt.save` | NR-4 with only the AI block's version set to 12 | the AI rebuild path alone (§5; expected: crash) |
| `NR-B2 France turn 1 new units.save` | NR-4 plus units raised as recruitment does: a new colonel army in Corsica, a new captain navy (brig) in Bretagne, a unit joining the Alsace-Lorraine garrison; AI block kept (v13) | our new character / army / navy / unit records, objects the AI does not know, commanders without obstacle (2) |
| `NR-B3 France turn 1 garrison unit.save` | NR-4 plus one unit joining the Alsace-Lorraine garrison; AI block kept | the smallest addition (a new unit record only) |

Reading the results: NR-4 waits for the player → human fix done. NR-B1 crashes → the version-12
path is the round-1 crash; the AI block must be maintained (§5 "What is needed"). NR-B1 loads →
the crash is elsewhere: bisect the world changes next. NR-B2/NR-B3 load and play → new objects
without AI mirrors are accepted (then only removals need AI-block work); B3 loads but B2 fails →
new characters / forces / missing obstacles are the problem.

## 10. Keeping the AI block in step with the world (CONFIRMED rules, `cai.rs`, `cai_world.rs`)
Evidence: first the June Coalition sequence (made with the NTW3 mod: no longer evidence, §19), the
original's France auto_save of our NR-A, and NR-1 → the original's auto_save of it. Tool:
`cargo run --release -p ntw_campaign --example cai_audit -- ...` (`graph`, `ids`, `pair`,
`removal`, `sites`, `simulate`, `links`, `mirror_rel`, `mirror_loc`, `mcheck`, `owns`, `owned`,
`counters`, `shapes`, `tas`, `newmirror`, `comp`, `show`).
- **Components.** Every AI object carries a block after its `CAI_BDI_COMPONENT_PROPERTY_SET`
  (+1 id, +2..+5 f32, +6 u32, +7 bool, +8 slot list (ids or 0), +9 incoming pair list {source,
  value}, +10/+11 incoming link counts per slot, +12/+13 and +17/+18 id lists per slot (transitive
  ancestors / descendants, INFERRED from the ctor notes in AI_RESEARCH.md), +14 `BLOCK_OWNS` =
  outgoing links {target, this, f32 add, f32 mult, u32 slot}, +15/+16 outgoing counts per slot,
  +19/+20 a symmetric pair of id lists, +21 bool, +22 u32, +23 bool). Ids are stable runtime ids
  (the same object keeps its id from save to save); `CAI_CENTRAL_BDI_POOL` #0 = the next id (max + 1
  in every save). Some records hold several blocks (`CAI_WORLD` ends with `CAI_HISTORY`).
- **Invariants on every original save** (`cai::check_block`, `cai::check_link_counts`): ids unique,
  below the counter; every id in the link lists is a component (the slot list may hold 0); the
  incoming pair lists are exactly the other components' outgoing links, and the four counters are
  exact per slot; every non-zero value at a reference site of the site table is a component
  (except `tolerated` sites, where the original itself leaves dangling ids).
- **Removal** (`cai::remove_components`). The site table (`src/cai_sites.txt`, 461 data sites, made
  by `cai_audit sites` from the sequences) says per site what the original did with a reference to
  a removed component: `subject` (the referring component goes too, e.g. a TAS / strength / visibility
  analysis of a removed army), `entry` (the reference goes: array element, pair, data item, or 0),
  `tolerated` (left dangling). Link lists, owns links and the counters are kept exact. `CAI_TAS_ANALYSIS`
  holds three variable lists ([u32, u32, 3 x (n, n x {id, f32, f32, i32}), 42 x i32, bool, u32];
  CONFIRMED on every long record) handled as lists. Check (`cai_audit simulate` on 5 consecutive
  original pairs): removing the world mirrors the original removed, our cascade removes **no
  component the original kept**, and the result passes every invariant. The original removes more
  (beliefs it recomputes each turn).
- **Mirrors** (`cai_world.rs`, CONFIRMED 100 % on the original's saves): one mirror per character,
  force and unit (not the rebels'); the relations listed in the module doc (`CAI_UNIT` #0/#2,
  mobile #0/#4/#5 and `CAI_SITUATED` = the leader's position / region / theatres, `CAI_CHARACTER`
  #0/#1/#2, `CAI_FACTION` #3/#4, `CAI_REGION` #9, `THEATRE` #1, settlement `CAI_GARRISONABLE` #0).
  `sync` removes the gone, creates the new (a cloned mirror with a fresh block: no links, as the
  original's freshly created ones before its AI turn), sets every relation (regions from the movement
  grid's region map) and advances the counter. Runs only when the world changed (a loaded save is
  written back byte for byte). PROVISIONAL: new mirrors have no beliefs until the original's AI turn.

## 11. Round-3 test saves (2026-10-04)
`nr_test_saves -- DIR turns 5`; both pass every rule of `save_check` including §10 (and the generator's
reload / re-serialise checks). Copied into the user's save folder next to the round-2 files:

| file | content |
|---|---|
| `NR-5 France AI turns.save` | new campaign, 5 End Turns (turn 6): 13 forces gone (merges, battles), 6 characters, 62 units and 1 force new; AI block version 13, 69 new mirror components, the removed ones cascaded |
| `NR-6 France recruit merge build.save` | France recruits (Alsace-Lorraine garrison, Corsica, a brig in Bretagne), builds (magistrate, court of justice), merges two armies, moves one, 3 End Turns (turn 4) |

What to report: load (or crash and the loading-bar position); France waits for the player; End Turn
2-3 times (AI moves, no crash); the recruited units / buildings appear; keep the `auto_save.save`.

## 12. Economy fields in saves (2026-10-04)
- `REGION` #9 base GDP, #10 GDP, #12 town wealth, #14 stored town wealth, #15 growth, #17 bankruptcy
  offset (u32 in the files), #18 discontent growth, #19 tax exempt: written from the model
  (`write_region_economy`; positions from CAMPAIGN_FIDELITY.md, REGION writer `0x00A51E30`).
  **#14 = #12 in every region of every original save** (CONFIRMED). #13 and #16 are not modelled
  (kept). Test `region_economy_is_written`.
- **Bankrupt turns** = `FACTION_ECONOMICS` #3 u32 (CONFIRMED: the economics saver `0x00BD46E0` writes
  treasury +0x3F4 as #1, the per-category bytes as #2 and +0x460 as #3). Loaded into
  `World::bankrupt_turns` and written back.
- **Trade routes' accumulated value** (CONFIRMED layout from the route saver `0x00AFD490` and the
  accumulator `0x00B05CC0` adding to +0x3C): `INTERNATIONAL_TRADE_ROUTE` v3 = u32 `n`, `n` waypoints
  {i32 region id, coord, u32, u32, bool}, bool, u32[], seven u32 (+0x28..+0x40), u32[], u32 count +
  items, i32; +0x3C is child `5n + 8`. The importer is the owner of the last waypoint's region
  (INFERRED; `trade_check`: it is a model trade partner for 712 of 714 routes in 10 saves). Loaded per
  (exporter, importer) as the sum of the routes into `World::trade_accumulated` (before the
  trade stand-in, which therefore shrinks by it); written back with the change from the stored sum
  put on the pair's first route (two pairs have two routes). Test
  `trade_accumulation_and_bankruptcy_are_written`. Domestic routes are not modelled.

## 13. Round-4 refresh of the test saves and the PROVISIONAL list (2026-10-04)
Regenerated with the current writer (economy fields, slot garrisons; model fidelity from main) and
copied over the old files: `NR-B2` (its new army, navy and unit now have AI mirrors), `NR-B3` (its unit
has a mirror), `NR-5` (turn 6, 3 commanders without obstacle), `NR-6` (turn 4, 5 without). `NR-4` and
`NR-B1` unchanged (only their timestamps would differ). All pass `save_check` (B1 apart from its
deliberate version 12).
- **Obstacles for new commanders: not written yet.** A character obstacle's boundary slots name
  run-time boundaries (0x80000000 | map boundary index, or plain indexes of pieces in
  `OBSTACLE_BOUNDARY_MANAGER`), its `MANAGED_OBSTACLE_BOUNDARY` items and the
  `OBSTACLE_BASE_GRID_NODE[]` lists register those pieces per grid cell, and the pieces are the map
  polygons cut along the zone flood (army 12 / navy 6 units) and the 24-gon core (PATHFINDING.md §8,
  §10). Writing one faithfully means porting the original's cutting and grid registration
  (`0x00B09030` and callees); a wrong one is worse than none (the refresh `0x00B1B7A0` removes only a
  found obstacle, then adds a correct one, §6). NR-5 / NR-6 test the "none" case.
- **Names**: new colonels / captains / regiments keep the template's names (the original draws them
  from its name lists and `NAME_ALLOCATION_DETAILS`; not decoded).
- **Record versions**: CHARACTER v14 = v12 + {utf16 key, bool} (the key is set for 4-5 famous generals
  only, e.g. `Gen_Napoleon`, `Gen_Duke_of_Wellington`, others ""; source not found); SETTLEMENT v3
  replaces v2's #5 (a repeat of the #3 name) with a bool. The original loads both versions (startpos);
  not upgraded.
- **`PENDING_BATTLE`**: the game and the generator autoresolve a battle as soon as it is pending, so no
  save holds one; the writer keeps the source's (empty) record.
- **Slot garrisons on the AI side**: done (§10).

## 14. Names of new characters (2026-10-04; research notes, wired in by §21)
- **Table** `names` (CONFIRMED layout `s,s,s,s,i,b,s`, 13 945 rows): group, name, `forename`/`surname`,
  gender `m`/`f`/`b`, i32 (0/1/2; 0 on historical names such as Bonaparte, Bernadotte: INFERRED "not
  for random names"), bool (INFERRED noble, "de La Fayette"), id. Saves store the localisation key
  `names_name_<group><name>` (CONFIRMED; builder `0x00F44290`).
- **Allocators** (CONFIRMED in the exe): ten per faction (faction +0x530 .. +0x608), pools taken from
  the faction's names-group record (+0xF0 .. +0x19C), saved as the faction's ten
  `NAME_ALLOCATION_DETAILS` {u32 pool size, u32 seed, u16[] deck}. Draw `0x008A9FA0`: first deck
  entry, removed; refill `0x008EFB70` + shuffle `0x0086EA70` (MS LCG). Ported in `names.rs`
  (`Allocator`). Check (`names_check`): in 218 of 254 non-empty stored decks the deck is exactly the
  tail of our shuffle of its stored seed (the other 36: UNKNOWN; INFERRED removals from the middle by
  `0x008D3510`, not verified).
- **Pool membership: UNKNOWN.** My rule (forenames / surnames by gender, i32 > 0, bool) gives the
  stored pool sizes for France at turn 1 (190 / 181 / 240 / 22) but fails for about half the
  factions: e.g. german_catholic pool 0 stored 214 vs 147, pool 5 stored 2180 vs 208; and France's
  pool 0 is 190 in NR-1 but 189 in the original's later saves, so pools change during play. The pool
  builder (the names-group record, `0x00F4D5D0` and its caller) is not decoded. **Not used**: new
  colonels / captains / regiments still take the template's names (PROVISIONAL). Next: decode the
  pool builder, then wire `Allocator` into `spawn_recruited_unit` and the writer (and write the
  advanced allocator state back).
- Regiment names: `unit_regiment_names` and the faction's `LAND_UNIT_NAME_ALLOCATOR` /
  `NAVAL_UNIT_NAME_ALLOCATOR` are a separate allocator family; not looked at yet.
- **Round 7 (2026-10-04, 04:00), vanilla evidence only.**
  - Pool rule re-fitted on the vanilla saves: 163-168 of 168 sizes per pool; the misses are the stale
    eur start position sizes.
  - **How a draw becomes a name (CONFIRMED in the exe):** `0x008A9F40` returns `pool[index]`, a
    name-record pointer from the pool array (count at +8, data at +0xC).
  - **Pool binding (CONFIRMED):** the faction loader (`0x0087AA92..`) binds its ten allocators
    (+0x530 .. +0x608) to the lists of its names-group record (`faction +0x514`):
    - group lists +0xF0, +0x100, +0x110, +0x120, +0x130, +0x140, +0x150, +0x19C, +0x160, ...;
    - allocators +0x530, +0x548, +0x590, +0x5A8, +0x560, +0x5C0, +0x578, +0x5D8, +0x5F0, +0x608.
  - **Group record (CONFIRMED):** its own name list (+0x18) and forts list (+0x28) are filled in
    `names` / `names_forts` table order (`0x00F43F40`).
  - **Not found yet:** the code that fills the +0xF0 .. lists. So the order of a pool is still
    UNKNOWN.
  - **What the draws show:** in the user's Peninsula game (turn 1 → turn 4), spa_spain drew 8
    forenames and 8 surnames, and exactly 8 new unit officers appeared. So each new unit draws an
    officer forename (pool 0) and surname (pool 4) (INFERRED).
  - **Orders tried, none of which matches the drawn indices to those officers' names
    (`names_draws` SEARCH):**
    - table order;
    - interleaved by weight;
    - id as a number, as a string and as u32;
    - name, lowercase key, gender, weight;
    - every rotation.
  - The old "nowhere" counts were inflated: ids change on every load, so characters are matched
    by name, not by id.
  - **Next:** find the writer of group +0xF0 (scalar searches for the list fields, or the
    names-group table's post-load step), then wire the names in.
- **Pool builder found (2026-10-04, round 10; CONFIRMED in the exe).**
  - **Where:** the faction DB record builder `0x00F74CB0`. `faction +0x514` is the `factions`
    record; `0x00E1C670` is the factions-table getter.
  - **How:** it walks its names group's name list in `names` table order (the group record's list
    from `0x00F43F40`). It pushes each name `weight` times (name record +0x1C) into one of these
    lists:
    - forename (+0x14 = 0), not noble (+0x20 = 0): gender (+0x18) ≠ f → +0xF0; ≠ m → +0x100;
    - forename, noble: → +0x110 / +0x120 by the same gender split;
    - surname (type 1): not noble → +0x130; noble → +0x140;
    - type 2 → +0x150 (no such rows: always empty);
    - weight 0 → a historical-name map instead.
  - The allocators are bound in that order: F0, 100, 110, 120, 130, 140, 150, 19C, 160, ...
    (`0x0087AA92..`). That order is the save order of `NAME_ALLOCATION_DETAILS`.
  - **So `names::pool_rows` is exactly the original's pools 0..6** (table order, consecutive
    weight repeats; CONFIRMED by the code and by every stored size). The "pool order" question is
    closed.
- **Still open: who consumes the draws.** In the user's Peninsula game spa_spain drew 8 forenames
  and 8 surnames over 3 turns, and 8 new unit officers appeared. But none of the drawn names
  (e.g. Fortunato, Leoncio, Maldonado) appears anywhere in the later save. The officers carry
  other names, and the one new colonel shares its unit's officer name.
  - **So:** officer names do not come straight from these draws. Either they come from another
    path, or the draws name something that is not saved (candidate characters?) (UNKNOWN).
  - **Leads** (the callers of the draw wrappers `0x008AA7E0` / `0x008AA220`):
    - `0x008B7340`, `0x008B7070`, `0x008B75C0`, `0x009940A0`, `0x0098F250`, `0x008DB860`,
      `0x0088B850`, `0x008DAAB0`;
    - `0x008AA5E0`: peeks the next index and looks up faction +0x6F4 first.
  - **Until decoded:** new characters and unit officers keep the template's names (PROVISIONAL),
    and the allocator states are written back as loaded.
- **Draw callers (2026-10-04, time-boxed 45 min; CONFIRMED code paths, the rest open).**
  - **Naming routine `0x009940A0`** `(allocators, faction record, world, forename, surname, keep,
    ...)`. It keeps a given name when it is non-empty and unique. Otherwise it picks again until the
    name is unique (`0x00A28CC0`, at most 1000 tries), using one of two sources:
    - when allocators are given: the faction's allocators (`0x008AA220` forename, `0x008AA7E0`
      surname, i.e. the decks);
    - when only a faction record is given: a random index into its pools +0xF0 (forenames) and
      +0x130 (surnames) from the world's random state (world +0xFB8, MS LCG, `0x008AA290`). Those
      picks do not touch the decks.
  - **Callers:**
    - `0x008DAAB0`: a new agent (from `0x008E1C20`, `AGENT_RECORD`). It draws forename and surname
      from the decks, then calls `0x009940A0` with the allocators.
    - `0x008B7340` / `0x008B7070`: create a character of 0x550 bytes. They draw from the decks, then
      set the names through `0x00878CD0`, which calls `0x009940A0` with only the faction record.
      Their birth year is `now − 25 − rand(0..30)`.
    - Other callers of `0x00878CD0` (`0x0088B850`, `0x008B75C0`, `0x008DB860`, `0x00A72010`,
      `0x00B4F090`, `0x00A241E0`, `0x00A8C470`) pass names they made themselves.
  - **Why the Peninsula draws are missing from the save: still open.** Those draws are probably
    spent on characters that left the save again (e.g. colonels merged away) or on candidates. It
    is also open which path names unit officers. Reproducing the original's names exactly would in
    any case need its random stream (world +0xFB8 = `RandSeed`) and its event order, which our turn
    does not follow.
  - **Decision:** names stay PROVISIONAL (template).
  - **If wired later:** draw forename = pool0[deck], surname = pool4[deck] with the uniqueness
    retry, and write the decks back. That gives names the original could give, not the same ones.

## 15. CHARACTER v14 string (2026-10-04)
v14 = v12 + {utf16, bool}. The string is set for 4-5 famous generals only (Gen_Napoleon / Gen_Late_
Napoleon, Gen_Duke_of_Wellington, Gen_Gerhard_Blucher, Gen_Mikhail_Kutuzov, Gen_Archduke_Charles), equal
to their bodyguard unit key when they have it; other generals with unique units (Ney, Moore,
Bennigsen, ...) have "". Its source is UNKNOWN (not in HISTORICAL_CHARACTER_MANAGER). **v12 stays**:
the original loads v12 (every startpos is v12 and NR-1/NR-4 loaded), so the upgrade is not required;
writing a wrong string for a famous general would be worse than keeping v12.

## 16. Embarked armies (2026-10-04)
Storage CONFIRMED by the ports worker (loaders `0x00870FD0` ARMY / `0x008822F0` NAVY, writer
`0x008FAB60`): ARMY #7 (u32) = the carrying navy, NAVY #4 (u32) = the carried army, 0 = none. The
writer sets both from `World::embarked` and 0 elsewhere; `save_check` requires the two ends to agree
(every original save passes; the byte-exact round trip of every original save still holds). Test
`embarked_armies_are_linked`. Where the original saves the embarked commander's position is
UNKNOWN: we keep him at the navy's position (PROVISIONAL). The loader's `transport_link` read comes
with the ports worker's merge; until then our loader still finds embarked armies by position.

## 17. Round-4 results, the campaign victory, captured regions and round 5 (2026-10-04)
**User results (vanilla game):** NR-4 loads, waits for the player, 3 End Turns
work (human flag CONFIRMED). NR-B1 crashes on loading (version-12 AI rebuild path, as predicted,
§5). NR-B2 and NR-B3 load and play 3+ End Turns, but show "Supreme Victory" and "end the campaign
or continue?" right after loading. NR-5 / NR-6 crash at ~25% of the loading bar. New evidence
(kept in `target/evidence`, out of git): `auto_nr4_t4.save` (the original's auto_save after 3
End Turns of NR-4, copied before it was overwritten) and `auto_b2b3_0211.save` (its auto_save
after playing NR-B2/B3).

**Commanders without an obstacle are accepted (CONFIRMED).** NR-B2 (2 such commanders) loads and
plays, and `auto_b2b3_0211` still has both without an obstacle after 3+ End Turns, so the
original does not re-create them on its own (the §6 "refresh re-creates them" holds only when a
character moves). `save_check` reports them as information only.

**The campaign victory (CONFIRMED in the exe).** `CAMPAIGN_VICTORY_CONDITIONS` v5 (reader
`0x00903DA0`): #0 regions to hold, #1 met (+0x31), #2 u32 (+0x98, 24), #3 deadline, #4 region
count (+0x44), #5 bool (+0x48), #6 type (+0x7C, 6 = none), #7/#8 bools (+0x30, +0x32 failed),
#9 u32, #10 date. The test `0x0096FCE0` runs for the human (faction +0x6E0, caller `0x008F3DF0`)
on the faction-level record. Type 5 is the Peninsular campaign's rule: it looks up the faction
`spa_france`, and when there is none, counts the conditions as met if every listed region is
held, which an empty list always is. Every start position holds type 5 with no regions for every
faction; the front end copies the chosen option from `CAMPAIGN_PREOPEN_MAP_INFO/
VICTORY_CONDITION_OPTIONS` (for each playable faction a `VICTORY_CONDITIONS_BLOCK[]` of three
options; for France in eur_napoleon: type 1 = 5 regions plus 35 in all by 1812, type 4 = 18,
type 3 = 60 regions incl. France by 1812) into the human's setup entry, its
`FACTION/CAMPAIGN_PLAYER_SETUP` and its `FACTION` record. (The modded Coalition saves also had Britain's
option in all three places.) Our writer dropped `CAMPAIGN_PREOPEN_MAP_INFO` and kept type 5, so
the original declared victory as soon as it ran the test: at load in NR-B2/B3. In NR-4 it fired
during the End Turns (`auto_nr4_t4` has France's record marked met). **Fix:** the writer gives
the human the first option (`victory.rs`). CONFIRMED by the user's reference save `ORIG france
turn 1.save` (evidence `orig_fr_t1.save`): a new **spa_napoleon** (Peninsular, 1811) campaign as
`spa_france`, written by the original at turn 1, holds option 0 of the start position in exactly
the three records (`victory_pick`); every other faction keeps the start position's default.
(That save is 6.8 MB because the Peninsular map is small, not because of a different format.) `save_check` rule: the human's tested record must not be type 5 without `spa_france`.

**Captured regions (CONFIRMED pattern; NOT the crash: NR-C8b loads, §18).** In the original's saves every
`GARRISON_RESIDENCE` in a region (settlement and slots: ports, forts, towns, road) names the
region's owner, apart from at most one slot (3 cases, 1 each). Our NR-5 / NR-6 had captured
regions (Alsace-Lorraine taken from France, Baden-Württemberg, Bavaria) where only the
settlement's residence changed hands: 5-7 slot residences per region still named the old owner.
NR-4/B2/B3 have no capture, and they load; NR-5/NR-6 have captures, and they crash. **Fix:** the
writer hands every residence of the old owner in a captured region to the new owner;
`save_check` rule: at most one residence in a region not of its owner. Also seen: on capture the
original clears the region's recruitment queues and leaves the buildings' faction keys as they
were (the builder).

**Other checks of NR-5 against the original (nothing found):** the unit and building keys exist in
the vanilla tables; no dangling id of a removed character, force or unit; recruitment items with
#11 = 0 and constructions with #2 = 0 also occur in the original's saves; the calendar fields
follow the original's rule (month = turn in year / 2, half = 0 or 2). Our NR files keep
`CHARACTER` v12 / `SETTLEMENT` v2 (the original writes v14 / v3; NR-4 loads with them). Moved
characters keep their old obstacle with its core cells, where the original's moved obstacles have
#4 = 0, core cells 0 and an empty boundary slot 1 (not a crash: NR-B2 and NR-4 have none, but a
difference to keep in mind).

**Round-5 test saves** (`nr_test_saves -- DIR round5`; all pass `save_check`, re-serialise
identically and reload; NR-C8b breaks the region rule on purpose):

| file | content | tests |
|---|---|---|
| `NR-4 France turn 1.save` | turn 1, nothing done (replaces the old NR-4) | the victory fix: no victory screen, End Turns still fine |
| `NR-B3 France turn 1 garrison unit.save` | NR-4 + one unit in a garrison (replaces the old one) | no victory screen at load (was shown) |
| `NR-7 France AI turns.save` | 5 End Turns (turn 6; one capture: Bavaria to Austria) | loads (NR-5 crashed) |
| `NR-8 France recruit merge build.save` | recruit, build, merge, move, 3 End Turns | loads (NR-6 crashed) |
| `NR-C1 removals.save` | turn 1 + NR-7's removals (15 forces, 1 character) | bisect |
| `NR-C2 moves.save` | turn 1 + NR-7's field moves (3 characters) | bisect |
| `NR-C3 buildings and queues.save` | turn 1 + NR-7's buildings, construction, recruitment queues (51 regions) | bisect |
| `NR-C4 factions and diplomacy.save` | turn 1 + treasuries, taxes, stances, trade, bankruptcy | bisect |
| `NR-C5 new forces and units.save` | turn 1 + NR-7's new forces (4) and units (69) | bisect |
| `NR-C6 unit strengths.save` | turn 1 + NR-7's unit strengths (8) | bisect |
| `NR-C7 calendar.save` | turn 1 with NR-7's calendar (turn 6) | bisect |
| `NR-C8 captured regions.save` | turn 1 + NR-7's capture, written with the fix | loads if the fix is right |
| `NR-C8b captured regions old slots.save` | NR-C8 with the slots left with the old owner (the old writer) | crashes if the capture is the cause |

(NR-7's script values equal turn 1's, so there is no save for them.)
- **Obstacles for new commanders (decision, 2026-10-04 04:00):** none are written for a new
  commander at a new position.
  - `zoc::obstacle_record` gives the record's scalar fields, but not field #0. That field holds
    the boundary slots, whose values are run-time piece ids, plus the pieces' storage:
    `OBSTACLE_BOUNDARIES` (5-u32 entries), the grid's #1 pool (u32 data) and the grid-node keys.
    None of these are decoded (PATHFINDING_PORTS.md §10.4).
  - The grid rules of §19 tie every piece to rows and to the manager, so a guessed obstacle
    risks another load crash.
  - A commander without an obstacle is CONFIRMED harmless (§17).
  - What is written: a new commander standing exactly where a source obstacle stands (a
    garrison) gets a copy or a rename of that obstacle. That path passes every §18/§19 rule
    (NR-C5).
## 18. The pathfinder crash 0x00B1D3E0 found in the crash dumps (2026-10-04, night)
**Evidence.** Windows keeps the original's crashes: event 1000 in the Application log and the
minidumps in `%LOCALAPPDATA%\CrashDumps` (copied to `target/tmp/dumps`, out of git). Every crash of
NR-2/3 (round 1), NR-5/6 and NR-7/8 is at fault offset 0x71D3E0 (VA 0x00B1D3E0; the exe loads at
0x4E0000 under ASLR, so dump addresses are Ghidra VA + 0xE0000); NR-B1 is at 0x79421E. `dump_stack`
(new example: minidump reader, stack scan for return addresses after CALLs, checked against the
exe's bytes) gives the same chain in every 0x71D3E0 dump:
`0x00AF89C0` (the pathfinder loader; its last step) → `0x00B78C60` (re-registers the grid's
obstacle boundaries from the grid nodes) → `0x00B07780` → `0x00B07550` (walks a linked list of
boundary pieces) → `0x00B1D3E0` (`MOV ECX,[ECX]`). The list node's next pointer is 0x3F800000 (a
float 1.0): the walk has left the list, i.e. the grid data the loader built is inconsistent.
NR-B1's 0x79421E is `MOV [EAX+0x84],ESI` with EAX = 0 in `0x00B94170`, called from the AI-block
loader `0x00872550`: the null director pool of the version-12 rebuild path (§5), CONFIRMED.

**What our writer did to the grid that the original never does (CONFIRMED in every original
save, `save_audit grid_lists / grid_items / grid_pair_stats`, new `save_check` rules):**
- `PATHFINDING_GRID[0]` = {u32 7201, u32[] ..., `OBSTACLE_BOUNDARY_MANAGER`, `OBSTACLE_BOUNDARIES`,
  `OBSTACLE_BASE_GRID_NODE[]`, u32[] cell map, `OBSTACLE_LISTS`, ...}. A grid node is {u32 key,
  u32, list 1 [{u32 piece, u32 x, bool, pairs}], list 2 [{bool, pairs}]}; the two lists are
  parallel row by row (equal lengths everywhere). The u32 array after the nodes is a cell → node
  map, (cell x | y << 16, node index), a permutation of the node indexes.
- The original never writes a node with empty lists: when an obstacle goes, nodes left empty
  are dropped with their cell-map entries (its saves after removals have fewer nodes). Our writer
  kept them: 1 873 empty nodes in NR-5, 1 810 in NR-7, 1 979 in NR-C1.
- The list-1 row field x (0..3; 0 in ~60% of rows) is not a pair count, but `grid_item` overwrote
  it with the pair count in every row of every node whenever the obstacle plan was not empty
  (NR-5..8, NR-C1, NR-C5: no x = 0 left at all).
NR-4, NR-B2, NR-B3, NR-C8 had an empty plan (no removals or copies), so neither happened, and they
load. **Fix:** rows are filtered row by row in both lists (a row goes when both sides have no pair
left), no other field is touched, empty nodes go with their cell-map entries and the map is
renumbered (`obstacles::grid_node`, `drop_empty_grid_nodes`); test
`removed_obstacles_leave_a_valid_grid`. INFERRED as the crash cause (the crash is in the code
that rebuilds from these nodes); the user's NR-7 / NR-8 test confirms. Prediction for the
pending bisect results: NR-C1 (removals: both faults) and NR-C5 (copies: x overwritten only)
crash, C2/C3/C4/C6/C7 load.
- PROVISIONAL: a row whose pairs on one side all belonged to removed obstacles is kept with an empty
  pair array on that side (952 rows in the new NR-7; the original recuts the pieces instead and
  never has such a row). If NR-7 still crashes, this is the next suspect.

**Moved characters' obstacles (CONFIRMED pattern).** The original leaves a moved character's
obstacle with `BOUNDARIES` slot 1 (the core pieces) empty, #4 = 0, the core cell range #15..#18
= 0 and no grid pair for slot 1; the zone (slot 0, box, zone cells, grid pairs) and the obstacle's
own `MANAGED_OBSTACLE_BOUNDARY` slot 1 with its boundary-manager entry stay (13/13 such
obstacles in `auto_nr4_t4`, 8/8 in the Peninsula save). The writer now
does the same for every kept obstacle whose character moved (`ObstaclePlan::clear_core`).

**Rebel armies are mirrored by the AI block (CONFIRMED).** The "8 missing characters, 2 missing
forces" of `France Early May 1811` (the user's Peninsula reference after 3 End Turns) are the
rebels' 8 armies: the original mirrors a rebel army in full (commander, force, units); a rebel
character without a force has no mirror (seen only in a modded save; INFERRED). Nothing stale. The
checker's view and the writer's AI view now include rebel armies; before, writing back an original
save with rebel armies would have removed their mirrors (over-removal).

**Other findings this round.**
- A new campaign's AI factions lose their `CAMPAIGN_MISSION_MANAGER` and `CAMPAIGN_SHROUD` (shroud
  flag false) in the original's own turn-1 save; only the human keeps them. The writer now does
  the same for a save written from a start position (fidelity; the original also loads both forms).
- The original's turn-1 save puts the human first in `FACTION_ARRAY` (spa_france, third in the
  start position); the setup's players keep the start position's order. For 0-B: the turn order
  probably starts with the human. eur_napoleon already has France first.
- An original save can hold a settlement mirror still at 0 for a garrison that has entered (its
  auto_save after our NR-C8); the check accepts that. Region residences of the old owner after a
  capture (NR-C8b) load and stay; reported only (`Report::regions_with_foreign_residences`).

**Round-6 test saves** (written as new files in the empty save folder; nothing restored):
`NR-7 France AI turns.save` (5 End Turns, turn 6) and `NR-8 France recruit merge build.save`. All
§17/§18 rules hold; 0 empty grid nodes; row fields as in the source.
## 19. Round 6 still crashed: 0x00AECFE6, the grid rows and the boundary manager (2026-10-04, 03:30)
**Result.** The round-6 NR-7 / NR-8 crash later in loading (the loading screen shows, ~5 s), at a
new fault offset 0x6ECFE6 (VA 0x00AECFE6): the 0x00B1D3E0 fix worked and the loader got further.
The two dumps give the same chain (dump address − 0xE0000 = Ghidra VA): the pathfinder loader
`0x00AF89C0` → `0x00AEE2E0` (reads one grid node: u32, u32, then per list-1 row u32 piece, u32 x,
and the row's pair list) → `0x00AECFF0` (reads the bool and the pair list) → `0x00B53800` (looks the
list up by value in a hash map built from the `OBSTACLE_BOUNDARY_MANAGER`) → `0x00AECFE0`
(`INC [EAX+0x14]`, a reference count, with EAX = 8: the lookup found nothing and returned 0 + 8).
**So every grid row's pair list must be a list of the boundary manager.**

**What the original's saves show (CONFIRMED in all 7 vanilla originals, new `save_check` rules):**
- The manager is a set: distinct, non-empty pair lists; every grid-row list (both lists) is one of
  them; unused lists can stay.
- List 2 of a grid node is not parallel to list 1: it holds the same pair lists in another order
  (a permutation in every node of every save, `save_audit list_perm`).
- A list-1 row of piece P names exactly the (owner, slot) obstacles whose `BOUNDARIES` list P
  (`piece_refs`); the obstacles' managed slots are exactly the manager's (owner, slot) pairs, and
  their non-empty `BOUNDARIES` slots exactly the grid's (`obstacle_consistency`).
- A moved character's obstacle keeps only its zone: grid pairs of slot 0 only, managed slots 0 and
  1 (the manager keeps one [owner | 2, 1] list), slots 2.. off and empty (13/13, 8/8).

**What round 6 did wrong:** it treated the two lists as parallel and kept a row while either side
had pairs, which left 952 empty pair lists (not in the manager: the crash), and the manager had
18 repeated lists after filtering. **Fix (`obstacles.rs`):** each list is filtered on its own (a row
goes when its list is empty; the lists stay permutations); the manager is deduplicated and gets
any list a row needs (`sync_manager`); moved obstacles are put in the original's state (slots 2..
off, only the zone left in the grid). The new NR-9 / NR-10 pass every rule above, the §18 rules
and every `save_audit` grid audit, like the original's own saves.

**Re-check of the AI-block maintenance on vanilla pairs** (it was learnt from the modded
Coalition sequence): `cai_audit simulate` on NR-4 → auto_nr4_t4, the Peninsula turn 1 → turn 4,
and NR-B2 → its auto_save: 0 components removed that the original kept, every AI-block invariant
holds after removal. One site not in the table (`CAI_DEFEND_REGION_COAST_ANALYSIS_POI` #0, 47
references) is "tolerated" in the vanilla pairs (the original keeps the dangling references),
which is what the writer does with unknown sites; added to the table. A site table learnt from the
vanilla pairs alone labels ~20 sites differently (entry / subject / tolerated), from far fewer
events; the table stays as it is (PROVISIONAL labels; safety CONFIRMED on vanilla: no
over-removal, invariants hold). Other rules first seen in modded saves: the moved-obstacle state
and the rebels' mirrors are now CONFIRMED on vanilla saves; the human flag, the victory option,
the AI manager types were CONFIRMED on vanilla saves already; the names pool rule (§14) fits the
vanilla saves the same way (163-168 of 168 per pool, the misses the stale eur startpos sizes).

**Round-7 test saves** (new files; the user's folder had only our round-6 NR-7 / NR-8, which are
obsolete): `NR-9 France AI turns.save` (= NR-7, 5 End Turns) and `NR-10 France recruit merge
build.save` (= NR-8).
## 20. Regiment and ship names of new units (2026-10-04, morning)
- **Rules (vanilla evidence, `regiment_names` example).**
  - The land list's class number is the row index of the `unit_class` table (CONFIRMED).
  - Each list holds that class's `unit_regiment_names` rows of order ≥ 1. The trailing name is the
    order −1 row.
  - Ships have one list per faction.
  - A list's in-use flags are exactly the names the faction's units carry (CONFIRMED: every vanilla
    original save and start position; no flag without a unit, no carried name without its flag).
  - A new unit takes the lowest free name of its class (INFERRED, ~99% of the flag changes between
    consecutive saves). Without a free name it takes the trailing name. With no list and no trailing
    name it gets no name, ("", "") (INFERRED: skirmishers, generals).
- **Writer (`regiments.rs`).** After the units are written, every new unit (an id not in the source)
  gets a name:
  - the lowest list entry not carried by another unit of its faction (land: by its `units`
    `unit_class`; ships: the naval list);
  - its `UNIT` #14 becomes a copy of that list entry's `CAMPAIGN_LOCALISATION`.

  Then every flag is set from the names carried, so the names of units that are gone are freed.
- **Effect.** A loaded original save is written back unchanged (its flags already match). Our
  earlier saves gave new units the template's name, which a later original auto_save kept as a
  duplicate (`auto_b2b3_0211`).
- **Checks.** `save_check` rule and test `new_units_get_regiment_names`. The round-10 copies in
  `target/tmp/round10` pass. Not in the user's folder: NR-9 / NR-10 are waiting for the morning
  test.
- **Character and officer names** (CHARACTER_DETAILS #1/#2, `UNIT/COMMANDER_DETAILS`): wired in,
  see §21.

## 21. Names of new characters and unit officers, wired in (2026-10-04, morning)
- **Algorithm (CONFIRMED in the exe), `charnames.rs`:** a new character or a new unit's officer
  takes a forename from the faction's allocator 0 and a surname from allocator 4 (deck draws,
  `0x008A9FA0`; the pools from the builder `0x00F74CB0`, `names::pool_rows`). The naming routine
  `0x009940A0` keeps the pair unless "forename surname" is a historical character's on-screen name
  (`0x00A28CC0`). Then it picks again, up to 1000 times, with the world's random state (world
  +0xFB8, saved as `RandSeed` #0): one LCG step per name, and `0x008AA290` takes the key from the
  second derived step and the display text from the first (the original's quirk, kept). The
  callers are agents `0x008DAAB0`, characters `0x008B7340` / `0x008B7070`, and the record-only
  path `0x00878CD0`.
- **Writer:** `save::write_save_named(..., Some(&NameData))`. `NameData::load` reads the names
  table, the faction groups and the localised historical names. It names each new CHARACTER
  (CHARACTER_DETAILS #1/#2), then each new UNIT's officer (`COMMANDER_DETAILS` #0/#1); a unit whose
  #10 is a new colonel / captain shares his name (as in the original's saves). It writes the
  advanced allocators 0 and 4 back (size, seed, deck) and, after a re-pick, the new `RandSeed`.
  An allocator whose stored size is not its pool's (stale startpos sizes) starts with the pool's
  size and an empty deck (INFERRED from the loader `0x0085EF80`).
- **Stream: PROVISIONAL.** The names follow the original's rules, but exact names would need the
  original's random stream and event order (our game draws at save time, the original at spawn
  time, and other systems also use world +0xFB8).
- **Used by:** the game's quick save (`CampaignSim::names`, loaded at campaign start) and
  `nr_test_saves`. `write_save` / `write_save_with` keep the old behaviour (no `NameData`:
  template names).
- **Tests:** `new_characters_and_officers_get_names` (save_compat). It covers names from the
  faction's pools, the colonel's unit sharing his name, distinct officers, allocators equal to the
  original's state after the same draws, a byte round-trip, and a clean `save_check`. Also
  `charnames::tests` (the historical re-pick uses the world state; with no clash the state is
  untouched).


## 22. Obstacles for new commanders: `grid_obstacle::add_character_obstacle` trial (2026-10-04, morning)
**Decision: keep "no obstacle" for new commanders for now.** The evidence (`obstacle_trial`,
read-only on the install; the outputs are in `target/tmp/obstacle_trial` only):
- **Rules:** with ours added to the round-10 copies of NR-9 (5 commanders) and NR-10 (9), every
  rule but one holds, before and after a write and read. The exception is NR-10: 2 run-time pieces
  (7219, 7287, the same 3 points: a sliver of near-collinear points, doubled area -1 515 703 in
  Fixed20²) are wound clockwise. The original never writes that: across all 11 vanilla saves kept
  (~70 000 pieces), every used piece has 3+ points and is counter-clockwise, and only freed pieces
  (use count 0) have fewer than 3 points (CONFIRMED). This is now a `save_check` rule, so the
  NR-10 trial fails it.
- **Matching the original's own obstacles** (`obstacle_trial compare`): our obstacle was rebuilt
  for every full obstacle of the original's saves, at the character's position.
  - Same cells and rings: 57/82 (`auto_nr4_t4`), 68/95 (`auto_b2b3_0211`), 49/60 (May 1811),
    54/59 (Peninsula turn 1).
  - Same polygon kinds in ~80 % of the cut versions.
  - The piece outlines are PROVISIONAL (not compared).
- **Combined versions** (versions for several obstacles on one cell) are no reason against it. The
  original keeps one on only 25-40 % of the cells that two or more obstacles cover (`obstacle_trial
  combined`: 959 / 3155, 326 / 1962, 1113 / 3712, 275 / 1733). In NR-10, 6 of our 9 obstacles share
  cells with other obstacles.
- **"No obstacle" is CONFIRMED harmless** (§17: NR-B2 loads and plays 3+ End Turns; the original
  makes a fresh obstacle when the character moves). Ours is not yet as safe:
  - the inverted sliver pieces above;
  - about 25 % of zones and 20 % of cut kinds differ;
  - two earlier pathfinder crashes (§18, §19) came from grid data that broke rules nobody knew yet.
- **To switch later:**
  - the ports worker makes the cut never emit a clockwise or degenerate piece (drop slivers, or
    orient them counter-clockwise);
  - the NR-10 trial then passes every rule;
  - the user loads one trial save (NR-10 plus obstacles) and plays End Turns with the commanders
    moving, as a new NR-n file.
- **Update (2026-10-04, after the ports worker's sliver fix 9bda7e3):** with the fixed writer,
  `obstacle_trial add` gives every new commander an obstacle with 0 new violations and 0 bad pieces
  (no clockwise, flat or under-3-point used piece) on all four saves tried: the round-10 NR-7 (5
  commanders), NR-8 (9), NR-C5 (5) and the 26-turn NR-Y (11). Each also writes, reads back and
  reloads. The match with the original's own obstacles is unchanged (57/82 and 54/59 same cells,
  ~80 % same kinds); the piece outlines stay PROVISIONAL.
  - **Decision: yes, as a candidate, confirmed by the original before it becomes the default.**
    The writer keeps "no obstacle" (CONFIRMED harmless) until the user's test.
  - **Trial save prepared:** `target/tmp/nr11/NR-11 France AI turns obstacles.save` = the user's
    NR-9, read only, plus obstacles for its 5 new commanders and nothing else. If NR-9 loads, a
    crash in NR-11 is the obstacles. Its only rule breaks are NR-9's own: 13 regiment-name flag
    entries, written before §20. It is **not** in the user's folder yet. It goes there as a NEW
    file only once NR-9 / NR-10 are confirmed loading. The user's test: load it and play 3+ End
    Turns, moving those armies.


## 23. Characters in play: traits, ancillaries, natural deaths, recruitment pools (2026-10-04)
This section covers the work for the 0-G characters slot (`work/fidelity-characters`, merged into this branch).
- **Traits and ancillaries for every character** (`save::write_traits`). `CHARACTER_DETAILS` #0
  `TRAITS/TRAIT[]` {utf16 key, i32 points} and #11 `AgentAncillaries[]` {utf16 key} are written from
  `World::character_details` for every character in the model, in the model's order. The layouts are
  CONFIRMED in every original save: TRAIT v1 and AgentAncillaries v0. A new character without details
  keeps the emptied template lists.
- **Deaths.** A dead character is no longer in the model, so the existing removal cascade drops him
  (§7, §10): the CHARACTER_ARRAY item, AI mirrors, force / residence / pool / raid references, and his
  obstacle. His post goes vacant in the model.
  - The one gap was a force whose commander died. The 0-G pass left it without a commander, but
    every original save has a commander for every force (`save_check`), and the first 26-turn run
    broke 4 rules on it.
  - `CampaignModel::character_dies` now hands such a force to its successor in the exe's order
    (0-G, `command_vacated` / `commander_unit`, from `0x008B8E50` → `0x008ED2C0`, CONFIRMED): the
    force's units are sorted (General or admiral first, then rank, category, the unit flags), and
    the first unit's own character takes command if it has one (an existing General or colonel
    riding with the army). Otherwise a new colonel (a captain at sea) joins that unit. The
    successor stands where the dead man stood. A force left without units is removed.
  - **An existing successor** keeps his own unit. The writer points his `CHARACTER` #4 at the
    force and `MILITARY_FORCE` #1 at him (both from the model on every write), keeps #5 on his
    unit, and the dead man's references go through the cascade. He has no obstacle until he moves,
    as for any new commander (§22). Test `an_existing_character_takes_over_a_dead_commanders_force`.
    New `save_check` rule (CONFIRMED on all 11 vanilla saves kept): a force's commander rides on
    one of the force's own units (his #5 is a unit of that force whose #10 is him).
  - The save writer gives a new colonel the name of the unit officer he takes over
    (`charnames`), as the original's colonels share their unit officer's name (§14, §21).
  - Exe lead: the unit constructor `0x0088B850` names its officer either from the decks plus the
    record-only re-pick (`0x00878CD0`), or from a name it is given. So unit officers are named on
    that path (CONFIRMED). The original's promotion path itself was not traced.
- **Recruitment pools** (`save::write_pools`). FACTION #75 `GENERAL_RECRUITMENT` /
  `ADMIRAL_RECRUITMENT` {u32[] ids, u32 timer} are written from `FactionDetails::general_pool` /
  `admiral_pool`; a candidate who dies leaves the pool (`character_dies`). `drop_dangling_refs` still
  keeps only living characters without a force (CONFIRMED rule).
- **New `save_check` rules** (CONFIRMED on all 11 vanilla saves kept; none fires on them): per
  character, each trait key once, keys non-empty, trait points > 0; each ancillary once, non-empty,
  at most 3 (`max_ancillaries`).
- **Checks.**
  - Tests: `character_changes_and_deaths_are_written` (save_compat) and
    `a_dead_commanders_force_goes_to_a_new_colonel` (ntw_sim). The first changes traits, points and
    ancillaries, kills a commander and a pool candidate, then writes and reads the save back. It
    checks: every character's traits and ancillaries read back equal, no reference to the dead
    remains, the colonel commands from the first unit under its officer's name, the pools equal the
    model's, and `save_check` is clean.
  - `nr_test_saves <dir> years 26` (France, eur_napoleon): 26 End Turns across the 1805 year end,
    with natural deaths. Before the fix: 4 violations from the commanderless force. Now: 0 problems,
    the save reloads, and 0 characters read back with different traits or ancillaries. The save is
    in `target/tmp/years` only, not in the user's folder.
  - The model keeps details of 13 characters that other removal paths took out of the world
    (battles, merges). They are not saved (the writer saves only characters in the world). Other
    slots may want to drop them.


## 24. Technology research (2026-10-04, for 0-B round 7)
- `save::write_technologies` writes `FACTION_TECHNOLOGY_MANAGER` `techs[]` {utf16 key, u32 state,
  f32 progress, u32 researcher, u32[], u32} from the model, by key. The fields are #1 =
  `FactionDetails::technologies` state (0 researched, 2 available, 4 not yet), #2 / #3 =
  `FactionDetails::research` progress and the school's `REGION_SLOT` id (0 / 0 without an entry).
  Layout CONFIRMED (CAMPAIGN_FIDELITY.md §Research). The model's research code itself is 0-B's.
- Technologies the model does not list keep their stored values. A value equal to the stored one is
  not rewritten, so an unchanged save writes back byte for byte: `user_saves_round_trip_byte_exact` (test since removed)
  run on 4 vanilla saves gives all identical (`NTW_SAVE_DIR` = copies in `target/tmp/bytecheck`).
- Test `technology_research_is_written`: an unchanged model leaves every technology record as
  stored. France with one tech researched (state 0, progress = cost) and one under way at a school
  reads back equal, only France's record changes, and `save_check` is clean.

- **Review of the 0-G save.rs changes (merged 9cd1f40):**
  - Kept: minister templates, `CHARACTER` #8 and `CHARACTER_POST` #2 from the model,
    `write_family`, model-named new characters skipped by the namer, `write_new_details` birth
    dates.
  - Checked: 4 vanilla saves still write back byte for byte, the 26-turn NR-Y has 0 problems, and
    every test passes.
  - New `save_check` rule (CONFIRMED on all 11 vanilla saves kept and on our round-10 saves): a
    post and its holder point at each other. `CHARACTER_POST` #2 = holder, his `CHARACTER` #8 = the
    post id, and a character with #8 != 0 holds that post.
  - New ministers keep the template's portrait (PROVISIONAL, as 0-G notes).


## 25. The save header's territory picture (2026-10-04)
- `SAVE_GAME_HEADER/MAPS[]` holds one item per theatre: {utf16 theatre key, u32 width, u32 height,
  i32 pitch = width × 4, u32[] pixels 0xAARRGGBB}. The front end's Load Game page shows it. Our
  writer used to copy the start position's picture, so the page showed France's 1805 lands after any
  conquest.
- **The original's rule** (`header_map_probe`, `header_map_check`): the picture is the theatre's
  `campaign_maps/<map>/<x>_map.tga`. Each pixel belongs to the region whose `regions` DB colour is
  its colour in `<x>_lookup.tga`.
  - Pixels of the save faction's own regions: R and B = `⌊91 b / 256⌋`, G =
    `⌊⌊(91 b + 165 × 255) / 256⌋ × 255 / 256⌋` (a green tint).
  - All other pixels: `⌊255 b / 256⌋`.
  - Alpha kept (0xFF).
  - Reproduced pixel for pixel from each save's own region owners on all 14 original saves kept
    (europe_main and spain_main) and on the eur startpos.
  - Allies and protectorates are not tinted (none is, in those saves).
  - INFERRED: the original builds the picture from the owners at save time. None of its saves has
    the player's lands changed, so this is not tested on the original.
  - The exe's builder (`GenerateRegionOwnershipMaps`) was not traced.
- **Code:** `ntw_campaign::header_map`: `TheatrePictures::load`, `render`, and `update_maps(tree,
  model, human, pictures)`, run after `write_save_named`. Callers:
  - the game's quick save (`CampaignSim::pictures`, loaded at campaign start for the header's
    theatres);
  - `nr_test_saves`.
  Unchanged owners leave the picture as stored.
- **Test** `header_territory_map_follows_the_owners`: France takes Bavaria and loses Corsica. Bavaria's
  pixels turn green, Corsica's plain, and every other pixel is unchanged.

## 26. Governorship tax levels (2026-10-04, for 0-B round 7)
- `write_taxes` writes each governorship's `GOVERNORSHIP_TAXES` {u32 lower, u32 upper, u8 lower rate,
  u8 upper rate} from the model's own governorship (`FactionDetails::posts`, matched by post id),
  which `SetTaxLevel` now updates. A governorship the model lacks gets the faction's levels (the
  previous rule).
- Tests:
  - `governorship_tax_levels_are_written`: both classes, index and rate read back; only France's
    record changes; an unchanged model leaves every record as stored.
  - `taxes_stances_and_script_values_are_written` now sets the tax through the command.
- Four vanilla saves are still byte-exact.


## 27. Duel counters (2026-10-04, for 0-G)
- `CHARACTER` #36 / #37 (u32, duels lost / won; positions CONFIRMED by 0-G) are written from
  `CharacterDetails::duels_lost` / `duels_won` for every character.
- Test `duel_counters_are_written`: changed counters read back, only that character's record changes,
  and an unchanged model leaves every counter as stored. Four vanilla saves are still byte-exact.

## 28. Shroud and sight (2026-10-04, for 0-G)
- `FACTION` `CAMPAIGN_SHROUD` trees #0 (explored) and #1 (visible) are written from `World::shrouds`
  with `shroud::encode` (root = the sight grid's). Our encoder writes all 39 trees of the vanilla saves
  kept back node for node (`sight_probe`), and a tree is rewritten only when its cells changed. A faction
  without a shroud in the model, or without the record, is left as it is; #2 and the flag are not touched.
- `CHARACTER` #17 (f32 sight radius) from `World::sight_radius` and #22 (hidden) from
  `CharacterDetails::hidden`, each only when it differs from the stored value (bit for bit).
- `FACTION` `EXPOSED_CHARACTERS` from `FactionDetails::exposed` (items {i32 id}, CONFIRMED layout in
  `orig_fr_may1811`), rewritten only when the list changed.
- Character `LINE_OF_SIGHT` is **not** rebuilt each save in the original, so we leave it as stored:
  `sight_probe` on 6 original saves shows every character whose disc is on matching the disc of his
  saved position and #17 in most saves (59/59, 61/61). In `orig_fr_may1811`, 17 of 58 do not match: they
  are discs of the same size at another position, so the shape is kept from an earlier sight update,
  not rebuilt when saving.
- New `save_check` rule: a `CAMPAIGN_SHROUD` has three trees on one grid. CONFIRMED on the 13 original
  saves kept and on every startpos. "Every visible cell is explored" holds in the saves, but the
  tutorial startpos breaks it (tut_france, cell (406, 175)), so it is not a rule.
- Test `sight_state_is_written`: France's explored and visible cells, a general's radius 22.5,
  hidden flag and exposure read back, and an unchanged shroud stays as stored. The four vanilla saves
  in `target/tmp/bytecheck` are still byte-exact.

## 29. The 0x00B1D3E0 crash: root cause, the loader replay and the fix (2026-10-04, 09:30-11:00)
**Evidence.** The four 09:22 dumps of NR-9 / NR-10 (`Napoleon.exe[(1)].14888.dmp`, `...17868.dmp`; the exe
loaded at 0x690000 this time, so VA = dump address − 0x290000; copies in `target/tmp/dumps`) and the
earlier NR-5..NR-8 dumps all show the same picture at the fault `MOV ECX,[ECX]` (0x00B1D3E0): ECX =
0x3f800008, ESI = 0x3f800000, EDI a garbage value, EAX = 0x800000ea/eb, EBX and EBP small counts
(0x6f9 / 0x2954; 0x63d / 0x291a). Chain: `0x00AF89C0` (the grid loader, last step) → `0x00B78C60` →
`0x00B07780` → `0x00B07550` → fault. Ghidra decompiles and listings of every function on the chain
and of the loader's helpers: `target/tmp/gh/k1o..k5o.txt` (out of git; targets `k1.txt..k5.txt`).

**What the loader does (CONFIRMED in the exe):**
- `OBSTACLE_BOUNDARIES` #0 = the **cell versions**, read by `0x00AEE750` (`n`, `n` × (flags, link), cell
  key, byte-as-u32) into a linked list at grid +0xA4 (every-32nd-node index for access by position).
  Each version's **grid-node pointer (+0x18) starts as `[grid+0x90]`**, the node list's begin, pushed at
  `0x00AF9534` before any node exists: the list's own sentinel.
- `OBSTACLE_BASE_GRID_NODE[]`, one per node (`0x00AEE2E0`): the key = first static boundary index (data
  pointer at +0), the count; per list-1 row: the version by position, `x`, and the pair list looked up
  in the manager's hash map (`0x00AECFF0` → `0x00B53800`; missing = `INC [8+0x14]` = the 0x00AECFE6
  crash of round 6), then **inserted into a map keyed by the pair list** (`0x00B46CC0`: find, then
  insert; a second row with the same list is not inserted). Per list-2 row: the manager lookup; the rows
  become the node's linked list at node+8+0x2c. The node is copied into the grid's node list
  (`0x00B5DD40`, which puts 1.0f at list-node+0x2c and the row list at +0x34), then `0x00B5BF60(node+8,
  node)` sets **the version of every map entry** to point at the node (`0x00B5BFE0`: `[V+0x18] = node`).
  A row whose pair list another row of the node already had is not a map entry: its version keeps the
  sentinel.
- `#5`: (cell key, node index) into the cell hash map, the node by position (every-32nd index).
- `OBSTACLE_LISTS` (`0x00AFA4F0`; `0x00AED950` per `OBSTACLE`): `BOUNDARIES` slot entries resolve by
  position (`e & 0x7fffffff`; the high bit = the ring flag, stored beside the version pointer in
  8-byte items), `#1..#4` → +0x64..+0x70, each true `MANAGED_OBSTACLE_BOUNDARY` list is looked up in the
  manager (same crash when missing). Three obstacle kinds in three hash maps of the obstacle manager
  at grid+0xB4 (barrier: raw key; character: `char+0xCC | 0x80000000`; fort: `+0x158 | 0x40000000`).
- **The last step `0x00B78C60`**: for every obstacle, the items of its `BOUNDARIES` slot **#2** (+0x68)
  give their versions' grid nodes (`[V+0x18]`, unique set); for each node, `0x00B07780` (`this =
  [V+0x18] + 8`) → `0x00B07550` walks the node's list 2 from +0x2c: each row with exactly one pair
  looks the pair's owner obstacle up and keeps the pair when its slot equals the owner's `#1`; a row
  with another pair count ends the walk; the collected list is then looked up / created in the
  manager. **A version still on the sentinel** makes `this` = sentinel + 8 = grid + 0xA0, and `[this +
  0x2c]` = grid + 0xCC = the obstacle manager's first hash map's load factor **1.0f = 0x3f800000** →
  ESI = 0x3f800000, ECX = ESI + 8, fault. EAX = 0x800000ea is the MSVC static-init epoch
  (`0x80000000 + n`) left by the function's thread-safe-static guard, EBX / EBP are `0x00B78C60`'s
  loop index and count (the 1786th of 10580 collected nodes in NR-9): the fault is in the walker's
  first iteration, so the pointer was bad before any walking.

**The discriminator (CONFIRMED on the data; now `save_audit load_check`):** in every vanilla save and
start position **no grid node has two list-1 rows with the same pair list** (and every version has
exactly one row, no version is in two nodes, piece use counts equal the links, every managed slot
list is the obstacle's own single pair). NR-5 / NR-9 / NR-10 have 521 / 458 / 674 such duplicate rows
(and 3030 / 4103 / 3982 versions without a row). Our writer made them by **filtering a removed
obstacle's pair out of combined versions**: a node with versions {A}, {B}, {A,B} became {A}, {A} after B
left. What the original does instead (CONFIRMED: NR-4 → `auto_nr4_t4` and the Peninsula turn 1 → May
1811, hundreds of nodes, e.g. keys 1535-1539: `[{A}, {B}, {A,B}]` → `[{A}]`, key 1540: `[{A}, {B1,A},
{B1}, {B0}, {B0,A}]` → `[{B1}, {B0}]`): **a removed layer takes every cell version naming it with it,
whole.** Round 6's "parallel lists" and round 7's "independent filtering" never touched this; rounds
1-5 crashed here for the same reason (the empty nodes and the `x` overwrite were harmless at load),
round 6 crashed earlier (empty pair lists missing from the manager, §19) and round 7 got back here.

**The loader replay (`ntw_campaign::grid_load_check`, part of `save_check`; `save_audit load_check
FILE...`).** It follows the steps above on `PATHFINDING_GRID[0]` and reports **faults** (the loader
reads a bad pointer: version / node indexes beyond the lists, pair lists missing from the manager,
the sentinel walk, walked rows naming a character without an obstacle) and **rule breaks** (states the
original never writes: a duplicate row list in a node, a version without a row, a version in two
nodes, a piece use count ≠ its links). Results: **0 faults, 0 rule breaks** on every vanilla save kept
(the 4 `bytecheck` copies, the original's `NR-A` / `auto_save`, `auto_nr1`, `auto_b2b3_0211`,
`auto_after_c8`, `orig_over_nr4_0252`, `orig_fr_t1_b`), on all 8 start positions, and on the NR saves
that load (NR-4, NR-B2, NR-B3, NR-C8, NR-C8b, the round-10 NR-4 / NR-B3). **The 0x00B1D3E0 fault on
every crashing save**: NR-2, NR-3, NR-5, NR-6, NR-7, NR-8, NR-9, NR-10 (173-318 sentinel versions each;
NR-2 also the managed-list fault). Unit tests: a consistent grid passes, a repeated list faults, a
missing manager list is the 0x00AECFE6 fault, a wrong use count is a rule break.

**The fix (`obstacles.rs`).** A removed (not renamed) owner's layers and a moved character's slots ≥ 1
are *dropped layers*; every cell version whose pair list names one goes whole: its rows in both
lists, its entry in `OBSTACLE_BOUNDARIES` (the survivors renumbered in the rows and in every
obstacle's `BOUNDARIES`, ring flag kept), one use of each piece it links, and the manager's lists
naming the layer (the cleared core's own `[owner|2, 1]` list is put back by `sync_manager`, as the
original keeps it). Copies and renames work as before. Test `removed_obstacles_leave_a_valid_grid`
(rows matched by pair list, a clean replay, versions = rows) and the emulator's unit tests; the four
vanilla saves still write back byte for byte. The round-8 saves (`nr_test_saves -- target/tmp/round11
turns 5`): 0 violations, replay clean (NR-5: 17005 versions = rows, 10859 nodes, 81 obstacles).

## 30. The 0x00B979C3 crash: a failed load in the AI block (link indexes) (2026-10-04, 10:05-12:30)
**Evidence.** NR-12 / NR-13 (round 8) crashed at VA 0x00B979C3 (dumps `Napoleon.exe[(1)].16828` / `14932`;
base 0x230000, VA = dump − 0x230000 + 0x400000). Chain (CONFIRMED): `0x008B8A60` "Creating campaign env for
loaded game" saw its **load-failed flag** set and destroyed the half-built env (`0x0099C3F0`) → the CAI
destructor `0x00A4F770` deletes the director BDI pool at CAI+0x628 (the indirect call at `0x00A4FAEA`) →
`0x00B9EDB0` → `0x00B979B0` reads `[this+0x8c]`, which still holds the raw save value 4001 (`CDIR_BDI_POOL`
#0, the same in every save), because the fixup that resolves it (director vtable slot 0 `0x00BBC450`,
step "Post load DIRECTOR_BDI_POOL" of the CAI loader `0x00A3CE70`) never ran. So the crash is secondary:
**some loader step between the director read and that fixup returned false.** The dumps hold no heap
pages, so the step was found by replaying the loader's checks on the data.

**The CAI loader's steps after the pathfinder (CONFIRMED, `0x00A3CE70`):** CAI_WORLD (`0x00BF0130`, the
mirror lists with their own readers), the central pool (`0x00C8C9F0`), the managers (`0x00C90720`), #29
pairs (component looked up in the interface's map `0x00C1E040`), #30..#34, #35 (components), #36 (global
ids), EXCLUSION ZONES, PRE_BATTLE_INFORMATION, the director pool (`0x00B8EEE0`: its base pool, then #0
read raw into +0x8c); then the post-loads: the world's fixup A (vtable slot 14 = `0x00C48B60`: per mirror
list, each item's resolve slot), "Load static sea grid", world slot 15 (sea grid cells), the sea-grid
mobiles, static POI data, the central pool's slot 0 (`0x00CFF8F0`: 19 fixed components, then its BDI
pool), each manager's slot 0 (`0x00D00640`: its faction component, its BDI pool), the director's slot 0,
world slot 2 + central slot 1, managers' slot 1, director's slot 1, dword `0x1B6` of `this` (`+0x6D8`), the "last manager"
list, recruitment... Every resolve goes through two lookups: `0x00CC3D50` (a component by id in the
central pool; 0 → the fallback `0x00A68C50` → 0) and `0x0105AC60` (a world object by id in the global
map); a 0 result fails the load, except where the field is tested for 0 first.

**The component block post-load `0x00CFD790` (CONFIRMED) — the failing step.** For every
`CAI_BDI_COMPONENT_PROPERTY_SET` block: the slot list (+8, 0 or a component), the incoming pair list
(+9), the id lists, the `BLOCK_OWNS` items. An incoming pair **(source, v) is resolved to
`source.BLOCK_OWNS[v]`: v is an INDEX into the source's outgoing list, and v ≥ its length fails the
load** (the pair is dropped and 0 returned). The data agrees: in every vanilla save and every loading NR
save, 100 % of pairs satisfy `source.OWNS[v].target == this` (63 711 / 63 711 in `auto_nr4_t4`, 65 686 /
65 686 in NR-4 and NR-B2; the value equals the link's slot number only by coincidence, ~4.5 %). NR-12 /
NR-13: **1 124 / 1 120 pairs beyond their source's list** (the load failure) and ~13 600 pointing at
another target. Cause: `cai::fix_block` treated the value as a slot number; when a removed target's
`BLOCK_OWNS` entry was dropped from a source, every later index in the other targets' pair lists was
off by one per dropped entry.

**Fix (`cai.rs`).** `remove_components` first collects every kept source's dropped `BLOCK_OWNS` indexes
(`owns_targets`), and `fix_block` renumbers each kept pair (`renumbered_pair_index`). New
`check_link_counts` rules (in `save_check` through `check_block`): every pair (source, v) has v < the
source's `BLOCK_OWNS` length and `OWNS[v].target` = this component; every `BLOCK_OWNS` item's second id is
its own component. Unit test `pair_indexes_follow_the_removed_owns_entries`; the save_compat removal
tests pass; 0 findings on 11 vanilla saves.

**The widened emulation and the other findings (all vanilla-only evidence):**
- *Zero census* (scratch tool `target/tmp/gridcheck`, `zeros`): AI-block sites holding 0 in our saves
  where no vanilla save (11, sites with 20+ values) ever has 0: only `CAI_BDIM_WAIT_HERE #2` (37) and
  `CAI_BDI_RECRUIT_GENERAL #0` (1), both `entry → 0` sites of the cascade (learnt from the modded
  sequence; `zero_seen` was 1 there). Their post-loads (`0x00CFD620`, `0x00CFF670`) test for 0 first, so
  they do not fail the load; the original drops the intention with its component, so both sites are
  now `subject` with `zero_seen` 0 in `cai_sites.txt`, and a new `check_block` rule reports a 0 at any
  scalar site whose row says the original never has one (0 hits on the 11 vanilla saves).
- *Shape census* (`shapes`): every record kind's child-type signature of our AI-turn saves occurs in
  a vanilla save, except `CAI_TAS_ANALYSIS` whose variable lists change its signature (layout CONFIRMED
  in §10): nothing the ESF reader would reject.
- *Situated / owner components* (`situated`): every `CAI_SITUATED` region and theatre component and every
  `OWNED_DIRECT` faction component of characters, mobiles, units, settlements and region slots exists
  (the situated post-load `0x00C48580` resolves the region unconditionally when non-zero).
- *Fixup A field maps* (from the readers): `CAI_UNIT` #1 and `CAI_CHARACTER` #3 are world ids resolved
  unconditionally (the existing "every character, force and unit the AI block names exists" rule);
  their other fields and the mobile's are components resolved only when non-zero.
- The 0x00B1D3E0 replay of §29 is unchanged (clean on the new saves).

**Round-9 test saves** (`nr_test_saves -- target/tmp/round13 turns 5`; 0 violations with every rule
above, replay clean, pair indexes 100 %, zero and shape censuses clean): copied as NEW files `NR-14
France AI turns.save` and `NR-15 France recruit merge build.save`.

**Loader probe for cdb** (`target/tmp/loader_probe.cdb.txt`, for the manager's live-attach run): logs every
loader section marker (`0x01095DC0`'s string argument) and breaks only on the failure paths decoded
above (the block post-load's `return 0`s, fixup A's fail label `0x00C492CE`, the BDI-pool and manager
post-load failures, the CAI loader's load-failed flag writes, the env creator's flag test, the two
lookups' misses), so one load shows every failing check in order.

## 31. The zero census was too strict (2026-10-04, 0-E worker)
The user's own vanilla `auto_save.save` (written by the original at 11:10 today, in its save folder)
holds 0 at `CAI_BDIM_WAIT_HERE #2` (six intentions) and `CAI_BDI_RECRUIT_GENERAL #0` (two), so §30's
"the original never leaves these at 0" was an artefact of the 11-save sample, not a rule
(CONFIRMED by that file). Both rows are back to `entry` with `zero_seen` 1 in `cai_sites.txt` (as the
modded sequence had shown); the `check_block` zero rule stays for the other rows. The user's
`nr20_naval.save` (also written by the original, 11:15) adds two more: a 0 at
`CAI_BDIM_MOVE_TO_POSITION #5` (row now `entry` / 1 too) and the value 2383 at one intention's
`CAI_BDI_COMPONENT_PROPERTY_SET #13`, which is no component of that block (it also appears in the
director's MOBILE_COUNT lists; meaning UNKNOWN), so `check_block` no longer applies the "names a
component" rule to that one site (`cai::DATA_RULE_EXEMPT`); the removal cascade is unchanged. Every
vanilla save in the folder passed `user_saves_pass_the_checks` (test since removed). Compatibility with the
original is out of scope now (the user's decision), so nothing more was spent on it.

Two consequences of that decision (same day): `save_check::Report` now splits its lines into
`violations` (rules our own load or the save's plain consistency needs) and `informational` (rules
that only guard the original's loader: every "pathfinder" / "OBSTACLE_LISTS" / "character obstacle
of missing" / "AI block" line; our loader reads neither `CAMPAIGN_PATHFINDER` nor `CAI_INTERFACE`),
and `save_audit check` prints the two counts apart. The `user_saves_*` tests skip a save that
vanishes, is locked or was written under 10 s ago, since the user may be playing while they run.
The scripts' restriction lists are now kept in our saves (UI_FIDELITY.md "Where I am", round of
2026-10-04 evening).

## 32. Recruitment managers of new queue items (2026-10-08, polish round)
Every `REGION` of the 8 shipped startpos files and the two vanilla saves (`auto_save.save`,
`Great Britain Early April 1811.save`) holds exactly one `REGION_RECRUITMENT_MANAGER`, at #27
(CONFIRMED by census: 30/72/25/30/72/25/31/8 regions, one manager each); port managers sit only in
`REGION_SLOT`s (11/39/0/11/39/0/13/4). The manager is v1 = {`REGION_RECRUITMENT_ITEM_ARRAY` v0, bool
false in every file, i32 unique object id referenced nowhere else}. So the writer (`save.rs`
`write_region`) puts a new land item only in the region's own manager, never a port's.
Malformed records are repaired in one place, `save.rs` `repair_recruitment_managers`, on the save's
copy of its source, and only on the managers that new queue items (ones without a valid load link)
go to, before any item is written: a land item's (the region's own), a ship's (the first port's).
Every other manager, and a record no new item needs, is left as it is, so a file saved untouched
keeps its bytes and one new land item changes nothing but the region's own manager. One rule places
what is added: a missing part is an empty one appended after its parent's last child. A REGION
without its manager gets one (for a record holding exactly #0..#26 that is the original's #27), a
manager without its `REGION_RECRUITMENT_ITEM_ARRAY` gets one (for an empty manager, #0), and the save
logs one line naming each repaired region. Our loader finds both by type (`world::read_region`,
`world::read_recruitment`), so an appended part moves no field; it reads a missing manager, or a
manager without its array, as an empty queue, with a `LoadWarning` (`RegionWithoutRecruitmentManager`,
`RecruitmentManagerWithoutItems`). The repair runs at save rather than at load because the loader
reads the file read-only and the game re-parses the source bytes when it saves (`napoleon`
`campaign/play.rs`), so a load-time repair would not reach the written file. After it, the writer
(`write_recruitment`) resolves every item's manager and record before it changes any manager, so a
missing one is a `SaveError` that leaves the record as it was (never a panic, a silent drop or a
half-written queue), and no queued item is lost in any record shape or queue order.
PROVISIONAL: a new ship in a region with no port manager (the recruit command refuses a ship there,
since naval points come only from port buildings, CONFIRMED 0x00B61EE0, so the original never has
this state) goes to the region's own manager, as before, and loads back from there as a queued ship.
No data shows where the original would keep it; it would need a port slot that has no manager.
Repeated `REGION` ids (malformed): the loader reads only the first record
of an id, all of it (forts and rebel faction too, `world::load_world`, `LoadWarning::DuplicateId`), and
the save drops the others before it indexes the source (`save.rs` `drop_duplicate_regions`, one log
line), so the written file holds exactly the regions the model was loaded from and none keeps a stale
owner (#20), residences or queue.

## For 0-B (please pass on)
Since round 4 the loader reads `World::trade_accumulated` from the saved routes (§12) **before** it
computes `FactionDetails::trade_commodity_standin`. The stand-in is "stored trade income minus the
computed route values", and the computed values now include the loaded accumulated values, so the
stand-in shrinks by exactly that amount (it was larger before because the accumulated part sat inside
it). Total trade income at load is unchanged; the split between "accumulated" and "stand-in" moved.

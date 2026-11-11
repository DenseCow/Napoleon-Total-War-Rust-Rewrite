# HANDOFF — current state

Keep this file short. It holds what is true now: replace finished items rather than appending logs.
Old session logs are in `docs/archive/`.

Last updated: 2026-11-10. Main tests at last full run (2026-10-08): `cargo test --workspace` passes on the install.

## This machine (2026-11-10: the checkout moved to a new PC, restored from a backup)

- **Layout:** the main checkout and the `NR-<x>` worktrees sit side by side in one folder (not `%USERPROFILE%\Documents`);
  `git worktree repair` re-linked them and each is a git `safe.directory`. `ntw-evidence`, `ghidra-backups` and the Ghidra
  project (`napoleon recomp.rep`) are in that same folder, not under `%USERPROFILE%\Documents`.
- **Rust:** 1.98.1 (the version the old machine built with), installed through rustup. In bash add it with
  `export PATH="$HOME/.cargo/bin:$PATH"`.
- **Exe tracing is REA + IDA here (user, 2026-11-10: IDA for everything, Ghidra kept beside it so no progress is lost):**
  workers read `Napoleon.exe` through the `mcp__rea__*` tools (provider `ida`, a copy of the exe under
  `%LOCALAPPDATA%\rea-ida\targets`, same virtual addresses; the first query after an open waits several minutes).
  ghidra-mcp is not installed (`.mcp.json`'s bridge fails to connect).
- **Names and comments live outside the exe database.** Every REA open of the exe is a fresh IDA analysis with no names
  (REA cannot open a saved database), so the names are kept as files and replayed: the Ghidra export
  `%LOCALAPPDATA%\ghidra\export\napoleon_ghidra_export.jsonl` (2,667 named or typed functions, 1,533 labels, 17,785
  comments; Ghidra 12.1.4 and the project copy are under `%LOCALAPPDATA%\ghidra`) plus the journal
  `%LOCALAPPDATA%\rea-ida\names\journal.jsonl` for everything named since. Tools in `%LOCALAPPDATA%\rea-ida\tools`:
  `import_ghidra_names.py --apply` replays both into the open database (about 23,600 edits; a private test run applied
  all but 55), `merge_pending.py` checks and `--apply` adds workers' new write-ups to the journal.
- **V5 on this machine** = the function's name and plate comment as JSON lines in
  `%LOCALAPPDATA%\rea-ida\names\pending\<worker>.jsonl` (the manager merges them); workers never rename through REA. One
  writer at a time on the exe database, same lock as Ghidra's.
- **The names are in the live database (2026-11-10)** as long as the exe stays open in REA: 3,193 names, 2,419 function
  comments and 14,861 line comments applied, 55 leftovers in the importer's log. After any close and reopen: one query
  (about 5 minutes), then the importer again (about 20 minutes, run detached; check that `0x420d00` reads
  `RegisterProposePeaceCommand`). A name can also be looked up with a search for the address in the Ghidra export (the
  journal file appears with the first merge of pending write-ups).
- **Bridge fix (applied by the user, 2026-11-10).** IDA queues a few milliseconds of follow-up analysis after some
  renames and about 1 in 1,400 decompiles; nothing in the headless worker ran that queue, and REA then refused every
  query ("IDA auto-analysis is incomplete") until a close and reopen. `tools\patch_bridge_drain.py` there makes the
  bridge's health probe run the queue (the line is marked `LOCAL PATCH` in the venv's `ida_pro_mcp\ida_mcp\api_core.py`,
  the original is kept beside it as `.orig`). A session may not change that file itself (the permission check refuses
  it): after a venv reinstall ask the user to run the script again.
- **File identity fault since the reboot of 2026-11-10 15:33 (after Windows update KB5127690):** a file looked up by
  path reports a different volume serial than the same file through an open handle, on both drives (Node `lstat` vs
  `fstat`; verified with a probe). Two effects. REA's `open_binary` refuses the exe copy ("Selected input changed while
  being read"): open it through the loopback share, `\\localhost\C$\<rest of the path>`, where both lookups agree. The
  shell tools return no output and kill a command after a few seconds: send output to a scratchpad file and read it, and
  run cargo detached (`nohup bash -c "... > out.txt 2>&1" & disown`), polling the file. Worker briefs must say both. If a
  plain `echo` in the shell tool shows its output again, the fault is gone: delete this bullet.
- **Not installed:** `gh` (so `tools/claims.sh` and the push hook's claim sync cannot run).

## Resume here (2026-10-22)

- **Repos:** work happens in the private repo (origin, `…-Private`). The public repo
  `DenseCow/Napoleon-Total-War-Rust-Rewrite` is a snapshot mirror for contributors: publish after a
  merge with `bash tools/publish_public.sh`; contributor PRs come back with `… import` (CLAUDE.md Hard
  rules). Its CI (`.github/workflows/ci.yml`, Windows, no game install) had its first run on 2026-10-08;
  publishing again cancels a running CI run, so batch publishes.
- **Workflow per branch:** finish → `/code-review medium` (fix commits only after the first round) →
  merge, applying the worker's BACKLOG ticks/Polish lines in the merge commit → `bash tools/progress.sh`
  → push (the hook syncs public #2) → publish in batches. Running work = the BACKLOG `(running: …)`
  markers and `git worktree list`; merged work = `git log`.
- **Resume points:** the one debugger sitting sheet at the top of docs/FOR_USER.md. Running work: BACKLOG markers.
- **Branches (state on 2026-11-10; worktrees `../NR-<x>`, a fresh worker for each next step, BACKLOG markers kept).**
  Reviews are files in the session scratchpad, so a new session reruns a review it cannot find.
  - Started 2026-11-10, full review each: `recruitmarch2` (the nine recruit-march lines) and
    `victorytest` (`NR-victoryscreen3`), both in a review fix round; `classslots2` (`NR-polish67`) and
    `abilitygate` (`NR-traderaid3`).
  - Exe write-ups: `actionclearsv5`, `regionvaluefix`, `actionclearsfix`, `victoryscreen3` and `allygarrison` are in
    the journal but not yet in the live database (one importer `--apply` is owed), and so are `scriptevents`,
    `expelplace`, `rallyinspire`, `battlekeys`, `warcall`, `portblockade2` and `residents` (merged 2026-11-10). Still in `names\pending\`, each to
    merge after its branch's review corrections: the branches above.
- **Plan from next session (user, 2026-10-23: finish as fast as possible):** 1) finish the WIP branches above. §0 is closed for new lines: the triage of 2026-11-10 moved its open lines to the "From §0 (triage, 2026-11-10)" blocks at the end of §3, §5, §7 and §12; 2) slots go to §2, §3 and §5 in parallel; 3) add the week's net tag change to the Progress block (`tools/progress.sh`) and brief workers to leave their area with fewer tags than they found.
- Usage (user, 2026-10-22): 12-point blocks (`BlockPercent` in tools/usage_budget.ps1) to the Wed 2026-10-28 reset. The Wed 10-21 night ended at 18% of its 22% cap; its 4 unused points go to the next block (`usage_budget.ps1 extra-next 4`, stored in `next_extra.txt` and applied once when the block opens, so Thu's cap is its opening reading + 16).
- **Disk:** C: filled up on 2026-10-09; 21 merged worktrees were removed (~100 GB back). Remove a
  worktree after its merge; check `df -h /c` before starting builds.
- Evidence saves live in `%USERPROFILE%\Documents\ntw-evidence\saves` (never inside a target folder);
  recovered build-folder data in `ntw-evidence\recovered-targets\`.

## Open bugs

- **Campaign:** the region details panel is filled since 2026-10-10 except Effects, NextTown and the selected-army
  garrison (PROVISIONAL, BACKLOG §0-E); the user couldn't find the demolish button (offered from a building slot via
  `CanDemolishBuilding`: check it shows).
- **Battle** (`--battle-key NHB_Austerlitz --skip-deployment`):
  - Unit cards hidden ~3 min in Austerlitz only: likely its intro cutscene hides the HUD and we run the
    cutscene timer without playing it. Play or skip it the way the original does.
  - Smoke looks like flat floating cards: compare the original's per-particle size/rotation/alpha curves,
    frame animation and soft (depth-fade) blending.
  - No muzzle flashes: the `muzzle_flash` names are exe-side and reach no effect group; find the exe's
    rule. `MUZZLE_HEIGHT` 1.35 m is PROVISIONAL.
  - No artillery crews drawn at the guns.
  - Glowing streaks from the middle of each unit (user screenshot 2026-10-09, Austerlitz, standing
    units on the frozen ponds): a bright yellow-white tapered beam per unit, pointing down-left
    toward the camera, several unit-lengths long, for the whole battle (from the start, moving or
    not). Possibly unit dust (fx_draw.rs ~506, emitted per
    unit) drawn with the wrong size/velocity/blend; find which effect emits it, then compare with
    the original.
  - Sea water (user check 2026-10-10, NHB_Nile, original side by side): ours pale sky-blue and far too bright, original dark
    grey-green; our waves are long regular stripes, the original small irregular chop; foam on shows no change. Ships, naval
    HUD and the wind compass are §4 (not started), not this bug.
  - Shrubs don't sway (trees do); "shrubs sway with the battle wind" is INFERRED and fails in game.
  - Red/blue boxes round every unit and broken red shapes in empty ground (user screenshot 2026-10-10,
    snow map; low priority, user): the boxes are our untagged debug outline (`napoleon` battle/view.rs
    ~1094, `unit_color`), four straight lines joined only at the terrain corners, so they sink into hills;
    the empty shapes are units with no men drawn that our musketeers still fire at. BACKLOG §7 Battle UI.
- **Original reference shots** still to match (agent options, sabotage, enlist general, settlement tabs):
  `analysis/campaign/CAMPAIGN_UI.md` "Original reference shots".

## Ghidra state (2026-10-07)

Napoleon.exe is fully analysed (47,359 functions). Backup:
`%USERPROFILE%\Documents\ghidra-backups\napoleon-recomp-2026-10-07-postanalysis.gar`. About 14
functions were renamed without plate comments; `BuildCharacterDetailsInfoTable` `0x009AD250` is at 45
completeness. Redo that documentation pass, then one worker runs `ORPHANED_CODE_DISCOVERY_WORKFLOW.md`
and `STRING_LABELING_CONVENTION.md` over the whole exe (back it up first). Setup rules: CLAUDE.md.
Before any debugger session: `cargo test -p ntw_data --test probe_script` and `cargo test -p ntw_data --test probe_install -- --ignored`; if either fails, don't run the probe.

## Who works on what

- **Claude (local):** the BACKLOG `(running: <worker>)` markers, mirrored on the pinned public claim issue #2 (`bash tools/claims.sh list` for contributor claims).
- **Free model (sandbox):** inactive (user, 2026-10-09). Its sections (§2 battle effects, 0-D, 0-E, 0-G) are
  open to our workers; its last round (0-D shrubs) is already in main (`b7b5583b`). Worktrees removed,
  branches kept.

## Needs an in-game check

Batched for the user: only what the screenshot harness can't do (a worker ran the rest on 2026-10-10:
`analysis/fidelity/IN_GAME_CHECKS_2026-10-10.md`; our shots in `ntw-evidence\screens\ours\2026-10-10_*.png`).
Delete a line once checked and record the result where it belongs. Base command: `cargo run --release -p napoleon -- <flags>`.
- (user 2026-10-10: the rest is verified by workers from data, layouts, the harness and Ghidra; only a look that no file, trace or harness shot can settle comes here)
- Quick look on a fresh user folder (no preferences file; uifail6): start with no flags, open Options → Graphics and the custom battle setup: both show the same unit size (the "large" step, registered default 2), and trees in a battle (`--battle`) draw to the "high" distance (150).
- Side-by-side, battle unit size (unitscale, INFERRED single-player link): a custom battle (`--ui-click single_player,sp_battle,button_classic_battle,button_host,button_ok`) with one line-infantry unit per side: at the default "large" a 160-man card fields 120 in both games; with the preferences file holding `campaign_unit_multiplier 0.25` ("small") it fields 40. `--battle-key NHB_Austerlitz` shows each unit's `num_soldiers` from the battle file unchanged.
- Side-by-side, allied garrison and the free spot (allygarrison, PROVISIONAL / INFERRED): `--campaign eur_napoleon --campaign-faction france`; march an army into an ally's settlement that has no garrison (it enters in both games), order a full army into your own full garrison (ours walks up and nothing moves until the exchange panel exists; the original opens the panel), and finish a recruit in a settlement whose garrison is full (the new army stands on the same side of the town in both games).
- Side-by-side, battle keys (battlekeys, INFERRED / PROVISIONAL): `--battle-key NHB_Austerlitz --skip-deployment`. P pauses, CTRL+T changes speed, T toggles fire at will, R runs the selected units; W/S/A/D move the camera, Q/E rotate, X/Z up and down, V/C tilt, the numpad works; CTRL+A selects all, RETURN deselects; F6 / F7 hide the orders and cards panels on the first press and show them on the second; ESC opens the menu; a double right-click on the ground makes the selected units run. With `$env:NAPOLEON_BATTLE_RESTART_KEY='1'`, F8 restarts with fresh views; without it R and F8 do not restart.
- Side-by-side, Rally and Inspire (rallyinspire, PROVISIONAL / INFERRED; when the ability button lands): `--battle-key NHB_Austerlitz --skip-deployment`. Rally on a wavering or routing allied unit: the routers rally at once, the morale lift lasts 30 s, the button stays lit, then recharges for 180 s; 2 uses. Inspire on a unit about 200 m away: the general walks up and stops within about 75 m, accuracy and melee rise on the unit card, the morale effect lasts 30 s, the stats drop back after 180 s.
- Quick look, residents (residents): `--campaign eur_napoleon --campaign-faction france`; walk an army into an undefended enemy town (walls or not): the occupy / loot choice appears; capture a settlement holding a known enemy spy: both "spy detected / executed" messages; a successful assassin on a governor in a town ends inside that town.
- Side-by-side, where an expelled army lands (expelplace): `--campaign eur_napoleon --campaign-faction france`; with a third faction's army standing in a region (military access, or an ally's), give that region to another faction in a deal; the army should jump to the same spot as in the original: the edge of its nearest own region's settlement area, and next to it when another army already stands there.
- Side-by-side, port blockade (portblockade2, PROVISIONAL): `--campaign eur_napoleon --campaign-faction france`. A navy blockades an enemy port: the trade prices, the port's ships held and the siege's land units held match. The docked navy sallies against its blockader: both outcomes match, and the blockade lifts only when no foreign navy is left docked. The end-blockade button, and peace with the port's owner, each lift it. ORIGINAL BUG: an army aboard a blockading navy lands elsewhere; the original keeps the port blockaded, ours lifts it. A settlement with no place for a recruit (garrison full, no army within reach, no free spot) holds its finishing land recruits, including the commander-targeted ones. An army takes an enemy port town with the owner's navy docked: the original puts the navy out to sea (the navy surrender), ours leaves it docked (PROVISIONAL, records the gap).
- Side-by-side, outcome movie loudness (victoryscreen3, INFERRED: gain = level × 0.01 of master × movie, a disabled group silences): `--campaign`; lose the last region with master 50 / movie 50, then with the movie group off, in both games.
- Side-by-side, region worth in a deal after a conquest (regionvalue, INFERRED refresh step): `--campaign eur_napoleon --campaign-faction france`; take a region next to a neutral's land, then in the same turn open diplomacy with a third faction and ask for one of its regions that borders the conquest. The AI's answer (accepts / asks more) should match the original's for the same offer; if the original only changes its answer a turn later, the refresh is not immediate.

## Needs the user

- **Stray branch to delete:** `origin/work/fidelity-campaign-w4` is superseded (main has later
  versions of its two docs), but auto mode blocks remote deletes, so the user runs
  `git push origin --delete work/fidelity-campaign-w4`. Same branch on the sandbox.
- **Raw Ghidra dumps in the private repo's history** (322 + 65, untracked since): they stay unless the
  user wants a history rewrite. The public repo has fresh history and none of them.
- **Debugger reads still open:** all in the one sitting sheet (docs/FOR_USER.md top, `analysis/fidelity/debugger/SITTING_SHEET.md`).
- **Renderer decisions (2026-11-10):** the six "Open decisions (the user's)" at the end of DESIGN.md §3.7.2 (reference
  machine for 60 fps, vanilla metals, anti-aliasing / deferred, TOML reader vs. the `toml` crate, quality options in
  settings, vendor upscalers). §2 drawing work waits on them.

## Waiting for a worker slot

- **Movement-arrow spacing/colours (debugger capture 2026-10-07, British army hover):** the arrow
  setter `FUN_00A27C30` IS called directly, from `0x009D6270` (Ghidra missed it before). Its `this` is
  `caller_this+0xA88`. On entry the stack held `0x302AD040, 0x14ADC2D8, 1, 0x1B, 0.0f`: arg 1 is an object
  (first dword looks like a vtable, `0x0136A62C` static, and it holds 0x19 and 0x1B), arg 2 an array of
  16-byte entries `{?, 8, 8, ptr}`, and the float is 0.0 (the "keep every point" spacing?). So UI_FIDELITY
  §7's argument order (colour, flag, path, spacing) looks wrong. Next: work out statically, from the
  caller at `0x009D6270` (its own stack params `[esp+0x118]`), what each argument is, then set
  SPACING and the colours. The process ran with the debug heap (`0xBAADF00D` fill): next launch, pass
  `_NO_DEBUG_HEAP=1` through the launcher's Env option.

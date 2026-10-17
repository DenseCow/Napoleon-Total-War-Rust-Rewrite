# HANDOFF — current state

Keep this file short. It holds what is true now: replace finished items rather than appending logs.
Old session logs are in `docs/archive/`.

Last updated: 2026-10-16. Main tests at last full run (2026-10-08): `cargo test --workspace` passes on the install.

## Resume here (2026-10-15)

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
- **Night 2026-10-17 (block cap 45.5%; no resume scheduled):** merged tonight: dippermai, polish48, polish49, roundstart, polish50, traderaid, dipempty, zocflood (FOR_USER "Done tonight"). **Resume next session (each branch pushed, its BACKLOG marker kept; a fresh worker per branch):**
  - `work/polish51` done (1c0c77c4, NR-polish51; HUD date from loc keys, EPISODIC_RESTRICTIONS #8, ORIGINAL BUGs 0x00979600 / 0x008ED110): run `/code-review medium` on `main...work/polish51`, then merge: delete its Polish line, add its three Polish lines (frontend.rs ~502 `seasons_onscreen_` key misses: the key is `seasons_onscreen_season_<x>`; the save header season from the loc name 0x00888320→0x008ED110, not `header_name`; create Ghidra functions at 0x00979600 / 0x009E24D0 / 0x009E2010 with V5).
  - `work/victoryscreen` (6422ea82, NR-victoryscreen): §0 defeat/victory screen. The scripts' side is traced (CAMPAIGN_FIDELITY "Campaign over" → "The scripts' side"); next: trace the exe posting of messages 34/35/37 (strings "ExitFunction", "ShowVictoryOptionsPopup"), then the model and the UI.
  Every brief names `tools/self_check.sh` and holds one item. §11: framework parts only; the unwrap audit last.
- **Disk:** C: filled up on 2026-10-09; 21 merged worktrees were removed (~100 GB back). Remove a
  worktree after its merge; check `df -h /c` before starting builds.
- Evidence saves live in `%USERPROFILE%\Documents\ntw-evidence\saves` (never inside a target folder);
  recovered build-folder data in `ntw-evidence\recovered-targets\`.
- Usage (user plan, 2026-10-17): 16-point blocks (~17 with the reading lag), one a day Sat–Tue, to use the ~70 points left at 30% before the weekly
  reset Wed 2026-10-21 00:00; `BlockPercent` in tools/usage_budget.ps1.

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

## Needs the user

- **Stray branch to delete:** `origin/work/fidelity-campaign-w4` is superseded (main has later
  versions of its two docs), but auto mode blocks remote deletes, so the user runs
  `git push origin --delete work/fidelity-campaign-w4`. Same branch on the sandbox.
- **Raw Ghidra dumps in the private repo's history** (322 + 65, untracked since): they stay unless the
  user wants a history rewrite. The public repo has fresh history and none of them.
- **Debugger reads still open:** all in the one sitting sheet (docs/FOR_USER.md top, `analysis/fidelity/debugger/SITTING_SHEET.md`).

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

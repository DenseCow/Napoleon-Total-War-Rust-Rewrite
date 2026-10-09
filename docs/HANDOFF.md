# HANDOFF — current state

Keep this file short. It holds what is true now: replace finished items rather than appending logs.
Old session logs are in `docs/archive/`.

Last updated: 2026-10-09. Main tests at last full run (2026-10-08): `cargo test --workspace` passes on the install.

## Resume here (2026-10-09)

- **Repos:** work happens in the private repo (origin, `…-Private`). The public repo
  `DenseCow/Napoleon-Total-War-Rust-Rewrite` is a snapshot mirror for contributors: publish after a
  merge with `bash tools/publish_public.sh`; contributor PRs come back with `… import` (CLAUDE.md Hard
  rules). Its CI (`.github/workflows/ci.yml`, Windows, no game install) had its first run on 2026-10-08;
  publishing again cancels a running CI run, so batch publishes.
- **Workflow per branch:** finish → `/code-review medium` (fix commits only after the first round) →
  merge, applying the worker's BACKLOG ticks/Polish lines in the merge commit → `bash tools/progress.sh`
  → push (the hook syncs public #2) → publish in batches. Running work = the BACKLOG `(running: …)`
  markers and `git worktree list`; merged work = `git log`.
- **Resume points (2026-10-09, block cap 46%):** workers commit and push a checkpoint every ~20 min.
  1. gait-blend2 (NR-gait-blend2, head `baa65793`, don't merge yet): done except two debugger reads (FOR_USER gait
     sitting): walk-ordered horses trot (rider vs horse record speed) and infantry at store C. Arrival braking,
     rider legs, gait by order and ladder timing are CONFIRMED; resume block at the end of UNITS_TERRAIN_FIDELITY.md §1.10.
  2. mod-loading2 (NR-mod-loading): review round 1 found 2 blocking (precedence must follow the whole graph
     `0x0108EBB0`/`0x0109ECF0`; `ntw_ai` tables.rs:79 `load_raw` must read the merged tables) plus three silent
     failures (unknown script line, `--mods` with no value, mods.rs:734 warning per open). The fix round was cut by
     the cap before any edit (branch clean at `9e7ffd58`). Notes: the exe drops a precedence pair that would close a
     cycle (`0x0108E8F0`); the AI's raw tables have no traced key (trace the exe's key reader per table). Then review
     the fix commits only. The unattended start-up debugger run for
     the six open points is MOD_LOADING.md §6 (no user input).
  3. 0e-panel (NR-0e-panel): region details panel fields (`0x009E69B0`) and `OnDivorceChild`; check its last push.
  4. campaign-source (NR-campaign-source, `75c2b8c2`): done: versioned RON/deflate own save (`own_save.rs`, F5), AI keys and
     region base values moved into the model, old ESF saves still load, `ntw_campaign::source` seam used by the scene,
     campaign list and UI. Left: the real-install round-trip and seam tests, the full test run + clippy + own-diff check,
     analysis/modding/OWN_SAVE_FORMAT.md, and the display port (`MapData` still wraps `CampaignMap`). Was: our own campaign save format and the `CampaignSource` seam
     (MODDING_AUDIT.md §4 items 1-2); check its last push.
  5. Then: MODDING_AUDIT.md items 3-4 (after mod-loading2 merges), ui-fixes (front-end UI scale, battle
     CursorPosition, prelude x/y). `tools/usage_budget.ps1` has a local `$WrapAt = 1.0` (user, 2026-10-09): ask
     whether to keep and commit it or restore 2.0.
- **Disk:** C: filled up on 2026-10-09; 21 merged worktrees were removed (~100 GB back). Remove a
  worktree after its merge; check `df -h /c` before starting builds.
- **Next:** §0, several workers tracing in parallel under the Ghidra writer lock, plus the §11 modding work alongside it (user, 2026-10-09: mod loading and the audits come early); the unwrap audit last.
- Evidence saves live in `%USERPROFILE%\Documents\ntw-evidence\saves` (never inside a target folder);
  recovered build-folder data in `ntw-evidence\recovered-targets\`.
- Usage (user plan, 2026-10-07): one 11-point block a day (~12 with the reading lag), until the weekly
  reset Wed 2026-10-14 00:00; `BlockPercent` in tools/usage_budget.ps1.

## Open bugs

- **Diplomacy screen (§0-E):** diplomacy2 merged (greeting by attitude, diplomat portrait, button lists
  for war / peace+trade / ally, Power and Wealth words, regnal numerals). Still different from the
  original: Austria's power reads "Mighty" (original "Terrifying"; it ranks 5th behind Prussia and
  Russia: debugger read at `0x00949630`), the panel creates no `diplomacy_button_*` components from the
  lists (ignored test; cause untraced), red cancel texts. Original screens in
  `%USERPROFILE%\Documents\ntw-evidence\screens\`: `2026-10-08_original_diplomacy_negotiation_britain_france.png`,
  `…_britain_ottoman_trade.png`, `…_britain_austria_ally.png`, `2026-10-08_original_diplomatic_relations_britain.png`.
- **Campaign:** the region details panel shows the region name but its other fields keep the layout's defaults
  (BACKLOG §0-E); the user couldn't find the demolish button (offered from a building slot via
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
  - Shrubs don't sway (trees do); "shrubs sway with the battle wind" is INFERRED and fails in game.
- **Original reference shots** still to match (agent options, sabotage, enlist general, settlement tabs):
  `analysis/campaign/CAMPAIGN_UI.md` "Original reference shots".

## Ghidra state (2026-10-07)

Napoleon.exe is fully analysed (47,359 functions). Backup:
`%USERPROFILE%\Documents\ghidra-backups\napoleon-recomp-2026-10-07-postanalysis.gar`. About 14
functions were renamed without plate comments; `BuildCharacterDetailsInfoTable` `0x009AD250` is at 45
completeness. Redo that documentation pass, then one worker runs `ORPHANED_CODE_DISCOVERY_WORKFLOW.md`
and `STRING_LABELING_CONVENTION.md` over the whole exe (back it up first). Setup rules: CLAUDE.md.

## Who works on what

- **Claude (local):** the BACKLOG `(running: <worker>)` markers, mirrored on the pinned public claim issue #2 (`bash tools/claims.sh list` for contributor claims).
- **Free model (sandbox):** inactive (user, 2026-10-09). Its sections (§2 battle effects, 0-D, 0-E, 0-G) are
  open to our workers; its last round (0-D shrubs) is already in main (`b7b5583b`). Worktrees removed,
  branches kept.

## Needs an in-game check

Batched for the user. Delete a line once checked and record the result where it belongs.
Base command: `cargo run -p napoleon -- <flags>`.




- **Region labels (merged 0e-labels):** `cargo run -p napoleon -- --campaign eur_napoleon --campaign-faction france
  --no-intro`, open Diplomatic Relations: the region labels sit under the panel; open a region's details: the
  title is the region name.
- **Battle HUD layout (merged with battle-ui2):** `cargo run --release -p napoleon -- --battle` at 1920x1080, then
  resize the window to 1280x720. The HUD scales to fit like the original at 1920x1080 (deployment panel, unit cards,
  orders bar on the cards), and clicks land on its buttons at both sizes.
- **UI side-by-side (merged `53361ec`), British campaign, compare with the original:** `--campaign mp_eur_napoleon --campaign-faction britain --no-intro`, then (1) open Technology: no line across the title; (2) select Wellesley: Army | Recruitment tabs, his portrait first (the Recruitment tab is empty for now: PLACEHOLDER); (3) open Lists: generals show portraits, colonels the unit card, and the panel docks top-right on a wide screen.
- **Attribute icons (merged `25a6b21`):** an agent's card (spy: spying picture) and the agents-tab
  recruitment rows, against the original. `--campaign eur_napoleon --campaign-faction france --no-intro`.
- **Settlement panel:** demolish, fort tab, recruitment prices, agents tab in a town with agents.
  First `cargo test -p ntw_script --test campaign_ui agents_panel -- --nocapture`.
- **Agent actions:** agent card → Assassinate / Sabotage / Duel opens a target picker with names,
  flags and success %. Clicking a row closes it. Compare % with the original. Sabotage Army is a known no-op.
- **Fog labels (INFERRED):** after a few turns, explored settlements keep their labels when the camera
  moves away; never-seen ones have none.
- **Fort selection:** `--campaign eur_napoleon --campaign-faction france --no-intro
  --campaign-ui-click selectfort:eur_france --screenshot target/tmp/fort.png`.
- **Promote panel:** select a French army → Promote. The panel is centred over the HUD with its bottom
  touching the band (CONFIRMED from `Huds.MoveRelativeToHUD`).
- **Sea battles:** `--battle-key NHB_Nile`; `--battle --battle-map hb_toulon`; Nile again with
  `$env:NAPOLEON_SEA_FOAM=1`.
- **Experience:** in Austerlitz, veterans waver later, rout shorter and tire slower.
- **Battle effects (INFERRED/PROVISIONAL):** in Austerlitz, the 12-pdr muzzle report is larger than the
  6-pdr's; canister bursts `LandGunFire_canister`; a 12-pdr shrapnel burst is smaller than a round
  shell; shell scorches are orange; grenade/carcass hits leave no scorch; ground debris stays upright
  from a low camera; no `FX: no group` lines in the log. One frame:
  `$env:NAPOLEON_FX_SHOT="target/tmp/fx.png"`; counts: `$env:NAPOLEON_FX_LOG=500`.
- **Flags:** unchanged since the cloth refactor (`cargo test -p ntw_sim --test flag_install -- --ignored`).
  Spanish and rebel units fly their own flags, not `flag_default` (PROVISIONAL; compare with the original).
- **Before any debugger session:** `cargo test -p ntw_data --test probe_script` and
  `cargo test -p ntw_data --test probe_install -- --ignored`. If either fails, don't run the probe.

## Needs the user

- **Stray branch to delete:** `origin/work/fidelity-campaign-w4` is superseded (main has later
  versions of its two docs), but auto mode blocks remote deletes, so the user runs
  `git push origin --delete work/fidelity-campaign-w4`. Same branch on the sandbox.
- **Raw Ghidra dumps in the private repo's history** (322 + 65, untracked since): they stay unless the
  user wants a history rewrite. The public repo has fresh history and none of them.
- **Debugger session:** who writes `unit+0xD48`, where `unit_scale` is read, formation radius
  `+0x670`, garrison cap `+0x6C`; Austria's power ranking input at `0x00949630` (Britain campaign, Early
  January 1805, Diplomacy → Austria: ours reads "Mighty", the original "Terrifying").

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

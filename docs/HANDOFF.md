# HANDOFF — current state

Keep this file short. It holds what is true now: replace finished items rather than appending logs.
Old session logs are in `docs/archive/`.

Last updated: 2026-10-10. Main tests at last full run (2026-10-08): `cargo test --workspace` passes on the install.

## Resume here (2026-10-10)

- **Repos:** work happens in the private repo (origin, `…-Private`). The public repo
  `DenseCow/Napoleon-Total-War-Rust-Rewrite` is a snapshot mirror for contributors: publish after a
  merge with `bash tools/publish_public.sh`; contributor PRs come back with `… import` (CLAUDE.md Hard
  rules). Its CI (`.github/workflows/ci.yml`, Windows, no game install) had its first run on 2026-10-08;
  publishing again cancels a running CI run, so batch publishes.
- **Workflow per branch:** finish → `/code-review medium` (fix commits only after the first round) →
  merge, applying the worker's BACKLOG ticks/Polish lines in the merge commit → `bash tools/progress.sh`
  → push (the hook syncs public #2) → publish in batches. Running work = the BACKLOG `(running: …)`
  markers and `git worktree list`; merged work = `git log`.
- **Resume points (2026-10-10 night, stop at 60%):** merged tonight: mod-loading2, polish ×2 + polish-script,
  0e-panel, table-path, campaign-source, power-rank, no-limits, generic-engine, recruit-pop, diplomacy3, recruit-cost
  (all three modding foundations are in). Open branches:
  1. gait-blend2 (NR-gait-blend2, `dd225fb7`, don't merge yet): static trace done (UNITS_TERRAIN_FIDELITY.md §1.10
     "Static trace 2026-10-10": wanted speed = order speed × cos(heading error) × ground/fatigue/slope); one debugger
     read separates the factors (FOR_USER sitting), then port into `ntw_sim` and review. Its branch edits FOR_USER.md:
     keep main's version at merge.
  2. polish-battle: merged (`88a10376`).
  3. deal-items: merged (regions/techs in deals; AI refuses to give them until its evaluation is traced).
  3b. deal-ai (NR-deal-ai, `c0e81d7a`, not reviewed): AI deal evaluator traced (AI_RESEARCH.md §4 "Deal evaluation"),
     tech value (500 + trunc(10 × cost^1.1), `0x00A36B20`) and accept tests ported in ntw_sim campaign/deal_value.rs, not
     yet wired. Resume: goal lists `0x00CC13D0` / `0x00CC0280`, region value `0x00AA1E90` / `0x00A364B0`, AI budget
     (`0x00CBBA80` +0x54), inflation factor in the model/saves; then replace `ai_refuses_deal` and review.
  4. Next in free slots: §0 items (BACKLOG §0); the §11 follow-ups added tonight (spa_napoleon xrefs, campaign list
     from data, render caps, AI fallbacks) after §0; Polish when it passes ~20 lines.
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
  original: the panel creates no `diplomacy_button_*` components from the
  lists (ignored test; cause untraced), red cancel texts. Original screens in
  `%USERPROFILE%\Documents\ntw-evidence\screens\`: `2026-10-08_original_diplomacy_negotiation_britain_france.png`,
  `…_britain_ottoman_trade.png`, `…_britain_austria_ally.png`, `2026-10-08_original_diplomatic_relations_britain.png`.
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
- **Debugger reads still open (battle):** who writes `unit+0xD48`, formation radius `+0x670`, garrison cap `+0x6C`
  (`unit_scale` is solved, BATTLE_FIDELITY row 48). Their probe plan `f0a_battle_probe.cdb.txt` was never committed
  (`.txt` is ignored under `analysis/`): a worker preps the breakpoints statically first; the first two can then run
  unattended at battle load, the garrison cap needs a siege battle (a short sitting, or a battle launched by flag).

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

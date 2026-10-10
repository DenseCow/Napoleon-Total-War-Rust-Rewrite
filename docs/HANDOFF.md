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
- **Tonight (user, 2026-10-10, night mode):** no wrap-up before 71% of the week; at wrap-up, merge what is review-clean and publish main to the public repo (`bash tools/publish_public.sh`).
- **Resume points (2026-10-10 evening, day mode):** merged today: ui-small, capture-refund, army-recruit-tab, deal-ai, in-game-checks, deal-region,
  new-portraits, polish1, gait-blend2, polish2, rule-seams, polish3, force-diplo. Slots (user, 2026-10-10): 3 Opus workers (reviews take one)
  + 1 Sonnet worker on Polish only. §11 (user, 2026-10-10): only framework parts that avoid costly refits later; rule-seams kept (no-rebuild
  system mods that stack); next §11 step before any refit: the seams macro line in BACKLOG §11. Open:
  1. ui-fails (NR-ui-fails, Opus, running): the four "Failed in-game checks" in Open bugs; 9a14cfa9 fixed the recruitment tab, then the worker was lost; a new worker is on the other three.
  2. region-transfer (NR-region-transfer, Opus, running): §0-B `CDIR_INTENTION_TRANSFER_REGION_OWNERSHIP`.
  3. (slot free: treasury-rules merged)
  4. (polish4 merged)
  5. polish5 (NR-polish5, Sonnet): done (3caf8abf + 2ab0345a); review QUEUED. At merge: delete "Own save: header pixels…", "The campaign
     model addresses building slots…" (stale: SlotRef exists) and the campaign_play.rs temp-file line.
  6. QUEUED for the next free Opus slot: gait-blend3 (Open bugs "Gait"), then the polish5 review, then the polish4 fix review (e91f9a51), then the polish6 review (93c3d61a); hud-state (Open bugs "Battle HUD") once ui-fails is merged; sea-water (Open bugs "Sea water"); self-checks (Opus: verify without the user, user 2026-10-10: (a) hired and field-promoted generals get a european general portrait on the army card and in the Lists, by harness shot or model test (the Enlist pool portraits show, user screenshot 2026-10-10); (b) agent action success % (Assassinate/Sabotage/Duel) against the exe formula, with tests; (c) fog labels rule (INFERRED) traced in Ghidra and tested; (d) agent attribute icons on the agent card and agents-tab rows, from the UI layouts/.luac and data (ours: ntw-evidence screens/ours/2026-10-10_settlement_agents_tab.png); (e) settlement recruitment prices (data) and card spacing (layout file; ours overlap slightly)).
  8. polish7 (NR-polish7, Opus in the Polish slot): Ghidra write-up lines (new-portraits, generic-engine, ui-small plate comments, mod-loading2).
  7. polish6 (NR-polish6, Sonnet): done (93c3d61a: DIPLOMACY_OPTIONS import, deal_value research_need, screenshot-harness port; napoleon tests not run); review QUEUED after item 6. At merge delete its 3 Polish lines (research_need half of the deal-ai line only).
  Agent ids (SendMessage after /clear): ui-fails a25a2b4692299be04, region-transfer a06771a913587b37f, treasury-rules a2020ea12b9268fd0,
  polish4 ae9aa0a950d29fed2, polish5 a4298743580337a9a, polish6 a1a3f09dcfb13fda0, gait-blend2 (old) ae00b6132af647320.
- **Disk:** C: filled up on 2026-10-09; 21 merged worktrees were removed (~100 GB back). Remove a
  worktree after its merge; check `df -h /c` before starting builds.
- **Next:** §0, several workers tracing in parallel under the Ghidra writer lock, §11: only the framework parts that avoid costly refits later (rule-seams running), the rest waits for its turn (user, 2026-10-10); the unwrap audit last.
- Evidence saves live in `%USERPROFILE%\Documents\ntw-evidence\saves` (never inside a target folder);
  recovered build-folder data in `ntw-evidence\recovered-targets\`.
- Usage (user plan, 2026-10-07): one 11-point block a day (~12 with the reading lag), until the weekly
  reset Wed 2026-10-14 00:00; `BlockPercent` in tools/usage_budget.ps1.

## Open bugs

- **Gait (user check 2026-10-10, gait-blend2 merged):** infantry and cavalry walk/run look good, but (1) cavalry still move
  in lockstep: they need the per-soldier offset infantry got so they don't all step at the same time; (2) a slight hiccup/jitter (infantry and cavalry, user 2026-10-10)
  when switching from running to walking. Queued: gait-blend3 in the next free Opus slot.
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
  - Battle HUD (user check 2026-10-10, `--battle`): resizing 1920x1080 → 1280x720 scales and centres it like the original;
    pause and speed buttons work. Wrong: (1) the speed control's selected-state circle doesn't move to the clicked button;
    (2) selecting a unit (card click or in the world) doesn't highlight its unit card. Queued: hud-state after ui-fails merges
    (same files).
  - Sea water (user check 2026-10-10, NHB_Nile, original side by side): ours pale sky-blue and far too bright, original dark
    grey-green; our waves are long regular stripes, the original small irregular chop; foam on shows no change. Ships, naval
    HUD and the wind compass are §4 (not started), not this bug. Queued: sea-water (Opus: ocean.fx inputs, reflection/fresnel,
    wave scale; BACKLOG §2 Water).
  - Shrubs don't sway (trees do); "shrubs sway with the battle wind" is INFERRED and fails in game.
  - Red/blue boxes round every unit and broken red shapes in empty ground (user screenshot 2026-10-10,
    snow map; low priority, user): the boxes are our untagged debug outline (`napoleon` battle/view.rs
    ~1094, `unit_color`), four straight lines joined only at the terrain corners, so they sink into hills;
    the empty shapes are units with no men drawn that our musketeers still fire at. BACKLOG §7 Battle UI.
- **Failed in-game checks (2026-10-10, `analysis/fidelity/IN_GAME_CHECKS_2026-10-10.md`), queued for a fix worker:**
  - Army Recruitment tab lists ~25 overlapping cards including ships (original: 4 land options): an army never
    filters naval units out (`ntw_sim` commander_recruitment.rs ~139-148); title adds ", Great Britain"; icon
    `ui\units\icons\cav_light_hussars.tga` missing.
  - Settlement panel: hovering a construction slot shows an empty dark tooltip frame that stays open after the
    pointer leaves (`ntw_script` ui/campaign/settlement.rs ~726-812).
  - Enlist panel traits show raw keys and the columns overlap (`ntw_script` ui/campaign/army.rs ~680-688, PROVISIONAL).
  - `template.BattleUnitCard.lua:163` "compare number with nil" on NHB_Austerlitz with `--skip-deployment`
    (`ntw_script` ui/battle_prelude.lua ~425-435, card info field nil).
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
- (none: user 2026-10-10, the rest is verified by workers from data, layouts, the harness and Ghidra; only a look that no file, trace or harness shot can settle comes here)

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
- **ui-fails (Opus, after polish1 merges: it shares commander_recruitment.rs):** fix the four "Failed in-game
  checks" in Open bugs, tracing the exe where the fix depends on it, each with a test or a harness screenshot.

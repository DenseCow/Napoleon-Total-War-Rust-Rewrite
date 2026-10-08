# HANDOFF — current state

Keep this file short. It holds what is true now: replace finished items rather than appending logs.
Old session logs, sandbox round reports and the 2026-10-06 review corrections are in
`docs/archive/HANDOFF_2026-10-05_to_06.md`.

Last updated: 2026-10-07. Main tests at last full run: `cargo test --workspace` passes on the install.

## Open bugs (user, in game, 2026-10-06)
- **Diplomacy screen: the X (close) button does nothing** (user, in game, 2026-10-07; ours: `%USERPROFILE%\Documents\ntw-evidence\screens\2026-10-07_ours_diplomacy_close_button.png`, Britain vs France, late June 1805). Also on that screen: Power 0 / Wealth 0 for both factions and "Test" under Current Treaties (check against the original: likely unfilled values and placeholder text). Fix the close the way panel-close did (the panel's own script path, UI_FIDELITY.md §10.1), from the layout/scripts; next free worker after the HANDOFF trim, before polish.
- **Army Recruitment tab, original (user screenshot 2026-10-07, `%USERPROFILE%\Documents\ntw-evidence\screens\2026-10-07_original_wellesley_army_recruitment_tab.png`; CONFIRMED by observation):** Wellesley's army in England (own region), Coalition campaign as Britain, early January 1805: tabs Army | Recruitment; Recruitment shows an **Options** row (4 cards: cavalry with a "2/1" badge and cost 472, artillery, two line infantry) and a **Queue** row of 10 slots, so an army in its own region recruits into itself. Ours (`work/ui-side-by-side`) opens it empty (PLACEHOLDER). After that branch merges, move this into CAMPAIGN_UI.md and the §0 "army recruitment contents" item; trace the options, the badge and the cost in `0x009FE7B0`.
- **Evidence saves: RECOVERED 2026-10-07** from the 4:24 PM shadow copy (the cleanup had deleted `NR-save-compat\target\evidence`). New home, outside any build folder: `%USERPROFILE%\Documents\ntw-evidence\saves` (55 files with `original`, byte-identical); all other non-build data of the 22 emptied worktree targets is in `ntw-evidence\recovered-targets\<worktree>\`. The tests default to the new path; all evidence tests run again (13 pass).

**Resume here (2026-10-07, evening):** the three blockers are MERGED and pushed: walls `28471b5` and music `d6c843b` (both checked in game by the user), panel-close `8aad50d` (checked by the user 2026-10-07).
Next (FIRST, user 2026-10-07): trim this file to what is true now (drop superseded status paragraphs, merged-branch histories and round-by-round notes; move anything worth keeping into the analysis files or docs/archive). Then clear BACKLOG Polish (~41 lines, over the ~20 limit; the "clear first" ones before all; the campaign.rs split goes with that file's lines, the unwrap audit last), then §0 (31 items; §0-A moved to §3), several workers tracing in parallel under the Ghidra writer lock. Everything from 2026-10-07 is MERGED and pushed: walls, music, panel-close, jitter, construction cost/refund, drop-down clock, both polish branches, and the UI side-by-side fixes (`53361ec`: tech links, Army|Recruitment tabs, portraits, Lists docking in the scripts' 1280x960 frame from the debugger sitting). All user-checked except the UI fixes (checks below). After every merge or user check: tick BACKLOG and refresh Progress with `bash tools/backlog_count.sh` and `bash tools/tag_count.sh` (user rule). Evidence saves live in `%USERPROFILE%\Documents\ntw-evidence\saves` (never inside a target folder). Usage (user plan, 2026-10-07): one 11-point block a day (~12 with the ~1-point reading lag), about 15:30, until the weekly reset Wed 2026-10-14 00:00 (~91%); `BlockPercent` in tools/usage_budget.ps1.

Campaign (`--campaign eur_napoleon --campaign-faction france --no-intro`; on this install only the
Coalition campaign and the tutorial are unlocked in the original, so side-by-sides use
`mp_eur_napoleon` / `britain`). **Day of 2026-10-07.** Every branch below is pushed. Workers report to the manager.
Rules since 2026-10-07: exe behaviour that decides a design is traced in Ghidra and CONFIRMED, never
left INFERRED in a brief; only blocking review findings hold a merge (CLAUDE.md "Done means"). `tools/drive_original.ps1` screenshots the original (launch via Steam).
Max 1 manager + 3
agents, and a `/code-review` fork counts as one. Workers do NOT self-review. Resume per branch: finish
the open findings, then `/code-review medium main...origin/<branch>`. Repeat until clean, then merge,
`cargo test --workspace`, push, and add the in-game check line.

- **Panels can't be closed:** MERGED 2026-10-07 (`8aad50d`, branch head `c76b4f0`), checked in game by the user 2026-10-07, after 16 review rounds: lazy layout, selection order and the tab-request refusal as the exe (CONFIRMED), card Range = the gun's longest effective range. Settled: UI_FIDELITY.md §10.1 and "Round 15 fixes".
- **Walls / forts:** MERGED 2026-10-07 (`28471b5`, branch head `b45a5ad`). User check 2026-10-07: the Paris card queues the walls, and the walls mesh appears on the map when they finish (looks off until the §2 shaders). Walls = "Small Star Fort", the LAST construction card (CONFIRMED); infrastructure tab = road. Road repair PROVISIONAL (nothing damages roads). Polish in BACKLOG.
- **Battle music restarts:** MERGED 2026-10-07 (`d6c843b`, branch head `00a5a09`). Root cause CONFIRMED (loop blocks are byte offsets); all Music-group events decoded whole off the audio thread. 10 s decode wait PROVISIONAL; start-rule order untraced (§9). User listen check 2026-10-07: no restart in 3 minutes of Austerlitz.
- **Attribute icons:** MERGED 2026-10-07 (`25a6b21`). Awaits its in-game check (below). Ghidra:
  `BuildCharacterDetailsInfoTable` `0x009AD250` is only at 45, so it needs another documentation pass.

**Ghidra state (2026-10-07):** Napoleon.exe is fully analysed (47,359 functions). Backup:
`%USERPROFILE%\Documents\ghidra-backups\napoleon-recomp-2026-10-07-postanalysis.gar`. The workers
renamed about 14 functions, but their plate comments were blocked by then-missing tools, and the
"finish the Ghidra documentation" pass was cut by the pause. Redo it as part of step 1. Setup rules
are in the CLAUDE.md ghidra-mcp section. After the blockers: one worker runs
`ORPHANED_CODE_DISCOVERY_WORKFLOW.md` and `STRING_LABELING_CONVENTION.md` over the whole exe
(back it up first), the others continue §0.

Side by side with the original (user screenshots, 2026-10-07, Lists → Armies + army bar):
- Attribute icons on general cards: the gold command star (bottom-left) and the rank stars MATCH.
- Generals must show their **portrait** (army bar and Lists rows). The army bar keeps the general's
  soldier count (as ours does); only the Lists rows of named generals have none. Colonels keep the
  unit picture + small star + count.
- **Bug (CONFIRMED by the user in the original):** the army panel ALWAYS has **Army | Recruitment**
  tabs, in fixed positions, wherever the army is; ours shows only Army. Build the tab row from the
  original's layout/script so both are always present (what the Recruitment tab shows outside the
  army's own regions is still to compare). Repro (Masséna, Army tab only):
  `--campaign eur_napoleon --campaign-faction france --no-intro --campaign-demo --screenshot target/tmp/army_tabs.png`. Ours shows the bodyguard card
  with its count for everyone (`DisplayAsUnit = true`, PROVISIONAL, campaign.rs ~line 766): portraits
  are the fix.
- The **Lists panel** is docked top-right in the original (o2 below); ours is centred. Find the
  original's placement rule (layout file / script) and apply it.

**Original reference shots (2026-10-07, Coalition campaign, Britain, 1920x1080, turns 1-4).** In
`target/orig_shots/2026-10-07_britain/` (o1..o6). Ours: `--campaign mp_eur_napoleon --campaign-faction
britain`. These are a final check only. Each fix takes its values from the source: layout files, `.twui`, UI
`.luac` and the exe (Ghidra), never from these pixels. Work queued for a worker: for each item below,
find the rule in the data/exe that produces it, compare it with ours, and fix the difference. Then
take our matching shot (add a harness `selectchar:<name>` step) to confirm. What they show:
- o1 army bar: tabs **Army | Recruitment**; the general's card is his portrait with rank stars, gold
  command star and count; a promote star button at the panel's right.
- o2 Lists: docked **top-right**, x 1290-1910, y 60-775. Generals: portrait; colonels: unit picture,
  small star, 160.
- o3 character details: docked **top-left**, x 10-630, y 60-775 (Subterfuge skill mask + stars, age,
  traits, followers). Agent selected: one HUD tab **Agents**; card = portrait, 3 small stars on the
  left edge, mask icon bottom-left; three round action buttons at the right, greyed with no target.
  Settlement selected: tabs **Construction | Recruitment | Infrastructure | Army**.
- o4 agent actions: right-click a target with the agent opens a centred **Agent Options** popup
  (x 686-1232, y 208-630): Sabotage (no %), Infiltrate 80%, Sabotage Army 75% (spy vs Caen, late Feb
  1805). Sabotage opens a centred **Sabotage** panel (x 650-1270, y 62-776): picture, "Select Target",
  rows of building icon + "Building Level" dots + chance (66%) + bomb button. How the HUD buttons
  enable is still to trace in Ghidra.
- o5 the star button = **Enlist New General** (x 650-1270, y 62-776, centred): "Generals enlisted 3/6",
  distance-from-capital bar, 3 candidates (portrait, name, stars, traits, cost), OK/X. Its bottom is
  ~55 px above the HUD band (our note said "touching the band": recheck).
- o6 **Research and Technology**: docked top-left like o3; Educational Buildings (Oxford); Civil /
  Military / Industrial columns; blue lines only link techs. Our thin blue line across the title
  (x ~278 of 1280) is a bug.

Refactor (from the walls review, pre-existing on main): the campaign model addresses building slots
as `Option<usize>` (None = the road/fort slot). Replace it with a slot enum so no sentinel values
remain (CLAUDE.md "no small ID types").

Remove leftover save-compat tests (user, 2026-10-07: loading the original's saves is out of scope):
delete the tests that read the user's save folder (`ntw_campaign` `tests/save_compat.rs`,
`user_saves_round_trip_byte_exact` etc.; they also flake when workers run them at once). KEEP all ESF
reading used for game data (`startpos.esf`, packs, mod overrides): mods from the original must keep
working.

Other campaign: region labels draw on top of UI panels (must sit under the HUD); the region details
title reads "XXX Details" (missing loc key, should be the region name); the user couldn't find the
demolish button (offered from a building slot via `CanDemolishBuilding`, check it shows).

Battle (`--battle-key NHB_Austerlitz --skip-deployment`):
- Unit cards hidden ~3 min in Austerlitz only: likely its intro cutscene hides the HUD and we run
  the cutscene timer without playing it. Play or skip it the way the original does.
- Smoke looks like flat floating cards: compare the original's per-particle size/rotation/alpha
  curves, frame animation and soft (depth-fade) blending.
- No muzzle flashes: the `muzzle_flash` names are exe-side and reach no effect group. Find the
  exe's rule. `MUZZLE_HEIGHT` 1.35 m is PROVISIONAL.
- No artillery crews drawn at the guns (possibly an old gap).
- Shrubs don't sway (trees do); "shrubs sway with the battle wind" is INFERRED and fails in game.
- Soldiers jitter when walking: FIXED, merged 2026-10-07 (first bad commit `e5c08bc`: the view estimated speed per frame while the model moves per 0.1 s tick; now per tick). Checked by the user 2026-10-07: jitter gone. Separate: units still look a little laggy (0.1 s steps, BACKLOG §3 interpolation). Bisect Austerlitz at `760ff03`, `ffe3f32`,
  `06d6a7e`; suspect the experience fatigue term flipping speed or gait level.

## Who works on what

- **Claude (local):** the three campaign blockers above, then §0 fidelity (0-A battle, 0-B campaign),
  in-game checks, reviewing and porting sandbox work.
- **Free model (sandbox, `.opencode/agents/`):** §2 battle effects, 0-E UI leftovers, 0-G character
  hooks, 0-D units. It works on sandbox branch `next`, a copy of main. To port: review
  `git diff <main commit next started from> sandbox/next`; afterwards the sandbox manager recreates
  `next` from the new main.

## Needs an in-game check

Batched for the user. Delete a line once checked and record the result above.
Base command: `cargo run -p napoleon -- <flags>`.

- **UI side-by-side (merged `53361ec`), British campaign, compare with the original:** `--campaign mp_eur_napoleon --campaign-faction britain --no-intro`, then (1) open Technology: no line across the title; (2) select Wellesley: Army | Recruitment tabs, his portrait first (the Recruitment tab is empty for now: PLACEHOLDER); (3) open Lists: generals show portraits, colonels the unit card, and the panel docks top-right on a wide screen.
- **Attribute icons (merged `25a6b21`):** general cards checked 2026-10-07 (match, see Open bugs).
  Still to check: an agent's card (spy: spying picture) and the agents-tab recruitment rows, against
  the original. `--campaign eur_napoleon --campaign-faction france --no-intro`.
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

- **Raw Ghidra dumps in git history.** 322 dumps were pushed and later untracked (`c8deabf`), and 65
  more were untracked on 2026-10-07. They stay in history; purging needs a history rewrite and
  force-push, the user's call.
- **Stray branch to delete:** `origin/work/fidelity-campaign-w4` is superseded (main has later
  versions of its two docs), but auto mode blocks remote deletes, so the user runs
  `git push origin --delete work/fidelity-campaign-w4`. Same branch on the sandbox. The remote URL
  still points at the old repo name (GitHub redirects); repointing it is also the user's to run.
- **Debugger session:** who writes `unit+0xD48`, where `unit_scale` is read, formation radius
  `+0x670`, garrison cap `+0x6C`.

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

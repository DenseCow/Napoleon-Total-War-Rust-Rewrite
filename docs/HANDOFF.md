# HANDOFF — current state

Keep this file short. It holds what is true now: replace finished items rather than appending logs.
Old session logs are in `docs/archive/`.

Last updated: 2026-10-08. Main tests at last full run (2026-10-08): `cargo test --workspace` passes on the install.

## Resume here (2026-10-08)

- **Repos:** work happens in the private repo (origin, `…-Private`). The public repo
  `DenseCow/Napoleon-Total-War-Rust-Rewrite` is a snapshot mirror for contributors: publish after a
  merge with `bash tools/publish_public.sh`; contributor PRs come back with `… import` (CLAUDE.md Hard
  rules). Its CI (`.github/workflows/ci.yml`, Windows, no game install) had its first run on 2026-10-08;
  publishing again cancels a running CI run, so batch publishes.
- **Workflow per branch:** finish → `/code-review medium` (fix commits only after the first round) →
  merge, applying the worker's BACKLOG ticks/Polish lines in the merge commit → `bash tools/progress.sh`
  → push (the hook syncs public #2) → publish in batches. Running work = the BACKLOG `(running: …)`
  markers and `git worktree list`; merged work = `git log`.
- **Stopped at the usage cap (2026-10-08 night), resume in this order:**
  1. `work/diplomacy2` (NR-diplomacy2, head `7b143591` WIP: builds, tests not updated). Rules traced and
     CONFIRMED (UI_FIDELITY.md §4.7). Done in code: regnal numeral, Power/Wealth words, button lists.
     Next: in `ui/campaign/diplomacy.rs` `post_negotiation_started` pass the greeting from
     `model.negotiation_greeting` (not ""), fix `MinisterPortraitPath`, update the stale test
     `faction_rankings_are_strings`, a test per difference, clippy 0, full tests; then the first
     `/code-review medium` of `870d6f19...`, merge, in-game check vs the three original screens.
  2. `work/campaign-0b` (NR-campaign-0b): see its last commit message / report for where it stopped.
  3. `work/polish-hotpaths` (NR-polish-hotpaths, uncommitted start in ntw_data/src/record.rs).
- **Next:** §0, several workers tracing in parallel under the Ghidra writer lock; the unwrap audit last.
- Evidence saves live in `%USERPROFILE%\Documents\ntw-evidence\saves` (never inside a target folder);
  recovered build-folder data in `ntw-evidence\recovered-targets\`.
- Usage (user plan, 2026-10-07): one 11-point block a day (~12 with the reading lag), until the weekly
  reset Wed 2026-10-14 00:00; `BlockPercent` in tools/usage_budget.ps1.

## Open bugs

- **Diplomacy screen (§0-E):** close button and "Test" treaty FIXED (user check 2026-10-08: X closes,
  reopen works, Current Treaties reads "At war"). Side by side with the original (Britain → France →
  Open Negotiations, Early January 1805), still different: (1) Power / Wealth are blank; the original
  shows word ratings ("Terrifying" / "Spectacular" for both); (2) the left panel's action buttons are
  missing: Present State Gift, Regions, Technology, Payments, Request peace (these fill Your Offers /
  Your Demands, which start empty in both); (3) the opposing diplomat's speech bubble with portrait
  ("Our dread sovereign has little time to consider your mewlings, so let us be brief: speak!") is
  missing; (4) the leader name reads "George", the original "George III". Same in both: flag rows,
  Current Treaties, public opinion, the three bottom buttons. Screens in
  `%USERPROFILE%\Documents\ntw-evidence\screens\`: `2026-10-08_ours_diplomacy_after_close_fix.png`,
  `2026-10-08_original_diplomacy_negotiation_britain_france.png`, and the original's Diplomatic
  Relations list `2026-10-08_original_diplomatic_relations_britain.png` (compare ours against it too).
  A second original screen, Britain → Ottoman Empire (at peace, trade agreement),
  `2026-10-08_original_diplomacy_negotiation_britain_ottoman_trade.png`, shows the button list depends
  on the relationship: at war = Present State Gift, Regions, Technology, Payments, Request peace; at
  peace with trade = Cancel Trade Agreement (red text), Request Alliance, Present State Gift, Declare
  war, Joining Wars (looks greyed), Trade Embargoes, Breaking Alliances, Military Access, Regions,
  Technology, Payments. Ratings there: Ottoman Power "Mighty", Wealth "Rich" (Britain "Terrifying" /
  "Spectacular"). The greeting follows the attitude (Friendly Ottomans: "Welcome! God willing, our
  friendship will blossom as a result of this meeting."). Leader "Selim I". Public opinion towards
  them "Indifferent", towards us "Friendly". Current Treaties "Trade agreement".
  Third original screen, Britain → Austria (ally),
  `2026-10-08_original_diplomacy_negotiation_britain_austria_ally.png`: buttons Cancel Trade Agreement,
  Cancel Military Access, Cancel Your Alliance (all red text), Present State Gift, Declare war, Joining
  Wars (greyed), Trade Embargoes, Breaking Alliances, Regions, Technology, Payments (no Request
  Alliance / Military Access: a held treaty swaps its request for a red cancel). Current Treaties:
  Military alliance, Trade agreement, Grants military access (indefinite), Has military access to your
  lands (indefinite). Power "Terrifying", Wealth "Rich", leader "Franz I", opinion "Very friendly" both
  ways; greeting "Valued friends of our gracious sovereign are always welcome. What matters do you wish
  to discuss this fine day?" (same diplomat portrait as the France screen).
  Trace in Ghidra what feeds each (rating thresholds, button visibility and red/grey states, greeting
  line choice, regnal number) before changing it; §0-E, next free slot.
- **Campaign:** region labels draw on top of UI panels (must sit under the HUD); the region details
  title reads "XXX Details" (missing loc key, should be the region name); the user couldn't find the
  demolish button (offered from a building slot via `CanDemolishBuilding`: check it shows).
- **Battle** (`--battle-key NHB_Austerlitz --skip-deployment`):
  - Unit cards hidden ~3 min in Austerlitz only: likely its intro cutscene hides the HUD and we run the
    cutscene timer without playing it. Play or skip it the way the original does.
  - Smoke looks like flat floating cards: compare the original's per-particle size/rotation/alpha curves,
    frame animation and soft (depth-fade) blending.
  - No muzzle flashes: the `muzzle_flash` names are exe-side and reach no effect group; find the exe's
    rule. `MUZZLE_HEIGHT` 1.35 m is PROVISIONAL.
  - No artillery crews drawn at the guns.
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
- **Free model (sandbox, `.opencode/agents/`):** §2 battle effects, 0-E UI leftovers, 0-G character
  hooks, 0-D units, on sandbox branch `next`, a copy of main. To port: review
  `git diff <main commit next started from> sandbox/next`; then the sandbox manager recreates `next`.

## Needs an in-game check

Batched for the user. Delete a line once checked and record the result where it belongs.
Base command: `cargo run -p napoleon -- <flags>`.



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

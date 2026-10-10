# For the user

The morning report from night sessions, newest night first. Two lists per night:

- **Needs you:** in-game checks, debugger sittings, decisions. One line each: what, why, exact command.
  Tell Claude the result; it records it where it belongs and deletes the line.
- **Done tonight:** everything finished, merged or settled. One line each: what, commit, how to see it.
  Claude clears a night's list once the user has read it.

## Night of 2026-10-09 → 10

### Usage tonight

- Night stop at **60.0%** of the week (block cap raised to 61.5% at your request; night mode stops 1.5 below).
  Then Sat/Sun/Mon at 11 points each leaves ~6 for Tuesday before the Wed 00:00 reset. Start each day around 2 pm.

### Needs you

- **New-campaign army cap in the original (~5 min, no debugger):** in the original, set the unit size option to the
  one that writes `campaign_unit_multiplier 0.75` (check `%APPDATA%\The Creative Assembly\Napoleon\scripts\preferences.script.txt`),
  start a new campaign (any faction), save on turn 1, and tell Claude the save's name. Claude reads its army/navy cap
  fields (CAMPAIGN_MODEL #23/#24; expected 20 / 10) to settle `ForceCaps::new_campaign` (PROVISIONAL, MODDING_AUDIT §2.6).
- **Decision to confirm (taken tonight, reversible):** trading regions/technologies is being wired into deals, but the
  AI's evaluation of such deals isn't traced yet. Until it is, the AI refuses any deal in which it would give away a
  region or technology (logged; you can still give yours). Say if you'd rather it be different.
- **Diplomacy negotiation (~3 min, side by side with the original):** `cargo run -p napoleon -- --campaign
  mp_eur_napoleon --campaign-faction britain --no-intro`, Diplomacy → Austria → Open Negotiations: the option buttons
  now appear; Regions and Technologies list both sides. Proposing regions/techs is not in the model yet (dropped, logged).
- **Region details panel, side by side with the original (~3 min):** `cargo run -p napoleon -- --campaign eur_napoleon
  --campaign-faction france --no-intro`, open a region's details (e.g. Paris): taxes, religion, public order, wealth
  and growth with their factors, predicted values. Compare with the original's same region on turn 1. Effects and
  NextTown are still placeholders.
- **Own save format + map display (quick look, ~3 min):** same command; play two turns, recruit a general, press F5,
  quit, load that save from the menu: the map looks as before, the general keeps his name, nothing is missing.

### Done tonight

- **Walking-horse sitting done (2026-10-10 afternoon):** the walk is slowed by the ground type (0.40 on that ground) and slope, not by turning; notes on `work/gait-blend2` `ab13a8ec`, port next.
- Gait debugger sitting: walking horses move at ~80% of their walk speed (why ours trot); men's levels via store C.
  Notes on `work/gait-blend2` `38df2276`.
- Austria power sitting: the original's per-faction power values (Austria 3rd = "Terrifying"); `ab028f7c`.
- Plan: modding foundations (one table path + guard test, no engine limits, generic switches) go first in free
  slots; CLAUDE.md "Modding seams" rule; `e29a33d5`. `$WrapAt = 1.0` committed (`169baff2`).
- **Mod loading merged** (`b47aff74`): packs, loose files and the `mods\` folder load with the original's precedence;
  two original-game bugs fixed (a repeated precedence line cancelled itself; an excluded dependency stopped start-up).
  See it: `cargo run -p napoleon -- --list-mods`.
- **Polish cleared** (`a7a1eb4c`): six cleanup lines (faster battle unit lookups, no double mod planning, logs once).
  Nothing visible to check.
- **Region details panel + population growth merged:** growth and religion conversion now run in the model (checked
  against all 422 regions of 7 original saves); religion shares no longer lost on save; ORIGINAL BUG fixed
  (DestroyChildren could loop for ever). See "Needs you" above.
- **One table path merged** (`cdcf2bd7`): every game table is read through one reader, mods included; a guard test
  stops new code reading table files directly. Vanilla reads unchanged (35 tables compared). Nothing visible to check.
- **Campaign source merged:** F5 now writes our own save format; the campaign map display no longer depends on the
  original's map files (only the importer reads them) — the first step toward custom maps. Character and officer
  names now live in the model (the exe's historical re-pick ported). See "Needs you".
- **Second Polish batch merged** (`1c58e28c`): script tables cached, clippy now clean across the workspace.
- **Austria now reads "Terrifying"** (`13b5581c`): our power formula already matched the original; the test had
  loaded France's start instead of Britain's Coalition. All 12 factions' power now match the debugger values.
- **No engine limits merged** (`885d889e`): wide ID types; army/navy caps, custom-battle unit counts and
  reinforcement limits are moddable data (`_kv_rules`), defaults CONFIRMED from the exe; no player settings added.
  The new-campaign navy cap is the one PROVISIONAL left (see "Needs you").
- **Generic engine merged** (`00431cb9`): the Napoleon-campaign switches are one per-campaign feature table, unit
  categories come from one place, seasons follow the month and unit upkeep is computed exactly as the exe does
  (land and naval listings). All three modding foundations are now in.
- **Recruitable population merged** (`17260a0e`): the exe's population gate and charge for recruiting (0 in vanilla
  data, so unchanged for vanilla; a mod can set it).
- **Diplomacy buttons merged:** the negotiation panel builds its option buttons (our `SetState` returned nothing),
  tradeable regions and technologies list both sides, treaty lines fixed. See "Needs you".
- **Recruit training block merged** (`8cb8f2c4`): a queued unit waits while its building is damaged or occupied or
  its tech is missing (as the exe); the recruit list is sorted as the original's; recruit prices are fixed at queue
  time (13 of 14 save mismatches explained).
- **Deals with regions and technologies merged** (`1c1e2400`): Propose builds them in the model as the exe; capture,
  liberation and deals share one owner-change rule (no army moved, as the exe); the AI refuses to give regions/techs
  until its evaluation is ported (worker on it now). **Polish-battle merged** (`88a10376`).
- **ui-small merged** (2026-10-10 noon): map labels follow the original's visibility rule and its labels setting; `Adopt` takes the index. deal-ai (the AI's deal valuation) is still being traced (HANDOFF 3b).
- **Capture refunds merged:** a captured port's queued ships are refunded to the old owner, land units and construction cancelled; a deal or liberation refunds an AI owner's whole queue, a human's nothing (as the exe; 36 captures in 12 original saves match). Nothing visible to check.
- **Public mirror published** (`28bcc8db`) with all of tonight's merges.
- **Running when this was written:** no-limits (ID types, counts, caps as moddable data), generic-engine (campaign
  feature table, unit category lists from data, calendar), gait-blend2 (why walking horses trot).

## Night of 2026-10-07

### Needs you

Base command: `cargo run -p napoleon -- <flags>`. The full check list is under HANDOFF "Needs an in-game check".

- **Attribute icons, agents:** (generals done 2026-10-07: match) `--campaign eur_napoleon --campaign-faction
  france --no-intro`, click a spy: its card shows the spying picture. Compare with the original.
- **Promote panel:** same command, select an army → Promote. Centred over the HUD, bottom on the band.
- **Agent actions:** same command, agent card → Assassinate / Duel opens a target picker; compare success %.

### Usage tonight

- Start: week 9.0% used; today's allowance 14% (night stop 12.5%). Weekly reset: Wed 2026-10-14 00:00.
- 5-hour window: 34% at 03:5x, refreshes 05:10. If it fills first, the session resumes at 05:13.

### Done tonight

The night stopped at 04:4x on today's allowance (week 11%). **No branch was merged**: all three
blockers went through many review rounds and each still had findings at the stop. Nothing new to
test on main tonight. Where each stands (details in docs/HANDOFF.md "Open bugs"):

- **Closing panels** (`work/blocker-panel-close` @ `f3b440f`): fixed in the real game (Lists and
  Technology close). One short final review, then merge.
- **Walls** (`work/blocker-walls`): walls are the last construction card, road on its own tab,
  damaged walls can't be upgraded, the AI builds and repairs walls. Was replacing the slot
  placeholder with a proper type (the cause of a crash) when the night stopped.
- **Battle music** (`work/battle-music-loop`): redesigned, looping music decoded fully before it
  plays; last review's 8 findings were being fixed at the stop.
- **Rules added:** code-quality rules, push guard (no pushes to main outside your PC), never change
  the branch of the main checkout, night resume rule (resume after a 5-hour-window stop, never after
  today's allowance), unattended debugger runs at night.
- **Settled:** "Small Star Fort" = walls level 1, last construction card (debugger + you). Recruitment
  tab bug confirmed in our build. Debugger launch fixed (working directory).


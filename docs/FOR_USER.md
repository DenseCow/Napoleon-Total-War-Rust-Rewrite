# For the user

The morning report from night sessions, newest night first. Two lists per night:

- **Needs you:** in-game checks, debugger sittings, decisions. One line each: what, why, exact command.
  Tell Claude the result; it records it where it belongs and deletes the line.
- **Done tonight:** everything finished, merged or settled. One line each: what, commit, how to see it.
  Claude clears a night's list once the user has read it.

## 2026-10-14 (night mode, 21-point block)

### Needs you

- **AI turn-change debugger sitting (~5 min):** settles whether a turn change refreshes the AI's remembered relations (INFERRED `0x00CF7F60` → `0x00CF0B10`). Claude launches the original under Ghidra's debugger; you load any campaign and end one turn.
- **Gait debugger sitting** (still open, see 2026-10-13 below).

### Done

- **recruit-gates merged** (`87acfa1f`): which units a region can recruit now follows the game's two hidden checks (regional unit resources, script restrictions); old saves still load. Nothing visible beyond the recruit list.
- **recruit-cost2 merged** (`6e277c42`): Spain's recruit prices now match the original (a saved +25 unit-category price bonus was being lost); 394 of 406 prices in your saves match, up from 392.
- **cai-belief merged** (`e9beb1a0`): the AI judges its rivals from its own remembered view of relations, refreshed only when the original refreshes it (deals, wars, captures, spy/assassination/sabotage missions). Nothing visible; shows in AI decisions.
- **polish16 merged** (`090d14b0`): a unit's men now start their stand, ready and aim animations together, as the original does (exe-confirmed). Quick look: `cargo run --release -- --battle-key NHB_Austerlitz --skip-deployment`, select a line unit and watch the men stand and aim in step.
- **treasury3 merged** (`d2e6598b`): fleets can repair damaged ships in port for gold, as in the original; moving, landing troops or fighting cancels the repair and refunds it. Nothing visible yet: the repair button is not wired to our UI (BACKLOG).
- **polish17 merged** (`109ba0a7`): Ghidra write-ups and notes; one battle animation input confirmed from the exe. Nothing visible.
- **recruit-followups merged** (`8ffe051e`): a mod unit naming an unknown resource is left out of every list, as in the original; walls effects on a region now count. Nothing visible in vanilla.
- **cai-broadcasters merged** (`33fbcc74`): making a faction a protectorate now ends its old ties and alliances as the original does; the Napoleon campaign's loot-resentment rule (abandoned ally, -50) now applies, read from data. Shows in diplomacy after a protectorate deal.
- **polish18 merged** (`2ac7fa83`): saved bonuses naming an unknown entry now resolve as the original does; Ghidra write-ups. Nothing visible.
- **treasury4 merged** (`67d58909`): the AI repairs its damaged fleets in port as the original does, and buys nothing while the tutorial's scripted shroud is on. Nothing visible directly.
- **polish19 merged** (`8f72788b`): a ship-repair test, Ghidra struct sizes, review leftovers. Nothing visible.
- **construction merged** (`f379268c`): buildings now finish at the end of the round, before diplomacy and research, as in the original; you can't queue a building in a slot something else holds; a captured region's slots pass to the new owner by the original's rule; saves keep slot holders. Quick look: queue a building, end turn, it completes on the right turn.
- **deals-fix merged** (`9af968b3`): a trade embargo in a deal now falls on the right side, as in the original; a payment you propose is capped to what the payer can afford and the row shows the capped amount; fixed an original bug where a faction in debt turned a capped offer into a demand. Quick look: in diplomacy, offer more money than you have; the row shows your treasury.
- **formation merged** (`6f89b634`): battle units close up their ranks after losing men, and a unit's width and depth come from where its men stand, as in the original. Quick look: `cargo run --release -- --battle-key NHB_Austerlitz --skip-deployment`, let a line unit take casualties and watch the gaps close.
- **polish20 merged** (`bc1dc7bf`): saved religion-and-class bonuses keep both of their values, as in the original; Ghidra write-ups. Nothing visible in vanilla saves.
- **broadcasters2 merged** (`172815ea`): a faction that loses its capital moves it to another settlement; with none left the faction is destroyed, and liberating one of its regions brings it back, as in the original. See it: take a small faction's last settlement in a campaign, then liberate it.
- **formation3 merged** (`90fa3a76`): when a unit re-forms or moves, each soldier walks to the nearest free spot in the formation instead of jumping into place; units reform at a walk or a run as in the original. Quick look: `cargo run --release -- --battle-key NHB_Austerlitz --skip-deployment`, order a line unit to turn or move and watch the men walk to their places.
- **construction2 merged** (`dccf6500`): damaged buildings repair over turns and buildings can be demolished, as in the original; repairs and demolitions in original saves now load. Our own saves are now version 4; a version-3 save made mid-turn between 22:51 and 23:51 on 2026-10-14 finishes its buildings one turn early (nothing lost). Quick look: load a campaign save with a damaged building and press repair, or demolish a building in a settlement.
- **polish21 merged** (`4a54c4cc`): when a fleet leaves port (sails off, is destroyed, or its admiral dies), the port goes back to the region's owner, as in the original. Deal payments, repeated negotiation text and Ghidra notes were also tidied. Logic only, covered by tests; nothing to look at.
- **director2 merged** (`4cd620ea`): the campaign AI now sees fleets at sea in their sea region, as the original does, and its region hand-overs keep agents in place (traced). Logic only, covered by tests; nothing to look at.
- **formation4 merged** (`e421bc5a`): idle units re-form only when the original's rule says so, and a unit's training level now feeds its morale and drill as in the original; a mod can add training levels. Quick look (optional): `cargo run --release -- --battle-key NHB_Austerlitz --skip-deployment`, leave a line idle after a move and see it settle into its ranks.
- Night stopped on the block cap (week 20%). Left unfinished, to resume next session: broadcasters3, transfer3, formation5, polish22 (docs/HANDOFF.md "Resume points"). Nothing needs you beyond the debugger sittings above.

## 2026-10-13 (usage stop at the weekly cap)

### Needs you

- **Gait debugger sitting (~10 min, BACKLOG "Gait: read n and k"):** Claude launches the original under Ghidra's
  debugger; you give one cavalry and one infantry unit a walk order in any custom battle. It breaks at `0x0054C691`
  and logs n, k, D, t, then watches `0x00659600` read descriptor word 9. Settles `TIMED_MOVE_PLANNED_SECONDS`.

### Done

- **Merged and pushed:** treasury2 (`ed17736a`), payment-capital (`321e584d`), polish15 (`a1f306e4`), gait-trace.
  Nothing new to look at in game (battle and campaign rules under the hood).

## 2026-10-11 (store run, night mode until you're back)

### Needs you

- Nothing open.

### Done

- **Sea merged** (`47cade7a`): the battle sea uses the game's own wave pictures and sea tables, and reflects the sky
  colour; see "Battle sea" above.
- **Cavalry gait merged** (`f48faac9`): three animation hitches fixed (a loop ran one frame too long, a change drew one
  frame past its end, the stride restarted on each change); see "Cavalry gait" above.
- **Wrapped up at 80%** (block cap). Pushed, not merged (HANDOFF "Resume points"): ui-fails (one small fix left after
  its second review: a mod's unit row must not wipe the unit's other rows), self-checks (3 of 5 settled: portraits
  correct; agent chances fixed for duels and building sabotage; fog labels confirmed), polish11 (partial).
- **Polish batch 9 merged** (`e7b96fcf`): Ghidra write-ups for the deal, region-deal and capture-refund functions;
  the exe checks no region owners in deals, so ours stays as is. Nothing visible.
- **Polish batch 10 merged** (`295505da`): one screenshot helper (a failed save now exits as an error), a clear type
  for port recruitment queues (save bytes unchanged). Nothing visible.
- **Region transfer merged** (`1eaf844a`): the campaign director that hands regions between factions now runs in
  the model as the original does (its save data is read and written too), with 4 bugs of the original fixed.
  Nothing to look at yet; the parts still PROVISIONAL are on the BACKLOG.
- New rule (yours): a fix round a review forces also fixes that review's small notes in the branch's own files (CLAUDE.md, `0dc70d12`).

## Night of 2026-10-10 (errands, from ~evening)

### Usage tonight

- Night wrap-up at **71%**, stop at **72%** of the week (block base moved to 62.5 at your request, 2026-10-10).

### Needs you

- Nothing open.

- (nothing yet)

### Done tonight

- **Treasury rules merged** (`5bd2d229`): money moves as the exe does. A player's pool hire or promotion can go into
  debt and the AI hires free; a state gift gives the receiver nothing (the exe's own behaviour); a deal's lump sum is a
  one-off payment (category 3 charge, category 1 credit). All tags cite exe addresses, review clean. Nothing visible to check.
- **Polish batch 4 merged** (`a15e21ac`): small clean-ups (one file system reused, no per-card allocation). Nothing visible.
- **Public mirror published** (`901c2e39`) with both merges.
- **Wrapped up at 71%.** Pushed but not merged yet (HANDOFF "Resume points" says how each continues): ui-fails (3 of 4
  failed checks fixed: recruitment tab, slot tooltip, unit-card error; the Enlist panel left), region-transfer (the AI's region hand-over: the campaign director
  is in the model with 4 original bugs fixed; tests and notes left), polish5 and polish6 (one review fix each),
  polish7 (Ghidra write-ups: mod loading and map labels done, generic engine partly).
- New rule (yours): the Polish slot never sits idle; when no Sonnet work is left it runs Opus (CLAUDE.md, `9eb742bb`).

## Night of 2026-10-09 → 10

### Usage tonight

- Night stop at **60.0%** of the week (block cap raised to 61.5% at your request; night mode stops 1.5 below).
  Then Sat/Sun/Mon at 11 points each leaves ~6 for Tuesday before the Wed 00:00 reset. Start each day around 2 pm.

### Needs you

- Nothing open.
- Moved to Claude's self-checks (2026-10-10): diplomacy negotiation, region details panel, own save + map display (still queued). The new-campaign army cap is traced and CONFIRMED (2026-10-12). The regions/techs decision is settled (deal-region, deal-ai merged).
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

- Nothing open.

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


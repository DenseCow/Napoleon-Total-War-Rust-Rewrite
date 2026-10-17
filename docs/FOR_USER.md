# For the user

The morning report from night sessions, newest night first. Two lists per night:

- **Needs you:** in-game checks, debugger sittings, decisions. One line each: what, why, exact command.
  Tell Claude the result; it records it where it belongs and deletes the line.
- **Done tonight:** everything finished, merged or settled. One line each: what, commit, how to see it.
  Claude clears a night's list once the user has read it.

## Debugger sitting sheet (one sitting covers every open read; updated 2026-10-15)

One session, about 30 minutes, when you have time. Claude launches the original under Ghidra's debugger with every
breakpoint set (never attached to a game you are playing) and logs the values; you only click. The breakpoint list
is ready: `analysis/fidelity/debugger/SITTING_SHEET.md`. Steps in order:

1. **Start-up and main menu (no clicks):** sound bank query watchpoint; DB row hash order. Claude may run this part alone at night.
2. **Campaign:** load any campaign, select a land army led by a general (its unit panel computes the promotion price;
   no promotion needed), then end one turn (settles whether a turn change refreshes the AI's remembered relations).
3. **Custom land battle:** Single Player → Play Battle → Land, map **Aosta Valley**, either side. On the armies page
   keep one cavalry and one infantry unit a side and give every unit one chevron. In the battle: walk order to the
   cavalry, then the infantry; run order to the infantry for ~3 s, then walk; let the infantry fire a few volleys; then
   select the infantry, hover one of the farmhouses in the middle of the map and right-click it to send the men inside.
   Settles: gait n and k (`TIMED_MOVE_PLANNED_SECONDS`), who writes unit experience `+0xD48`, the flag record name,
   animation cue dispatch, the soldiers per volley, drawing between ticks, the formation refresh serial, and the
   building garrison cap `+0x6C` (no siege battle needed).

Side-by-side looks (no debugger): the first full side-by-side run, once step 3 is done.

## 2026-10-17 (night mode)

### Usage

- Week at 30% at the start (03:09). Blocks raised from 12 to 16 points (your request) so Sat–Tue use the ~70 points
  left before the Wed 00:00 reset. Tonight's stop moved to 45.5% at your request (this block only; later blocks keep 16).

### Needs you

- Nothing new. The debugger sitting sheet above is still open.

### Done tonight

- Usage blocks raised to 16 points (`tools/usage_budget.ps1` line 22).
- **Battle morale knows who is attacking a unit and from where** (`9e0f975c`): the attacker list and front/flank/rear direction now come from the exe's own rules; three functions written up in Ghidra. Stays INFERRED until sitting sheet step 3. Visible in a battle as units losing more morale when hit in the flank or rear.
- **Diplomacy save reader/writer written up in Ghidra** (polish36 merge, `af69aa91` area): notes only, nothing visible in game.
- **Backstab penalty goes to the declarer's subculture** (`1624daae`): when a faction breaks a treaty by declaring war, factions of the declarer's own subculture now resent it, as in the exe (ours used the victim's). Visible in a campaign: break an alliance and check the diplomacy attitudes.
- **Battle AI and battle model share one attack-direction rule** (polish37 merge): the AI used its own copy; now it calls the model's. Nothing visible except an attacker standing on top of a unit now counts as frontal.
- **Battle movement class from the unit's category alone** (cavclass merge): as in the exe; elephants and ships get no gait modifier, a dismounted fixed unit walks as infantry. All 328 vanilla land units keep their old class (install test), so nothing changes in vanilla play.
- **Trade income kept per route, as in the exe** (routeacc merge): each trade route keeps its own running total instead of one per faction pair; our saves move to version 7 and older ones load without loss. Visible in a campaign: the trade overlay's per-route values. Commerce raiding is a new open item.
- **Ghidra write-ups for three battle squad-summary functions** (polish38 merge, 1aad88c1): notes and names in the exe project only; nothing changes in play.
- **Battle movement class reads the shared unit-class codes** (polish39 merge, dcc2f5c0): code clean-up and a corrected note on elephants; nothing changes in play.
- **Turn passes over factions out of the game** (deadqueue merge, aaa5c85d): as in the exe (`0x00948CF0`), a faction out of the game gets no turn, and director transfers queued for it wait until it re-emerges. Ending the turn with every human faction destroyed no longer hangs; the exe ends the campaign there, which is now running as its own item (campaignover).
- **Alliance break and lost-access messages** (allybreak merge, 960ecd7f): breaking an alliance between two major factions messages every major human, and cancelling military access messages the target, as in the exe. Our map shows no message feed yet (open item), so nothing is visible in play.
- **Unit class codes live in one place** (polish40 merge, ed82a6b0): the battle AI reads its class numbers from the shared unit-class module; code clean-up, nothing changes in play.
- **Campaign save naming and save/load shortcuts** (savename merge, f5046aa0): saves get the original's default names, the save panel asks before overwriting (our saves and the original's), CTRL+S / CTRL+L / ESC work, typing a name no longer moves the map, and scripts can switch saving off (`disable_saving_game`). Visible in a campaign: open the save panel from the menu and save. Left open: one untraced part of when saving is allowed.
- **Trade: routes built once, home term needs a domestic entry** (polish41 merge, 3a928db3): faster trade sums, and as in the exe (`0x00BB3490`) a faction with no domestic trade entry gets no home-trade term. In vanilla only one faction has no such entry (`spa_france`), so its trade income can differ slightly from before.
- **Military access loss pushes armies out** (accessloss merge, b6a5639b): when a war, a broken treaty or peace ends military access, the faction's armies are moved out of the other faction's lands, as the exe does (`0x00B29170`, `0x00B67BD0`, `0x00B0CA20`, `0x00B13A30`, `0x00B0D0C0`). Logic only, covered by tests; the order regions are walked in is still a stand-in.
- **Campaign ends when the player is destroyed** (campaignover merge, b5af9b4f): destroying the human faction sets the campaign-over flag (campaign +0xFE9, saved) and the turn loop stops, as the exe does (`0x008F419E`); the AI's remaining orders that frame stop quietly. Logic only, covered by tests. The defeat screen is not built yet, so the map just stops; that is a BACKLOG line.
- **Battle soldier notes corrected** (polish42 merge, f8a4abb1): Ghidra notes on the squad summary records; the soldier's squad-box radius is its own size at set-up but the exe grows it for objects with attached entities (`0x00607540`), now tagged and listed for tracing. Notes and comments only, no game change.
- **Small clean-ups** (polish43 merge, 3e733313): the access-restore rule now lives once in treaties.rs; a save log message and a save test made accurate. No game change.
- **Trade offers respect scripted diplomacy permissions** (dipperm merge, d3e0e5d7): the diplomacy screen's trade check now reads the script-set permissions as the exe does (`0x009F33E1` / `0x00C1A7D0`), and the relationship line names "protectorate" only for the player's own protectorates. The AI's own checks are traced but not ported yet (BACKLOG). Quick look: start a campaign, open Diplomacy, pick a faction; trade shows as allowed or blocked.
- **Turn order fixed** (turnorder merge, bf914e31): the rebels now take their turn after every listed faction, and round-start events skip factions that are out of the game, as `0x0096C050` / `0x00948CF0` do. Covered by tests; no visible change beyond end-turn order.
- **Ghidra names** (polish44 merge, e979aaf2): the military-access turn counter and the region-expulsion function are documented in Ghidra and the notes. No game change.
- **Small clean-ups** (polish45 merge, 206a3ad9): the access give-back rule and the trade-held check each live in one place now; three notes worded exactly. No game change.
- **Diplomacy screen stances fixed** (dipstance merge, e646fc13): the patron / protectorate lists were swapped and are now as in the original (`0x009B2690`), the military-access mark sits on the protectorate, and the treaty list gains the "protectorate of …" line (`0x00B750D0`). Quick look: in a campaign with a protectorate, open Diplomacy and select the protector and the protectorate.
- **Ghidra write-ups** (polish46 merge, cc74edfe): the path-grid refresh calls after an access change, the faction registry and three AI/diplomacy functions are named and typed in Ghidra (scores 72–97); notes only. No game change.
- **Worker self-check tightened** (polish47 merge, ccd9ad42): `tools/self_check.sh` now also checks Ghidra functions the notes name without an address, fails on every lookup error, and a rename by another worker is excused only by an explicit, printed `--not-mine 0xADDR`. Tooling only.
- dippermai merged (287e46a0): the negotiation panel's trade button explains why a trade agreement can or can't be made (routes left, embargo, no route), from the game's own text; see it on the campaign diplomacy screen → negotiate → hover Trade agreement.
- **Campaign date text from the game's strings** (polish48 merge, 028e071a): the HUD date ("Early January 1805") and the default save name now use the original's round_id text; see it on the campaign map's top bar.
- **Diplomacy screen follow-ups** (polish49 merge, 6b48c3a3): a patron missing from the world is logged once and left out instead of a blank name; Ghidra write-ups for the diplomacy stance functions. Nothing to see in game.
- **Round start like the original** (roundstart merge, 52b05225): at the start of each round only factions still in the game get their regions refreshed (rebels skipped), rebel relationships are no longer reset, and trade prices update at round start too. Logic only, covered by tests.
- **War declarations: refusing allies** (polish50 merge, 871e04fa): an AI ally of the attacker that refuses the call to war now keeps its alliance, as the original does; notes on peace and war-declaration functions corrected. Logic only.
- **Commerce raids on trade routes** (traderaid merge, 5248efa3): a fleet raiding a sea trade route cuts that route's income each turn (three-term formula 0x00B60A40); the raids pass to the next commander when the admiral dies or escapes, and the round start checks them too. See it: park a fleet on an enemy sea trade route in a war, end the turn, and the enemy's trade income drops.
- **Diplomacy screen empty cases** (dipempty merge, ebe286c8): the diplomacy details, treaties and access-turns calls now return nothing for an unknown or empty faction key (the rebels never match) instead of the player's own data, as in the original. Logic only, covered by tests.
- **Zones of control around walls** (zocflood merge, 6fe8905d): an army's zone of control now spreads around obstacles the way the original's does, is built once at map load, and is rebuilt only when something changes it (a move, a death, war or peace, a region changing hands). See it: move an army next to an enemy force beside a river or a city wall and compare where its movement stops with the original.

## 2026-10-15 (night mode)

### Needs you

- Nothing new beyond the debugger sitting sheet above (its breakpoint list is still being prepared).

### Done tonight

- **Section 0 sorted to its exit bar** (`97d48483`): 22 items that change play stay; 40 deep traces, debugger reads and tidy-ups moved to their feature sections. See docs/BACKLOG.md "From §0" blocks.
- **One debugger sitting sheet** (`91302c98`): every open debugger read in one ~30-minute session, top of this file.
- **Pre-report self-check** (`2f78f587`): `bash tools/self_check.sh` catches functions renamed in Ghidra without a write-up and stale comments before a branch is reviewed. Nothing visible in game.
- **Mod-loading functions written up in Ghidra** (`2715a3b4`): the functions that load mods and the character constructors are fully documented; a wrong description of one global in analysis/campaign/MOD_LOADING.md was fixed. Nothing visible in game.
- **Trade routes break when a port changes hands, and threats/declines count in diplomacy** (`68f6a79b`): a captured port cuts the old owner's trade route; a threatened faction remembers it when it accepts or declines a deal. Visible in a campaign: take an enemy trade port and watch its route vanish from the trade overlay.
- **Two speed-ups** (`polish24`, `polish25` merges): battle skirmishers no longer build a new list every tick; the campaign director checks for human armies only where it needs to. Nothing changes in play.
- **Debugger sitting sheet ready** (`abc61cc7`): every open read in one ~30-minute session (start-up, campaign, one custom land battle on Aosta Valley; no siege needed). Top of this file; details in `analysis/fidelity/debugger/SITTING_SHEET.md`. Two reads were settled from the exe without a sitting.
- **Armies leave a region that changes hands** (`b1ff0c35`): when a region is handed over (deal, liberation, a battle capture, the end of a protectorate), other factions' armies there are moved out as in the original, and docked navies when not at war. Visible in a campaign: give a region with a foreign army in it away in a deal and watch the army step outside the border.
- **Battle formations laid out like the original** (`389fa114`): units march and stand in rows filled from the centre outwards the way the exe does it, with the diamond and wedge shapes and the light-infantry stagger. Visible in battle: `cargo run --release -p napoleon -- --battle-key NHB_Austerlitz --skip-deployment`, order a line or a cavalry wedge.
- **Campaign tests split by topic** (merge after `81e0b236`): the 6,100-line campaign test file is now 12 topic files; nothing to see in game.
- **Trade and diplomacy functions named in Ghidra** (`031c972b`): notes and Ghidra only; found that breaking trade should drop only the first route each way (BACKLOG §0).
- **A region the director hands over gets its garrison army** (`c93ea6a8`): when the director transfers a region, the new owner gets the army the original raises from the garrison count (units picked from what the region and its neighbours can recruit, infantry first). Visible in a campaign when a region is transferred by the director.
- **Breaking trade drops only the oldest route each way** (`9c6853cf`): war, an embargo or breaking trade now removes only the first trade route in each direction, as the original does; any other routes stay. Visible in the trade panel after breaking a deal with a faction you have two routes to.
- **Three small tidy-ups** (`polish28`, `polish29`, `polish30` merges): campaign notes re-wrapped; armies pushed out of a region after it changes hands no longer scan the whole map each. Nothing changes in game.
- **Scripted UI pieces land where the script puts them** (`67ab1350`): CreateFromLayout now uses the x, y a script passes (battle pip frame, xp bling, escape-menu options were stuck at their file offsets); a script passing a wrong type gets nil and a log line. Fixes a bug in the original where one bad call silently broke every later call on that panel.
- **Squares and pike formations shaped like the original** (`c8ab2e98`): infantry squares and pike walls/squares now lay their men as the exe does; also fixes the original's squares sitting 1 m off-centre. Quick look: `cargo run --release -p napoleon -- --battle-key NHB_Austerlitz --skip-deployment`, order a line infantry unit into square.
- **Dismounted cavalry no longer counts as riding** (`0ac37bf4`): frightened horses, drill and ground speed now use the exe's "riding now" test, everything else its "mounted" test. Logic only, covered by tests.
- **Script image lists that mix styled and plain entries work like the original** (`91f2f632`): CreateFromComponent hands each image the list's current first entry. Logic only, covered by tests.
- **Port ship-repair button works** (`1c620ba0`): the navy panel's repair button, from the original script, now orders and cancels repairs through the model. Quick look: `cargo run --release -p napoleon`, load a campaign with a damaged fleet docked in a port, select it and click repair.
- **Charging cavalry frightens units as in the original** (`4560ad25`): the fighting-cavalry morale effect counts each attacker once, by direction, and only while it is riding. Logic only, covered by tests.
- **A first backstab now costs the peace-treaty penalty, as in the original** (`a6e10ca6`): the treaty-break counter starts at 1 and military-access abuse is counted; our save format goes to v6 and older saves are migrated. Logic only, covered by tests.
- **Trade routes left after a treaty break keep earning** (`c338f20c`), as in the original; one function now feeds income, accumulation and the trade panel. Logic only, covered by tests.
- **Port repair button checks each unit** (`53380b60`): a land unit inside a navy takes the land path, as in the original. Logic only, covered by tests.
- **Stopped at the block cap (week 30% of the 32% this block allows), 2026-10-16 ~01:40: no resume scheduled.** You start the next session; unfinished branches are listed in docs/HANDOFF.md "Resume here".

## 2026-10-14 (night mode, 21-point block)



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


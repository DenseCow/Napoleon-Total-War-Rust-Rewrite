# Debugger sitting sheet (one launch, ~30 minutes)

Every open debugger read, in the order of the user steps in `docs/FOR_USER.md` "Debugger sitting sheet". All
addresses are **static** and were re-checked in Ghidra on 2026-10-15 (instruction bytes at each site, callers).
Supersedes the battle plan on branch `work/sittings` (`1d08e25c`), the 0-G promotion probe and the reads listed in
the notes files below; those files keep the reasoning, this sheet keeps the breakpoints.

## 0. Launch and conventions

- Launch the original under Ghidra's debugger as in `docs/GHIDRA_MCP.md` "Live debugging" (offer `dbgeng extra
  options`, `cwd` = the install folder; never attach to a running game). The game process must see
  `_NO_DEBUG_HEAP=1` (start Ghidra from a shell that has it set, so the launched game inherits it): without it the
  Windows debug heap makes loading crawl. Resume both loader stops; set `sxd` as in GHIDRA_MCP.md.
- Then: `.effmach x86`, `ghidradbg.commands.ghidra_trace_sync_disable()`, `.logopen <scratch>\sitting.log`.
- `D(x)` = the dynamic address of static `x`: `x - 0x400000 + base`, base of `napoleon.retail.exe` from
  `debugger_modules` (`debugger_static_to_dynamic` returned addresses unchanged on 2026-10-09). This applies to data
  addresses too (vtables, type descriptors).
- Commands are cdb syntax through `ghidradbg.util.dbg.cmd(...)`. The evaluator is C++: write `poi(...)` as
  `@@masm(poi(...))`, numbers as `0x...`, and no quotes inside a breakpoint's command string. Logging breakpoints end
  in `gc`; `bp /1` is one-shot. Counters: `$t0..$t9` (`r $t1 = @$t1 + 1`; `.if (@$t1 > 0n12) { bd <id> }`).
- `ba` (hardware watch): at most **4 at once** on x86; `ba r4` fires on read or write; it stops *after* the access,
  so `ub @eip L1` shows the accessing instruction. Budget per step is in each table's last column.
- No breakpoint below runs unconditioned every frame: per-frame or per-tick sites carry a hit counter that disables
  them, and each is armed only right before the user action it watches. `bc *` between steps.

## 1. Start-up and main menu (no clicks; may run unattended at night)

Arm right after the second loader stop, before resuming.

| # | BACKLOG line | Break | Log | Ours assumes | Expected hits | ba |
|---|---|---|---|---|---|---|
| 1a | §11 "DB row hash order and in-file duplicate keys" (row order) | exec, logging `D(0x00E786F7)` (in the units loader, ECX = key-map entry), **disabled at start**; `bp D(0x00E86AA0) "be <1a>; gc"` enables it at the loader's entry, `bp D(0x00E86AC9) "bd <1a>; gc"` disables it at the return | `dd @ecx+0xc L1` (row index), `du @@masm(poi(@ecx+8))` (key text; string object {len, ?, u16*}) | rows in file order (`ntw_data` record.rs, PROVISIONAL) | one per `units` row (~438), once | 0 |
| 1b | same line (duplicates) | exec, counter `D(0x00E782E3)` (the drop-the-new-row path) `r $t1 = @$t1 + 1; r ebp; gc` | `? @$t1` at the main menu | keep-all; vanilla has no in-file duplicate, so 0 | 0 | 0 |
| 1c | §9 "The sound bank query" (MIDDLEWARE_VERIFY §3 round 14) | exec, one-shot, stopping `bp /1 D(0x00DF3627)` (after `0x00DF3640` returns the database; one caller, `0x00DF3600` ← `0x00481020`). At the stop: `ba r4 @eax+0x74 ".catch { ub @eip L1; kb 6; dd <db>+0x74 L1 }; gc"` (bank 19, music) | each watch hit's instruction and stack; then `bc` it once the main menu music plays | weighted matcher (PROVISIONAL) | 1 write (bank factory `0x00E15D20` path) + the query's read(s) as the menu music starts; stop after 20 | 1 |

Notes: `0x00E778A0` (the generic-looking loader) has a single caller, `0x00E86AC4` in the `units_tables` loader
`0x00E86AA0` (name literal `0x013BD638`): 1a and 1b cover the `units` table only; every other table has its own
template instance (MOD_LOADING.md §3.1). MOD_LOADING §6 items 1–4 (`;`, scan order, header dependencies) need temp
packs and a swapped `user.script.txt`: they stay a separate unattended night run, not part of this sitting.

## 2. Campaign (load any campaign; select a land army; end one turn)

| # | BACKLOG line | Break | Log | Ours assumes | Expected hits | ba |
|---|---|---|---|---|---|---|
| 2a | §0 "Promotion price number" | exec, counter `D(0x008E2799)` (inside land slot +0x44 `0x008E2770`, EAX = record +0xC block; the army panel builder `0x009ABE00` calls the slot at `0x009AC5FF` for "PromotionCost", so **no promotion is needed**: select a land army with a general) `r $t2 = @$t2 + 1; .if (@$t2 > 0n20) { bd <2a> }; dd @eax+0x38 L1; dd @eax L4; gc`; plus counter `D(0x008E279D)` (no record, −1) `r $t3 = @$t3 + 1; gc` | the price `[+0x38]` per unit row, and `? @$t3` | the hire formula (`pool.rs` `promotion_cost`: 400 + 300 × rank + distance, PROVISIONAL) | one per land unit row of the selected army (≤ 20) per panel build | 0 |
| 2b | §6 "Stored CAI relation belief" (the turn-change mark) | arm just before End Turn. exec, logging `D(0x00CF7F60)` `dd @esp L1; r ecx; gc` (the mark; callers `0x00A761F0` ×3, `0x00A87F50`, `0x00A915C0`); exec, counter `D(0x00CCD019)` (the refresh's rebuild call vt+0x24, EBP = belief) `.if (@@masm(poi(@ebp)) == D(0x01378434)) { r $t4 = @$t4 + 1 }; gc`; exec, counter `D(0x00CCCF70)` `.if (@@masm(poi(@ecx)) == D(0x01378434)) { r $t5 = @$t5 + 1 }; gc` | the mark's callers in order; `? @$t4`, `? @$t5` when the human turn returns; `bc` all three | every relation belief is rebuilt at each handover (`director_handover`) | mark: one per faction turn; `$t5`/`$t4` vs the relation-belief count (factions alive × list size at faction +0x100) | 0 |

## 3. Custom land battle on Aosta Valley (one cavalry, one infantry unit a side, every unit one chevron)

Route: main menu → Single Player → Play Battle → Land; map **Aosta Valley** (`battles` key `NAP_MP_Valley_[SA]`,
preset `nap_mp_valley_[sa]`; its list flags equal the default map `NAP_MP_Amazon`'s, so it is offered); either side;
on the armies page keep one cavalry and one infantry unit per side and give **every** unit one chevron. Why this map
(install data read 2026-10-15, `battles`, the preset's buildings, `models_building`): seven of its nine
`west_euro_farmhouse*` buildings stand in the centre, each with 48–66 intact fire lines, so each gets a garrison object (`+0x1E8`) and is
defendable (`garrison.rs`); the nearest stand about 170 m ahead of the south zone's front edge and 260 m ahead of the
north one (1v1 areas at y = ±400, depth 200; farmhouses at (−106, −132), (−27, −111), (−77, 37), (−5, 4)). No
siege battle is needed. Arm 3a and 3b before choosing the battle, 3c in deployment, 3d–3j after the start.

| # | BACKLOG line | Break | Log | Ours assumes | Expected hits | ba |
|---|---|---|---|---|---|---|
| 3a | §2 "What string names a flag record (`+0x38`)" | exec, one-shot logging `bp /1 D(0x011CE5CA)` (after `MOV EAX,[EDI+0x2FB4]`, flag manager ctor `0x011CE120`) | `dd @edi+0x2FB0 L1` (count); for i = 0..7, rec = `@@masm(poi(@eax+4*i))`: `dc rec+0x38 L8`, `da`/`du @@masm(poi(rec+0x38))`, `dd rec+0x48 L1`, `dd rec+0x64 L1` | the key is the `flags.tai` entry named by `factions.flag_path` | 1 (if none by deployment: fallback `D(0x01206E80)`, `rec = poi(poi(@ecx+0x2FB4)+4*dwo(@esp+4))`) | 0 |
| 3b | §3 "Who writes `unit+0xD48`" | exec, one-shot, **stopping** `bp /1 D(0x0051B7D0)` (unit body ctor, ECX = body U). At the stop: `ba w1 <U+0xd48> ".catch { ub @eip L1; kb 8; db <U+0xd48> L1 }; gc"` | every write's instruction, stack and value | not modelled (`army.rs` Experience 0, PROVISIONAL) | ctor 1; writes 1–3 during load. The first body built may belong to any unit, so every unit has one chevron (route above): U's expected level is then 1 whichever it is; a write of 0 still shows the writer | 1 |
| 3c | (3b, 3h) | deployment: `db <U+0xd48> L1`, then `bc` the 3b watch. Find one cue container: `s -d 0 L?7fffffff D(0x0145B920)`, skip hits in the module; for a heap hit h, c = h − 0xC, keep it if `dwo(c)` ≥ 1, `dwo(c+4)` in 1..64; path `du @@masm(poi(c+0x10))`; pick a line-infantry musket reload/fire clip; E = `poi(c+8)` | U's level; c, E, `dd E L(2*count)` | — | — | 0 |
| 3d | §3 "Gait: read n and k at `0x0054C691`" | exec, counter `D(0x0054C6A8)` (before `t` is stored; EDX = body, EAX = out {D, t}, XMM0 = t) `r $t6 = @$t6 + 1; .if (@$t6 > 0n12) { bd <3d> }; dd @edx+0x27c L1; df @edx+0x13c L1; df @esp+0xc L1; df @eax L1; r xmm0; gc`. **User:** walk order to the cavalry, then to the infantry | n (living men), k (rank spacing), v, D, t (low float of XMM0) | t = 5 s (`TIMED_MOVE_PLANNED_SECONDS`, INFERRED); static says t = (n·k + 2v)/v | one per order issue (`0x00584E10`, callers `0x0051A8FD` and the 11-tick re-plan `0x00581B81`); stop at 12 | 0 |
| 3e | §3 "Units drawn between model ticks" (BATTLE_FIDELITY §59) | while the infantry walks: stopping one-shot `bp /1 D(0x0066EF8E)` → P = EDX (a moving soldier entity). Then `ba r4 <P+0xD4> ".catch { r eip }; gc"` for ~3 s of play, then `bc` | the logged eip of every hit, grouped (the watch stops after the access, so eip is the next instruction): the model tick's own root-motion update gives two per tick, its read `MOVSS XMM0,[EDX+0xD4]` at `0x0066EF6C` (eip `0x0066EF74`) and its store at `0x0066EF8E` (eip `0x0066EF96`), ~30 each at 10 Hz; every other eip is another reader, and the one that comes ~fps × 3 times in `0x0070xxxx..0x0079xxxx` is the render read | `view::DrawnPose` blend (PROVISIONAL) | ~30 at eip `0x0066EF74` + ~30 at `0x0066EF96` + fps × 3 render reads | 1 |
| 3f | §3 gait-blend "Ladder transitions" (UNITS_TERRAIN §1.9 "Next sitting": the man's clip stores) | exec, logging `D(0x00773D18)` and `D(0x00773D5C)` (the blended and the snap store; EDI = display, EAX = new clip): `.if (@@masm(poi(@edi)) == D(0x01341E20)) { r $t7 = @$t7 + 1; r edi, eax; dd @@masm(poi(@edi+0xd4))+4 L1; dd @eax+4 L1; dd @eax+0x1c L1; dd @edi+0xd0 L1 }; .if (@$t7 > 0n200) { bd <3f> }; gc` (man displays only; vtable `0x01341E20` is stored by `0x00717660`). **User:** run order to the infantry, ~3 s, then walk | per change: display, old slot `+4`, new slot `+4`, kind `+0x1C`, node `+0xD0`, which store | walk ↔ run = a blend over the line's blend-in, no transition clip | ~1 per man per gait change (~120 per order); stop at 200 | 0 |
| 3g | §3 "`volley_plan` runtime confirmation" | exec, counter `D(0x00539926)` (after `0x005694D0` returns n in the candidate gather `0x00539910`; EBP = the fire order, `[EBP+4]` = unit) `r $t8 = @$t8 + 1; .if (@$t8 > 0n10) { bd <3g> }; r eax; dd @ebp L1; dd @@masm(poi(@ebp+4))+0x18 L1; gc`. **User:** let the infantry line fire (default drill) | n, the order's vtable (`0x013239F0` = drill 0 `fire_volley`), men `+0x18` | n = files (front rank), capped at the men (`shooting::volley_plan` drill 0) | one per fire-order tick; stop at 10 | 0 |
| 3h | §3 "Animation cue → slot dispatch" | while 3g's firing runs: `ba r4 <c+4> ".catch { r eip; kb 5 }; gc"` and `ba r4 <E+4> ".catch { r eip; kb 5 }; gc"` for ~30 s (4–5 volleys and a march), then `bc` | eip and stack of any read | nothing reads the sound-side cue list in a battle (round-9 verdict) | 0 if the verdict holds; any hit is the dispatch | 2 |
| 3i | §0 "Battle formation left (formation4)": the alternate-tick parity serial (held by formation5; this only gathers data) | exec, counter `D(0x0053C1A9)` (EAX = summary `[EDI+8]`, ESI = battle model) `r $t9 = @$t9 + 1; .if (@$t9 > 0n16) { bd <3i> }; r edi, eax; dd @esi+0x58 L1; gc` | per unit: EDI, the parity value EAX, the tick `+0x58`, over two ticks | parity = our unit id | units × 2 ticks; stop at 16 | 0 |
| 3j | §3 "Garrison cap `+0x6C`" | exec, counter `D(0x00855509)` (in the capacity `0x008554F0` after the slot count `0x006F22C0`, ESI already popped: EAX = slots, ECX = T = `[this+4]`, the battlefield building record whose garrison object `+0x1E8` was just counted; not a wall) `r $t0 = @$t0 + 1; .if (@$t0 > 0n10) { bd <3j> }; dd @esp L1; r eax, ecx; dd @ecx L1; dd @ecx+0x54 L1; dd @@masm(poi(@ecx+0x54)) L1; dd @@masm(poi(@ecx+0x54))+0x6c L1; gc`. **User:** after 3i, select the infantry, hover one of the centre farmhouses until the cursor shows it can enter, right-click the farmhouse, wait until the men are inside | the caller (`[esp]`, the return address), slots, T and its vtable, P = `[T+0x54]` and its vtable, cap = `[P+0x6C]` | capacity not enforced (`garrison.rs`, PROVISIONAL) | one per capacity query; the four callers by return address: `D(0x0085557B)` add (`0x00855560`), `D(0x00855536)` remaining (`0x00855520`), `D(0x0059A5D1)` the UI (`0x0059A500`), `D(0x00615FCA)` the battle-script binding `building:capacity()` (code `0x00615FA0`, no function defined in Ghidra; method table entry `0x01451C10`, name string `0x013330AC`; it passes `[wrapper]+0x1F0` as `this`), which fires only if a battle script asks; stop at 10 | 0 |

Budget: 3b's watch is cleared in 3c; 3e (1) is cleared before 3h (2); never more than 2 at once.

After the sitting (3j): P's vtable names the class whose `+0x6C` holds the cap; its constructor's writer is then a static
trace (BATTLE_FIDELITY §54(3) / round notes: two unrelated `+0x54` fields were confused before).

## Settled statically on 2026-10-15 (left out of the sheet)

- **Order source at `0x006533E0`** (§3 gait line "Debugger sitting at `0x006533E0`"): `0x006533E0` has exactly two
  callers, `0x00585602` (in `0x00584E10`, the unit move order, CONFIRMED source of the walk and run) and `0x006DC7AF`
  (in `0x006DC700`, the artillery crew follow). No third source exists, so the sitting is not needed; details and a
  second, uncatalogued path into `0x006DC700` in UNITS_TERRAIN_FIDELITY.md §1.10.
- **Formation radius `+0x670`** (§3): written by `0x0053C3D0` (store `MOVSS [EDI+0x44],XMM0` at `0x0053C5FA`,
  summary `+0x44` = squad `+0x670`), already recorded 2026-10-14 (BATTLE_FIDELITY §6.1) and ported (`abilities.rs`,
  `march.rs`, `morale.rs`).
- **Battle HUD frame** (E of the old plan): CONFIRMED 1280x960 by the 2026-10-09 sitting (BACKLOG ticked).
- **Horse gait stores** (F of the old plan): done in the 2026-10-09 sitting; only the man's stores remain (3f).

## Results (fill in during the sitting)

- 1a: · 1b: · 1c:
- 2a: · 2b:
- 3a: · 3b: · 3d: · 3e: · 3f: · 3g: · 3h: · 3i: · 3j:

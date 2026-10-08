# 0-G promotion probe: what a field promotion charges (2026-10-05, hardened 2026-10-06, reviewed 2026-10-06)

The last open item of BACKLOG **0-G**. Two files, both tracked:

- `0g_promotion_probe.cdb.txt` — the cdb script the user runs (hand-written by us: breakpoints
  and log lines only, no exe output, no decompiled code).
- this sheet — how to run it and what to send back.

There is **no Python step and nothing to install** beyond `cdb` itself (it ships with the
Windows SDK; use the x86 one).

**It is read-only against the game.** Every breakpoint prints with `.printf` / `du` and
continues with `gc`; nothing writes game memory or registers. `cargo test -p ntw_data --test
probe_script` refuses any other command, at the top level or inside a breakpoint.

## Run it

1. From the repo root, create `target\tmp\gh\` if it is not there. The script's `.logopen`
   path is relative, so **run `cdb` from the repo root**.
2. Start the game and reach the campaign map. `-pn` attaches to a running process only, so
   the game must be running first.
3. Attach:

```text
cdb -pn Napoleon.exe -pd -cf analysis\fidelity\debugger\0g_promotion_probe.cdb.txt
```

Then, in the game:

1. **End turn** until your faction has `promote_general_in_field` (army) and/or
   `promote_admiral_at_sea` (navy). Without it the Promote button is never offered.
2. Promote **two land units of different types** (a line-infantry regiment and an artillery
   battery are ideal), each commanded by a plain colonel.
3. If you have one, promote a land unit whose colonel has command stars.
4. Promote **one ship** (the static reading says the price is 0).
5. Note what the tooltip showed for each. Quit the game.

Send back **`target\tmp\gh\promotion_probe.log`** — the whole file.

Two commands confirm the script is sound before you spend the sitting:

```text
cargo test -p ntw_data --test probe_script
cargo test -p ntw_data --test probe_install -- --ignored
```

The first checks the script against the address table and that it only reads; it needs
nothing but the repo. The second reads `Napoleon.exe` and confirms each breakpoint still lands
on the instruction it is documented to land on, and that the three cost slots' value / `-1`
return pairs are the eight bytes the recorded instructions imply; it needs the install. If the
first fails, do not run the probe. If only the return-pair check fails, send the failure text
back: the recorded disassembly is wrong somewhere and the PRICE line cannot be trusted.

## What it decides

| Log line | Question it closes |
|---|---|
| `ARMED <name>` | printed once as each breakpoint is **set** (a separate line after each `bp`), followed by cdb's own `bl` list. 21 in all. |
| `PRICE land=<n>` | **the price itself.** Two land promotions giving the same number means one figure per (agent type, faction subculture); different numbers means it follows the promoted unit. |
| `PAY amount=<n> reason=<r>` | what the treasury actually took. Must equal `PRICE`. `amount=0` after `EXEC-NAVAL` confirms a naval promotion is free. |
| `CHAIN key=... value=... rec=...` | the pointer chain behind the price, and which record it came from. The chain past `rec` is a guess; a broken one prints `????????`, the `PRICE` line is still good. |
| `HASH buckets=<n> key=...` | the key string used, so the per-culture variation is visible. |
| `POOL-TAKE` | the counter both promotions and both hires touch (`this+0x6C`) — **not** the treasury and **not** the price. |
| `GATE` | the promote gate was entered for the unit clicked (entry only; its result is not logged). |
| `HIRE-GENERAL` / `HIRE-ADMIRAL` | the pool's own hire, a different cost source, logged side by side. |

The treasury spend, the hash and `POOL-TAKE` are also hit **outside** the promote path (other
spends, other lookups), so expect extra `PAY` / `HASH` lines and a slower game while attached.
Read the lines between an `EXEC-*` line and its `PROMOTE` line.

## If a breakpoint never hits

- **No `ARMED` lines at all** — `cdb` did not attach, or the log path is not writable. Make
  sure `cdb` was started from the repo root, `target\tmp\gh\` exists, and the game was running.
- **Some `ARMED` lines missing, or a `bl` entry shown as unresolved** — `cdb` refused that
  `bp`. It prints the offending line to the console; send that text back.
- **`EXEC-LAND` / `EXEC-NAVAL` missing but a tooltip showed a price** — the Promote button did
  not go through the engine's executor (a mod?). Send the log: the absence is itself the answer.
- **`COST44-NORECORD` instead of `PRICE land=`** — the engine found no record for that
  (agent type, subculture) pair and the slot returns `-1`. That is an answer, not a failure.
- **`COST38-*` / `COST3C-*` lines** — the naval pair at slots `+0x38` / `+0x3C`. Comparison
  only.

## Why it has to be a probe — and how sure each step is

The static reading is from 0-G's Ghidra rounds 1-3 in the sandbox. Its decompiles were kept
only in the sandbox's ignored `target/tmp/gh/`, so **none of it is on record in this repo**,
and the review of 2026-10-06 downgraded every step that rests on it to **INFERRED** until this
probe logs it:

- INFERRED: the land unit class's cost slot `+0x44` is `0x008E2770`, which reads one value
  through `0x008E27D0`, `-1` with no record, with **no rank and no distance input**; the naval
  class's is a return-0 stub, so a naval promotion is free (`PAY amount=0` will say).
- INFERRED: the value comes from the string hash `0x00F9C2A0`, keyed by
  `<agent-type name> ++ <the human faction's subculture>`, the `agents_tables` row used only as
  a guard. **Which object owns the hash is UNKNOWN**: round 2 and an earlier draft of this sheet
  said a character object, round 3 an `AGENT_RECORD` row — and round 3's own list of the row
  reader's writes includes a byte at `+0x40`, the offset the hash keeps its bucket array at.
  The `CHAIN` line settles it.
- CONFIRMED (shipped data, read on the install in round 3): `db\agents_tables\agents` (1657
  bytes, 65 strings) and `db\agent_culture_details_tables\agent_culture_details` (5020 bytes,
  163 strings) hold strings only, so neither carries a price.
- Not found statically (an honest negative, not a proof): no writer of the hash. So the number
  needs the running game; that is the case for this probe, not a CONFIRMED impossibility.

**The `units` column #7 `unknown_3c`** (round 2,
`cargo run --release -p ntw_data --example promotion_price_check`, 442 rows): ratio to
`recruitment_cost` from 0.698 to 4.857, 38 of 119 cost values split. That rules it out **only
if** the price is one number per (agent type, subculture), which is itself the INFERRED reading
above — so it is **INFERRED unlikely**, not refuted. If step 2's two land units give different
prices, it is back in play.

## If the answer is "one flat number"

The model replaces the hire formula in `CampaignModel::promotion_cost` / `promote_unit` with
that constant for a land force (naval stays 0), the value goes into `CHARACTERS_FIDELITY.md`
§12 as CONFIRMED (the log is the evidence), and BACKLOG 0-G's **Open** list loses its last item.

## If the answer is "it follows the unit"

Then the record behind the hash is per unit and `unknown_3c` is back in play; compare
`*(record+0x0C)` against the `units` row pointers. The `CHAIN` line already prints the record
pointer, so a second pass can `db <rec> L40` and match the key string.

## History

- Round 2's script broke on `0x008E27A0` (the cost slot's `return -1`) instead of `0x008E279C`
  (`return <price>`), so the price line could never have appeared. Round 3 instrumented both
  returns of all three slots and moved the addresses into `crates/ntw_data/src/debugger.rs`.
- **Review, 2026-10-06 — three more real bugs in round 3's script, fixed:**
  1. Its prose used `##` and `$$` comment lines. In cdb a line starting `#` is the
     **disassembly search command**, so cdb would have run each of those lines as a search;
     and `$$` comments end at the first `;`, so text after a semicolon would have run as a
     command. Every comment is now a `*` line (cdb ignores the rest of such a line, semicolons
     included), and the checker refuses `#` and `$$` lines.
  2. Its `ARMED <name>` tag sat **inside** each breakpoint's command string, which cdb runs
     when the breakpoint is **hit**, not when it is set — so "a gap in the ARMED lines names the
     stale breakpoint" was false: an unhit breakpoint and a refused one looked the same. The
     tag is now a separate `.printf "ARMED <name>\n"` line after each `bp`, plus a `bl` listing,
     and the checker requires the line.
  3. Its `.logopen` was an absolute path on one person's machine
     (`C:\Users\...\NR-sb-0g\...`); it is now relative to the repo root. Also dropped: the advice
     to "start cdb first and let it wait" (`-pn` does not wait for a process), the claim that
     the gate "always returns 1" (UNKNOWN), and "the breakpoints are on the promote path only"
     (the treasury and hash breakpoints are not).

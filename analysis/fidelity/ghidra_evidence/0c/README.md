# 0c raw Ghidra evidence — PRIVATE FORK ONLY

**Do not merge or cherry-pick this directory into the public main repo.**
The .txt files here are raw Ghidra decompilation dumps and target lists
extracted from the game executable. The project's .gitignore deliberately
excludes nalysis/**/*.txt as copyrighted game material.

They live here only so the reverse-engineering evidence backing every
\CONFIRMED\ tag in the fidelity notes is not lost with the sandbox
worktree, which is its only other copy. The owning repo is the **private**
fork \DenseCow/NapoleonRust-sandbox\; these commits are pushed to the
\sandbox\ remote and nowhere else.

When porting work to main, take the \crates/\ changes and the
\nalysis/fidelity/*.md\ reports — **cherry-pick specific commits, never
merge the branch wholesale.**

## Round 11 (2026-10-04) — what each file backs

| file | backs |
|---|---|
| `ghidra_out_r11a.txt` | the exact 53 bytes of `0x00E5F760` (`dis:`) plus its decompile — the `index % count` fragment rule and the group-0 fallback |
| `ghidra_out_r11b.txt` | all 864 descriptors of `0x013AEBE0` read at stride `0x18`: the `+0x14` census (**708 set / 156 clear**, values only 0 or 1) and the 864 `+0x04` names confirming the table order is the slot index |
| `ghidra_out_r11c.txt` | `0x006613A0` and `0x006618D0` in full — the two call sites that decide which `0x00E5F760` argument is the slot, and the `A + 0x14 + slot*0x18` gate |
| `ghidra_out_r11d.txt` | the complete `getCallingFunctions` census of every slot of the condition vtable `0x013A91F8`, the bank vtable `0x013A993C` and the entry vtable `0x013A95A8` |
| `ghidra_out_r11e.txt` | the only slots with any callers: `0x0054E920` (a flag at `+4` whose callers are the AI negotiator) and `0x006F3C00` (a `*(this+0x24)=0` reset); plus the class-name accessors returning `"audio_distance"` and `"sound_bank_projectile_impact"` |
| `ghidra_out_r11f.txt` | the decisive `xref:` census - the weight `0x00CB0280` and the entry reader `0x00E28DF0` have **no code references at all**, only vtable slots. Conclusion: nothing in 1.3 dispatches a bank query |

## Round 12 (2026-10-04) - what each file backs

| file | backs |
|---|---|
| `ghidra_out_r12a.txt` | `0x00817BB0` and `0x00817C10` in full; the `xref:` census of the stance table `0x014520B0`; `0x007EE760` (240 bytes) and `0x007F1730` (24 bytes) - the clip queue is `{action_slot, time}`, **not** `{handle, time}`; `0x0065FB00` and `0x0064D6F0` as further `+0x2C` readers, and `0x0064D8C0` / `0x0064DAF0` / `0x0061C760` as their load-time context |
| `ghidra_out_r12b.txt` | the raw data reads: the **46 `{first_slot, count}` pairs at `0x014520B0`** (the animation cycle families), the all-zero transition entry at `0x01454398` = `{0x360, 0}`, and the per-slot blend tables at `0x01332050` / `0x01332064` |
| `ghidra_out_r12c.txt` | the exact bytes of `0x00817BB0` (`dis:`) pinning the transition block layout to `this + 0x18 + (from*73 + to)*0x1C`; `0x00E10410` = the `battle_entity_man_animation_tables` accessor and `0x00E10170` = `battle_entity_animation_table_manager` |
| `ghidra_out_r12e.txt` | the three sibling record builders `0x00E10210` / `0x00E10290` / `0x00E10390` (camel / elephant / horse) and `0x00E52C30`, which assembles the 100-byte record array and calls the writer `0x00E52480`; `0x006666E0` and `0x006616C0` |
| `ghidra_out_r12f.txt` | the sibling static tables `0x014520F0` (entry 8 of the same array) and `0x01452190` (entry 60), read to check that the stance table is one contiguous run |
| `ghidra_out_r12g.txt` | **`0x00E52480`** (1954 bytes, the sole callee of `0x00E52C30` that allocates the 0xA8 per-table records - the closest thing yet to the `+0x2C` writer) and `0x00E61160`; plus `dis:0x00663240`, which shows Ghidra renders the stance-table reads as `[0x014520b0]` - the reason two `insn:` scans for `0x14520b0` found nothing |

## Round 13 (2026-10-05) - what each file backs

| file | backs |
|---|---|
| `ghidra_out_r13a.txt` | **the population loop, `0x00E60260`, in full and instruction by instruction** - the store `MOV [ESI + EDX*0x4],EAX` / `MOV [ESI + 0x14],count+1` at `0x00E605C4..0x00E605D6`, the fragment stride `ADD ESI,0x1c` and the slot bound `ADD EAX,0x90` / `CMP EAX,0x1e600`; plus `0x00E506D0` (the ctor that allocates its dword `0xB` = **the `+0x2C` array** and zeroes every group's `+0x14` 864 times), `0x00E50930` / `0x00E50960` (the `0x8C` per-slot fragment-count initialiser, `0x360` times), `0x00E61160` exact bytes (`ADD EBX,0x18` / `CMP EBX,0x5100` = the 864-group walk), `0x00E60660` (the path-keyed clip cache that mints the handle), `0x00E61020`, and the 13 callers of `0x006631A0` |
| `ghidra_out_r13b.txt` | all 13 call sites of `0x006631A0` with their first argument, and `0x00E4F4D0` (the clip-object maker the handle comes from), `0x0065FB00` / `0x0067A050` (the per-frame readers of the handle's fields) |
| `ghidra_out_r13c.txt` | **`0x00643900`** (28 bytes: a vcall returning the state, `0x49` = "none"), `dis:0x00662E70` (the 8-byte state-setter thunk `MOV ECX,[ECX+4]` / `JMP 0x006631A0`), and the `insnr:` census of every `+0x1B8` read/write in `0x006xxxxx` - which is how `0x006543D0`'s `+0x1D8 -> +0x1B8` switch was found |
| `ghidra_out_r13d.txt` | **`0x00663730` in full** (715 bytes, 8 callers, no callees: a switch on `entity + 0x1D8` whose every case stores an immediate into `entity + 0x1B8`, over 73 cases, reaching 63 distinct states), `0x00662E80` (the generic store of its argument into `+0x1B8`), `0x0066CE00` (the per-frame update), and `dis:` of the state-10 site |

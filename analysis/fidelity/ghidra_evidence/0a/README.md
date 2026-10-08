# 0a raw Ghidra evidence — PRIVATE FORK ONLY

**Do not merge or cherry-pick this directory into the public main repo.**
The .txt files here are raw Ghidra decompilation dumps and target lists
extracted from the game executable. The project's .gitignore deliberately
excludes nalysis/**/*.txt as copyrighted game material.

They live here only so the reverse-engineering evidence backing every
\CONFIRMED\ tag in the fidelity notes is not lost with the sandbox
worktree, which is its only other copy. The owning repo is the **private**
fork \Rosebuddyy/NapoleonRust-sandbox\; these commits are pushed to the
\sandbox\ remote and nowhere else.

When porting work to main, take the \crates/\ changes and the
\nalysis/fidelity/*.md\ reports — **cherry-pick specific commits, never
merge the branch wholesale.**

## Round N+2 batches (the garrison cap `+0x6C`, §54)

- `0a_garr2_*` — the battle building setup's own stores (`lst:0x00688de0:0x00688ea0`) plus
  `0x005784C0`: the exact `[EBX+4]` / `[EBX+0x54]` chain and the `type+0x14` get-or-create slot
  block. This CORRECTS the object chain of §53 (4).
- `0a_garr3_*` — `0x00E691E0` (the `battlefield_buildings` row callback) and a scalar-`0x6C` sweep
  of the DB layer `0x00E69000..0x00E6A800`; `0x0052F3A0`.
- `0a_garr4_*` — `0x00E4F4D0` (the caller of the "inherit if unset" merge: it builds the
  FIRE_POSITION / FUSE_POSITION / IMPACT_POSITION / DISTANCE block of a *projectile* record, so
  its optional `+0x6C` columns are NOT the garrison cap) and `0x00E55EB0`.
- `0a_garr5_*` — the `+0x6C` writes inside the building-type builder `0x00E6A030`: a 16-byte struct
  copy (`[ESI+0x40]` → `[EBX+0x6C]`) and a zeroing initialiser, i.e. not the cap.

## Round N+4 batches (the XP writer sweep, item 1; garrison cap ideas a+b, item 4)
- `sb0a_xp1_*` - exe-wide `scal:0xD48:0x00400000:0x01000000`: **12 hits, all `MOVZX` reads, no
  store to the experience byte anywhere**; plus `str:chevron/Experience/experience`.
- `sb0a_xp2_*` - the listing around `0x008762CB` (the campaign model's own container),
  `callers:0x008751e0`, `0x00872550`, `0x008751e0`, and the new readers `0x008590b0` /
  `0x005bdce0` / `0x0057f070`.
- `sb0a_xp3_*` - `str:army_experience` / `str:navy_experience` (**no referent**), and `0x009aa5e0`
  plus its caller `0x009abe00`.
- `sb0a_xp4_*` - `lst:0x009aa5e0:0x009aafe0` with string operands: the **script getter
  registration** table (`Name`/`Men`/`Experience`/`RecruitCost`/...), proving it is not a setter.
  Plus item 4(b): `callers:0x00688dd0` and `0x0052f3a0` decompiled - the record is
  `record_index` s **own row**, not a copy.
- `sb0a_xp5_*` - item 4(a): the **whole body** of `0x00688dd0` (`lst:0x00688dd0:0x00689610`) =
  **no `+0x6C` instruction at all**; a `scal:0x6c` sweep of the battle-file load path
  `0x00510000..0x00560000`; and `callers:0x00546a10` (the load chain
  `0x00506c00 -> 0x00515340 -> 0x00546a10 -> 0x0052f3a0 -> 0x00688dd0`).

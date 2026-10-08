# Sandbox findings for Claude (small-model workers, fork repo)

A separate sandbox copy of this repo lives at
`https://github.com/Rosebuddyy/NapoleonRust-sandbox` (private).
All small-model work happens there on `work/sandbox/*` branches and is
merged only into the sandbox repo — never into this repo's `main`.
This file is the handoff: what the sandbox workers finished, with
branch names and commit SHAs so anything useful can be cherry-picked.

Sandbox `main` mirrors this repo's `main` at `3b7762d`.

## Done and pushed to the sandbox repo

### 0-B campaign rules, round 12 step 1 — recruitment cancel path
- Branch: `work/0b-round12-sandbox`, commit `21f1736` (comments only, 19+/2-).
- Files: `crates/ntw_sim/src/campaign/commands.rs:1008`,
  `crates/ntw_sim/src/campaign/turn.rs:407`,
  `analysis/fidelity/CAMPAIGN_FIDELITY.md` ("Where I am" recruitment bullet).
- Ghidra (`NR-fc-ghidra`): `0x00AECEE0`/`0x00AED220` CONFIRMED as the
  land/naval recruitment-item constructors (sole caller `0x00B58DD0`,
  turns from `UNIT_RECORD`+0x34 as already modelled). Cancel `0x00B1A820`
  calls item slot +0x1c (`0x00B5C060` land) with 1, crediting economics
  via `0x00BB3810(item+0x20, kind 3)`; campaign variable 37 =
  `recruitment_population_cost` added to queue-manager +0x54.
- Still UNKNOWN: whether the credited amount equals the full queued cost
  (kind-3 evaluator + item+0x20 contents), so the model's full refund
  stays PROVISIONAL. Spawn side (`0x00B5C1B0`, new-army placement,
  `num_men` size, garrison join) open — step 2 in progress.
- Tests: `cargo test -p ntw_sim -p ntw_campaign`, all ok (283 + 20).
- Game check (manager, prebuilt `target/debug/napoleon.exe`):
  `--campaign eur_napoleon --campaign-faction france --screenshot` exits 0,
  turn 1 as France, 0 script errors.

### Pathfinding-ports, round 8 — `polypath.rs` visibility audit
- Branch: `work/sandbox/ports`, commit `036bc00` (2 files, 16+/1-).
- `crates/ntw_sim/src/campaign/polypath.rs:941`: `pub fn dist_to_polygon`
  demoted to private — all 3 callers are inside `polypath.rs`
  (lines 571, 602, 659). `cell_rc`/`multiplier`/`goal_step`,
  `octant_dir`, `dist_to_segment` confirmed required `pub` (used by
  `embark.rs`). Behavior-neutral.
- Notes: `analysis/campaign/PATHFINDING_PORTS.md` §14 (Round 8).
- Tests: `ntw_sim --lib campaign::` 133 passed, `ntw_campaign --lib`
  29 passed, 0 failed.

### 0-C middleware, round 4 — bank query + cue dispatch leads
- Branch: `work/sandbox/0c-middleware`, commit `1e16d25` (notes only, +18).
- File: `analysis/fidelity/MIDDLEWARE_VERIFY.md` (§0 round-4 bullet + §3).
- Ghidra (`NR-f0c-ghidra` exe, `NR-miles-ghidra`): bank query still
  UNKNOWN (vtable refs are in-module DATA, factory/reader have no direct
  callers, names live in data tables + `0x00E14D30`). Cue dispatch not
  found; cue+187 stays INFERRED. New CONFIRMED: `0x010CB060` 7th arg is
  the cue list at the `0x00F85780` call site (maker is generic). Global
  cue list `0x01650414` has no consumer (loader writes, `0x00F76E30`
  frees). Next lead recorded: per-frame anim update near loader
  `0x00DE1E10` walking 8-byte {time,cue} entries. No `mixer.rs` change.
- Tests: none (no code changed).

### 0-C middleware, round 6 — maker callers mapped
- Branch: `work/sandbox/0c-middleware`, commit `5e0005b` (notes only, +15).
- CONFIRMED: `0x010CB060` takes 7 args (7th → object +0xA0), caller list
  complete at 3. Only `0x00F85780` passes the cue list (both its callers are
  campaign anim-set builders: character sets + walk sets). Other two callers
  INFERRED cue-less (static-model path, effect/bone-attachment build).
  Per-frame tick NOT FOUND — every path is load-time. cue+187 INFERRED.
- Next: consumer `0x00DD5E50`, walk up from `0x00F72A60`/`0x00E5xxxx`
  toward battle per-frame update.
- Branch: `work/sandbox/0c-middleware`, commit `a065ccd` (notes only, +13/-3).
- Killed: `0x00DE1E10` is a campaign anim-set builder (sole caller
  `0x00DDC320`), not a cue walker — round-4 lead dead. Both cue-list
  containers write-only from exe code (reader inside anim library). Anim
  library never calls the sound play path directly (zero callers in
  `0x0100xxxx` play paths) — direct-callback hypothesis dead.
- cue+187 stays INFERRED. New lead: game-side per-frame anim player ticking
  the `0x010CB060` object — from maker's other callers `0x011C1CC0` /
  `0x00E5FF20`, or battle per-frame update.

### 0-G characters, hooks spec for 0-E
- Branch: `work/sandbox/0g-characters`, commit `b27aff3` (new file, +299).
- New `analysis/fidelity/CHARACTER_UI_HOOKS.md` (§§H1–H4): HireGeneral /
  HireAdmiral (`commands.rs:129,137` → `pool.rs:224,293`, formula
  `pool.rs:183-198`), PromoteUnit + PromotionCost (`commands.rs:146` →
  `pool.rs:348`; UP triple: unit gate UNKNOWN, effect gate INFERRED,
  command state CONFIRMED; naval cost 0), spy actions (chances, commands,
  `AgentActionResolved` + 13 script events), fog layer (`visibility.rs`,
  `agents.rs:75,390`, existing UI seams `campaign.rs`). Every call
  cited file:line. No code changed; 133 campaign tests pass.

### 0-G characters, field-promotion cost — static trace
- Branch: `work/sandbox/0g-characters`, commit `2bb08d9` (notes only, +42/-3).
- File: `analysis/fidelity/CHARACTERS_FIDELITY.md` (§11 row, §12, new addendum).
- Ghidra (`NR-spt-ghidra`, read-only): charge mechanism CONFIRMED static —
  land `0x008E1C20` / naval `0x008E2260` call unit-class slot +0x44, pass to
  treasury spend `0x00BAF500(value, 2)`; UI builder `0x009ABE00` publishes
  the same slot as `PromotionCost` (displayed == charged by construction).
  Hire prices through a different slot (+0x3c: base + per-star rank +
  distance), so the PROVISIONAL hire-formula is now INFERRED-wrong in
  structure — no code changed (replacement not CONFIRMED). Land value
  INFERRED: slot +0x44 = `0x008E2770` static table read (no rank/distance
  input); naval slot +0x44 is a return-0 stub (naval promotion charges 0).
  Gate UNKNOWN (base tables hold return-0 stub at slot +0x40).
- Probe plan ready for a ~10 min user debugger sitting (script in the
  sandbox scratch, details in the fidelity note): compare `PROMCOST-LAND`
  vs tooltip vs `PAY`; naval expectation `PAY amount=0`.
- Tests: promotion/pool/hired/campaign suites all pass (1+2+2+133).

### 0-A battle, experience/chevrons — PORTED (needs `cargo check -p napoleon`)
- Branch: `work/sandbox/0a-battle`, commit `631a3ae` (4 files, +51/-25).
- Ghidra (`NR-f0a-ghidra`, re-derived): `unit+0xD48` CONFIRMED as experience
  level (exposed as "Experience" by `0x005ABF40`/`0x005CD340`; read by
  waver `0x0053E4D0` = base+exp*5, rout `0x0053A720` = max(0,base-exp*20),
  fatigue-bonus `0x00670F40`). Old "unit index stagger" note was a
  misreading — corrected. Hit-number `0x00DAB5F0` has NO experience term
  (CONFIRMED negative). Fatigue-table +0x20 term INFERRED, not ported
  (needs `unit_stats_land_experience_bonuses` in `ntw_data`).
- Code: `LandUnit::experience: u8` (`model.rs`), `MoraleInputs::unit_index`
  → `experience` (same math, relabeled; `morale.rs`), battle-file + custom
  xp wired (`setup.rs`). Test renamed to `timers_scale_with_experience`.
- Tests: `ntw_sim` 283 + `ntw_ai` suites all pass (39/11/9/3/1).
- NOT verified: `napoleon` crate compile (fresh-target Bevy crash here).
  `cargo check -p napoleon` required before cherry-pick. Behavior note:
  units built via `LandUnit::new` default xp 0 (old code staggered timers
  by unit id — that was the misreading).

### 0-A battle — experience/fatigue table DECODED and ported
- Branch: `work/sandbox/0a-battle`, commits `2054325` (+ `80ab2ef` runner
  script, `7127e84` Austerlitz report moved under `analysis/fidelity/`).
- **Whole new table decoded:** `unit_stats_land_experience_bonuses`, v0,
  **10 rows, decodes with ZERO leftover bytes** — that residual check is
  strong proof the layout is right, not a guess.
  - Identified by string anchoring, not call-site scanning: `FUN_00E31490`'s
    own `"Loading database: %s"` string names the table, and its
    `record_index` error string names `UNIT_STATS_LAND_EXPERIENCE_BONUS_RECORD`.
  - `0x00670F40` reads **column 6 (builder +0x20)** of the row at the unit's
    experience byte (`unit+0xD48`), indexed **BY ROW POSITION**.
  - Ported: table in `ntw_data`, `Battle::experience_fatigue` carries the
    column, `fatigue::experience_bonuses` does the positional lookup, wired
    from the DB in battle setup.
- **Also CONFIRMED:** `0x00ED49A0` reads **+0x24 (flat)** and **+0x28
  (multiplier)** of the same table as `row+0x24 + ROUND(base*row+0x28)`.
  A second, still-unwired term on the same table — cheap next port.
- **Claude cherry-pick warning:** touches `crates/napoleon/src/battle/setup.rs`.
  That crate cannot be compiled in the sandbox (fresh-target Bevy
  `STATUS_ACCESS_VIOLATION`), so this round is unit-tested only. Needs
  `cargo check -p napoleon` before merge.

### Where the raw reverse-engineering evidence lives — READ THIS
- `.gitignore:17` deliberately excludes `analysis/**/*.txt` (and `.tsv/.csv/
  .log/.bin`) as *"Bulk data extracted from the game (copyrighted text/tables)
  — our own reports (*.md) and code are uploaded, bulk dumps stay local."*
  That policy is **still in force for the public repo — nothing was leaked.**
- **Owner decision:** the raw Ghidra dumps are archived on the **private** fork
  `Rosebuddyy/NapoleonRust-sandbox`, force-added past the gitignore rule, at
  `analysis/fidelity/ghidra_evidence/<worker>/` on the fork's **`main`**
  (247 files, ~5.1 MB: `0a 98`, `0b 54`, `0c 34`, `0d 36`, `0e 22`, `ports 3`).
- **Verified after the push:** fork visibility `private`; evidence browsable and
  byte-fetchable at the fork's `main` (spot-checked `ghidra_out_0c21.txt`,
  1318.3 KB, sha `627247d`); `git/trees` on the **public** `main` contains **no**
  `ghidra_evidence` path — public tip is still the docs-only commit `ac3c2cd`.

#### Fork branch topology changed — read before syncing
- `sandbox/main` is **no longer a pure mirror** of Claude's main. It is now
  Claude's main **plus all 7 worker branches merged in** (`05b93d8`), so sandbox
  WIP code and evidence are browsable in one place at
  `Rosebuddyy/NapoleonRust-sandbox/tree/main`.
- **`sandbox/claude-main-mirror` (`ac3c2cd`) is the preserved pure mirror** of
  Claude's public main. Use it as the clean rebase base for new work; do not
  rebase workers onto `sandbox/main` any more.
- The old fast-forward sync (`push sandbox origin/main:main`) will now be
  rejected as non-fast-forward. Sync with an explicit merge of `origin/main`
  into the fork branch instead.
- The 7 per-worker `work/...` branches still exist and are unchanged — cherry-pick
  from those for clean single-commit history.

### 0-C middleware round 8 — cue-list lead CLOSED (definitive negative), 2 more premises refuted
- Branch: `work/sandbox/0c-middleware`, commit `b90a1c2` (+ evidence `63bbdd1`).
  Notes only, +81. No Rust, no tests — brief required both halves CONFIRMED.
- **Complete xref census of the global cue list — all 19 refs accounted for:**
  - `0x01650414` (data): 8 refs — writer `0x00F843A0` (4), teardown
    `0x00F76E30` (3), exit-free `0x013013B0` (1).
  - `0x01650410` (count): 8 refs — `0x00F843A0` (5), `0x00F76E30` (2),
    `0x013013B0` (1).  `0x0165040C` (cap): 3 refs, all `0x00F843A0`.
  - `0x013013B0` is only an exit-time destructor. **There is NO per-frame
    reader of the global cue list in `Napoleon.exe`.** `0x00F843A0` publishes
    the container nowhere else — its only other holder is the return value →
    `0x00F85780` → anim object **+0xA0**.
- **REFUTED #1: that list is campaign-side, not battle.** Its only writer is the
  campaign anim-set/walk-anim builder `0x00F85780`; teardown hangs off
  `0x00DED3D0` ← `0x00E07B80` ← `0x00E17650`.
- **REFUTED #2: `0x00E611E0` / `Animations/BattleConfiguration/` is not cue
  data.** It builds **0x1E610-byte** objects keyed by u16, and its parser
  `0x00E62760` reads `ACTION_<name>` blocks with up to **5 fragments of 0x1C
  bytes** each (`filename`, `blend_in_time`, `equipment_usage` ×10,
  weapon/defensive/ambient/personal/special flags) — this is the battle
  **animation action table**, a different subsystem entirely.
- **Full 0xA4 anim-object layout now CONFIRMED** (`0x010CB060`). It is a
  *bone-attached animation instance*, not an anim library:
  `+0x00` anim-file `UIFileIn*`, `+0x08` `0x010CC4C0` bone sub-object,
  `+0x48/+0x4C` direction, `+0x50` length, `+0x60` angular velocity,
  `+0x80` bone array (count `+0x88`, data `+0x8C`), `+0xA0` cue container.
- **`0x00485B90` has NO battle branch at all** — 0 calls into
  `0x00F7xxxx..0x00FAxxxx`, its 109 callees are all front-end/campaign, and the
  exe contains **no "Creating battle..." string**. Round 7's plan to isolate a
  battle branch there was never going to work.
- `.anim_sound_event` global `0x0164B4D4` has exactly **4** refs (2 loaders,
  2 static inits) — **no consumer**. `strs:cue` = **0 hits**.
- Round 7's `0xa4]` scan was stack noise: the allocator pattern is `push 0xa4`,
  and the correct scan yields only 4 functions — **no sibling allocator**.
- `0x00F76D10` confirms **10** 0xA4 objects at dwords 0,1,2,3,4,6,8,9,0xB,0xC of
  a 0x34 block; `0x00F77940` shows walk anim sets live in a name-keyed
  `DATABASE_TABLE<CAMPAIGN_WALK_ANIM_SET_RECORD::BUILDER>`, 0xC-byte records.
- **Still UNKNOWN:** the per-frame tick / cue dispatch; whether anything
  consumes these containers at all (both provably write-only from exe code);
  the battle per-frame dispatcher; the bank query. **New best lead:** the tick
  is the only code that would touch **both** `+0x80/+0x88` and `+0xA0`.
- Three rounds spent (6, 7, 8) all ended in honest negatives. Worth noting the
  original premise — that battle cues are dispatched from a container at anim
  object `+0xA0` — has now been refuted twice over. Claude may want to
  reconsider whether cue dispatch is even exe-side rather than data-side.

### 0-C round 9 — cue thread CLOSED for good; ACTION table CONFIRMED (+779, first code in 4 rounds)
- Branch: `work/sandbox/0c-middleware`, commit `88437fe` (2 files, **+779/-17**).
  `ntw_formats --lib` **146 pass** (was 137, +9), exit 0 (verified). 0 clippy
  hits in the file. Diff includes rustfmt reflow + a dropped UTF-8 BOM, so the
  −17 is formatting, not behaviour.

**Part A — the cue-dispatch thread is DEAD exe-side. Do not re-open.**
1. `+0xA0` read scan over `0x010C0000..0x01220000`: 216 register-indirect sites,
   **none of this class**. The only store to this class's `+0xA0` is round 7's
   maker `0x010CB060`. Its destructor `0x010D2C30` (callers `0x00DDF570`,
   `0x011E33D0`, `0x00F76D10`, `0x00E56CC0`) walks its arrays and **stops at
   `+0x9C`** — the class does not even own the container.
2. Round 8's paired-offset lead is **REFUTED as a filter**: `+0x80` & `+0x88` &
   `+0xA0` yields **78** functions, not 1.
3. **The decisive result** — a complete `getCallingFunctions` census of the three
   bank-event play paths: `0x01005140` ← {`0x01005840`, `0x01007B00`};
   `0x01000F60` ← {`0x01005140`, `0x01004F80`, `0x01004E50`};
   `0x01004430` ← {`0x010075F0`}. **All six call sites are inside `0x0100xxxx`.**
   No animation, battle or campaign code can start a sound at all.
- **Conclusion:** in 1.3 the `.anim_sound_event` lists are parsed, cached and
  never read. **Cues are data-side only**; `cue + 187` stays an INFERRED data
  inference and **no engine code should be written to play them.** Battle
  animation *sound* fidelity must come from the action table instead.
- Corrected from round 8: the second array's count is at **`+0x98`** / data
  **`+0x9C`** (not `+0x90`), and a third array has count `+0x3C`, data `+0x40`,
  stride `0x14`.

**Part B — battle animation ACTION table, contract now CONFIRMED from the exe**
- Addresses: parser `0x00E62760` (6764 B), loader/cache `0x00E611E0` (419 B),
  ctor `0x00E50930`, per-slot init `0x00E50960`, name resolver **`0x00E5FA30`**,
  `equipment_usage` reader `0x00E60EE0`, `special` reader `0x00E69EB0`, battle
  entity ctor **`0x0061CB90`** (sole `record_index` referrer).
- Geometry confirmed: `0x10 + 864 × 0x90 == 0x1E610`; 5 × 0x1C fragments + u32
  count at `+0x8C`. Fragment `0x1C`: **+0x00** tag (`0x360` = empty, else slot
  index), **+0x04** UniString filename, **+0x10** f32 `blend_in_time` (default
  **1.0**), **+0x14** u32 `equipment_usage`, **+0x18** u16 flags.
- 864 descriptors of 0x18 at `0x013AEBE0`; hash map `0x00E694F0` /
  `0x004CE060` / `0x004C9FF0`, buckets `0x0145BAD4` / `0x0145BAD8`.
- **Closed enums:** `equipment_usage` = none/rifle/rifle_butt/rifle_bayonet/sword/
  axe/pike/lance/longsword/shield, with **10 = unrecognised** and **11 = the
  `cancel` marker**; `special` = 10 bit values `0x20..0x4000` with
  **`0x8000` = unrecognised**. Five on/off keywords = flag **bits 0..4**;
  `,` = `0x013343B4`, `=` = `0x0131328C`.
- String-anchored class name: **`EMPIREBATTLE::ENTITY_ANIMATION_ACTION_TABLE`**.
- **`ACTION_<name>` → index: `entry[i].index == i` for all 864** (0 mismatches),
  so **table order is the slot index**. Compare is `strcmp`, so **case
  sensitive**. All 864 names transcribed (20 Ghidra could not read were pulled
  raw from `.rdata`: image base `0x00400000`, VA `0xF07000`, raw `0xF05E00`).
- Code is additive: geometry constants, `fragment_field` offsets,
  `EquipmentUsage`/`SpecialUsage` + sentinels, `FLAG_*`/`DISPLAY_FLAGS`/`ON_OFF`,
  the 864-name `SLOT_NAMES`, and `action_slot()`.

**0-C still open:** the per-frame consumer of the action table — now the *only*
lead left for battle animation. Meanings of four non-name descriptor dwords
(`+0x08` = 32 distinct category codes; `+0x0C`/`+0x10` INFERRED link indices with
`864` = none; `+0x14` bool). Whether the 5 fragments per slot are alternatives
or a priority list (ours treats a later fragment as replacing the slot — still
INFERRED). The bank query and the battle per-frame dispatcher, not attempted.

### 0-A round — `unit_scale` SOLVED after 3 rounds open, and it changes every battle
- Branch: `work/sandbox/0a-battle`, commit `e2fdafe` (8 files, **+534/-21**).
  Tests **369 pass, 0 fail** across `ntw_data`/`ntw_sim`/`ntw_ai` + 15 install
  tests (verified independently, exit 0). No new clippy warnings.
- **The `MULSS`, found at `0x004A67E5` inside `0x004A6600`** (army → battle unit
  creator): `MOVZX EAX,word [EBP+0xA]` (card men, u16) → `CVTDQ2PS` →
  `MULSS XMM0,[ESP+0x30]` (the clamp from `0x004A6540`) → `CVTTSS2SI` **truncated**
  → pushed to `0x005363C0` → `0x00513320`, which writes `in_ECX[0x32] =
  in_ECX[0x33] = men`, i.e. card **`+0xC8`/`+0xCC`**. Ship cards take an
  **unscaled byte at `+0x0E`** (`0x004A6A06`); battle-file units stay unscaled
  (`0x00513440`).
- **HIGH-IMPACT FIDELITY FINDING — our unit counts are wrong by default.**
  The scale step is the environment-variable preference **`gfx_unit_scale`**
  (`0x00404230`, storage `0x0149D880`, **default 2**, help text *"Set unit scale.
  0 - lowest, 3 - ultra"*). The player's `preferences.script.txt` ships
  `gfx_unit_scale 2`, so **the original's default battle fields 0.75 × the card
  men**. Any head-to-side-side comparison against the original will disagree on
  every unit's strength until this is wired. Ported as
  `ntw_sim::battle::unit_scale` (+110 lines) with `SetupData::unit_scale` and
  `unit_scale_setting()` reading the preference.
- **The `+0x24`/`+0x28` pair is NOT fatigue — this corrects a round-13 brief.**
  `0x005CD340` labels the call's result **`"XpAdjustedCost"`** (it sits between
  `"Experience"` and `"RecruitCost"`/`"UpkeepCost"` in the string table), and
  `0x0045CB50` sums it and refuses purchases over `FUN_004A2910`'s budget,
  checking each type's experience first. It is the campaign's **XP-adjusted
  recruit/upkeep cost**, and `base` is *scaled*, not added. Ported as
  `GameDatabase::experience_adjusted_cost` / `naval_experience_adjusted_cost`.
  **Deliberately NOT wired into the fatigue path** — my earlier brief assumed it
  was; 0-A disproved that from the string evidence.
- **New table decoded: `unit_stats_naval_experience_bonuses`** — getter
  `0x00E31710` names it twice, **v0, 10 rows, ranks "0".."9", zero leftover
  bytes** → 7-column layout CONFIRMED. `+0x1C`/`+0x20` are the pair
  `0x00ED49A0`'s naval branch reads (rank 9: `+255`, `×1.45`). In `ntw_data`
  with an install test printing every row.
- **`crates/napoleon` is UNVERIFIED for compile** — `SetupData::unit_scale`,
  `unit_scale_setting()`, `build_battle`, `make_unit`. Sandbox rule forbids
  `-p napoleon`. `rustfmt --check` parses it (only a pre-existing import-order
  nit), types hand-checked. **Needs `cargo check -p napoleon` before merge.**
- **Garrison cap `+0x6C`: narrowed, still UNKNOWN, and now explained.**
  `building+0x54` *is* the `battlefield_buildings` row, but the cap chain runs
  `garrison+4 → +0x54`, and **no column of any building table sits at `+0x6C`**
  (`battlefield_buildings` ends at `0x58`; `building_levels`' `+0x6C` is inside a
  string slot) and there is **no `garrison` string in the exe**. So the cap is a
  *runtime field of the building type object* — which is exactly why three
  rounds of "find the column" failed. Next lead: its setup/vtable
  (`0x1321294` → `0x1338CD0`) or a `type+0x6C` watchpoint.
- Formation `+0x670`: no debugger here, so **`FORMATION_RADIUS = 0` left
  untouched and no value invented.** Also open: land `+0x0C..+0x1C` / naval
  `+0x10..+0x18` columns (no reader), what fills battle-settings map key `0x0B`,
  and whether the XP cost is charged anywhere beyond the panel and auto-build.

### 0-E negotiation round — 20-shape ledger, appliers named, tree healthy (60 tests)
- Branch: `work/sandbox/0e-ui`, commit `1b4784c`. Tests **60 pass, 0 fail, 0
  warnings**, `EXIT=0` (verified independently).
- **Honesty pass on the appliers** — this is the valuable part. `apply_deal`'s
  PLACEHOLDER list now names a **target address per dropped row type** instead
  of silently discarding: regions → `0x00B449F0`; payments → `0x00BB3810` /
  `0x00B1A790`; stance/access/gift → the seven 0-G addresses; **technologies →
  explicitly UNKNOWN**, "no address reachable". Mapping `Propose` /
  `ProposeDeal` / `AcceptOffer` onto one applier is downgraded to **INFERRED**
  (none of the three wrappers reads an argument or pushes a literal).
- **Shape ledger §4.1 — 6 of 20 CONFIRMED** (note: the old "19 shapes" count
  only covered the ones scripts call):
  - `BuildOfferAndDemandStrings` `0x009B48B0` — `"Offers"/"Demands"/"Action"/
    "Regions"`, region row is action id `== 6`.
  - `TradeableTechnologies` `0x009C5AA0` — `"Proposer"/"Recipient"/
    "FactionKey"/"tech_status"`, rows via card builder `0x009ABB50` (2 keys
    still unread).
  - `TradeableRegions` `0x009C5770` — `"Proposer"/"Recipient"/
    "CurrentlyOffered"/"CurrentlyDemanded"`.
  - `MaxPlayerPaymentAllowed` `0x009BE720` / `MaxOpposition…` `0x009BE6B0` —
    both one number out via `0x00BCAFE0`.  `ProposerId` `0x009BFD80` CONFIRMED.
  - `FactionListsForStanceDeclarations` `0x009BAB90` — literals
    `request_join_war`, `break_trade`, `break_alliance`.
  - The remaining 13 (`Propose`, `ProposeDeal`, `AcceptOffer`, `DeclineOffer`,
    `Cancel`, `End`, `Finished`, `CanPropose`, `CanThreaten`,
    `PrepareCounterOffer`, `RemoveAction`, `IsNegotiation`,
    `BuildPossibleActions`) are **UNKNOWN shape** — no arg reads, no literals.
- **Native appliers named §4.2:** offer `0x00B58C00`, demand `0x00B58A30`
  (←`0x00BCA430`←`0x00BA90E0`), plus `0x00B58560` / `0x00B58890`; per-item
  `0x00A6CBE0` → `0x00B1A790` (writer `0x00B1A820`, row vtable `+0x1C`);
  payments/tribute `0x00BB3810(amount, 3)`; region transfer `0x00B449F0`
  (+wrapper `0x00B58A10`); container `*(vt+0x38)+0x120`, rows `+0x14`/`+0x18`,
  flag `+0x1E8`, single items `+0x20`/`+0x24`. **Technologies UNKNOWN** — no
  granting address reachable except through the row vtable.
- **Naval tab §4.3:** CONFIRMED no naval recruitment generator is registered.
  The one recruitment generator `0x009FE7B0` *does* push a `naval` category
  (`0x00A01217`) and a per-card `is_naval`, so the INFERRED reuse of
  `GenerateRecruitmentPanel` is sound; the selector (manager bool `+0xA4`) is
  still INFERRED and `CampaignShipCard` is UNKNOWN (luac). Not wired — the
  source says so.
- **Fort panel §4.4:** registration `0x009C7B50`, info builder `0x009FDFE0` →
  `forts` / `fort_ptr` / `controlable`, rows via `0x009C9170` (the construction
  row builder). Blocked on a `CampaignSelection::Fort` that does not exist yet.
- Note for coordination: `CHARACTER_UI_HOOKS.md` lives in `NR-sb-0g`, not in
  the 0-E sandbox. 0-E read it read-only.

### 0-B campaign round 13 — full decode CONTRADICTS the round 12 port, 4 bugs fixed
- Branch: `work/0b-round12-sandbox`, commit `bf2519b` (tests **285 pass**, exit 0).
  **Supersedes `ebd6635`** — do not cherry-pick that one alone.
- `0x00B1B5A0` decoded from the bytes (`0x00B1B5A0..0x00B1B6A4`), not the round 10
  note, which was incomplete:
  ```text
  row = government_types_table.record_index(new_government_key);   // 0x00B1B61B
  mag = (row->limit < row->value) ? -events[0x94] : +events[0x94];  // 0x00B1B665..0x00B1B67E
  record[0x288].set(value = row->value, drift = mag, limit = row->limit, limited = 1);
  ```
- **The four corrections — the round 12 port was wrong on all four counts:**
  1. **value and limit were swapped.** `0x00B1B684`/`0x00B1B686` push
     `[row+0xC]` and `[row+8]` as the last two args of `0x00B69640`, whose
     listing is `(value, drift, limit)`. So **value = #2, limit = #3** — the
     reverse of `setup_relationship_factors`, which resets to the fixed #3
     (`0x00B45A40`). A government change **shocks** the factor to #2 and lets
     it **recover** to #3; the port had it drifting the other way.
  2. **Drift sign.** `+` unless limit < value (`CMP EDI,[ESI+8]`, `JGE`), so
     **+2** on every shipped row. Raw event drift, no `abs()`.
  3. **Drift magnitude is CONFIRMED, not assumed.** `[model+0xFAC+0x94]` is the
     attitude-events array (30 triples × 12 bytes, order = `ATTITUDE_EVENTS`);
     offset `0x94` = 12×12+4 = the **drift of triple 12, `government_type`**,
     built-in drift 2 (`0x0042F110`).
  4. **One direction only.** The caller walks the changed faction's records and
     resolves the counterpart's record via `0x00B64C50` (first whose **target**
     is the changed faction), so the factor written is the *counterpart's*
     attitude towards the changed faction. The port wrote both sides.
- **Round 12's "does it rewrite religion?" UNKNOWN is CLOSED: no.** Only
  `record+0x288` = slot 16 (`8+0x28*16`). Religion is slot 15 at `0x260`,
  touched only by `0x00B1B540`; `scal 0x288` finds exactly 3 writers, none at
  `0x260`.
- **Caller chain CONFIRMED — and it is not a UI path:**
  `0x00B1B100` ← `0x008BEAA0` ← `0x00B449F0` = **the peace-terms deal
  applier**. A government change is a **peace-treaty option**, run only when the
  recipient is the local human (`*(0x6649C0()+0x818)`). That is why no vanilla
  save reaches it and why the round 10 factor-16 check could not exercise it.
- **Still open, and it may be a real bug:** `record_index` (`0x0047AA40`,
  `RET 4`) takes **one** key and all 3 writers pass a single government key, so
  the runtime row is `f(single government)`. The file table is `ssii`/16 rows
  and our port keys the map on the **pair**. Round 10's 506/506 only shows #3
  does not vary with the pair; **#2 — which only a change ever reads — is
  unverified.** Needs the 16 shipped rows.
- **Not modelled:** the twin pass `0x00B1B190` sets the changed faction's *own*
  record drift+limit only, from the counterpart's government row, value
  untouched. So no factor on the changed faction's own side currently moves.

### 0-B campaign, round 12 step 5 — government drift on a government change PORTED
- Branch: `work/0b-round12-sandbox`, commit `ebd6635` (7 files, +247/-12).
  Tests: `ntw_sim` **285 pass** (was 284). No new clippy warnings.
- **Claude cherry-pick note:** only `ntw_sim` is compiled/tested here.
  `ntw_campaign`/`ntw_script`/`ntw_ai` only construct `CampaignCommand`s and
  never match it exhaustively, so breakage risk is low but it is **unverified**
  (fresh-target Bevy crash blocks it here).
- The round 10 decode was **incomplete on the drift itself** — only "a
  government change sets it drifting toward #2 (`0x00B1B5A0`)". So the drift
  a turn is tagged **INFERRED** (magnitude 2 from the `government_type`
  attitude event `0x0042F110`; sign taken *towards* the limit), reasoned from
  the `0x00B290D0` clamp (`min(limit)` positive, `max(limit)` negative) and
  every shipped `diplomatic_relations_government_type` row having #3 above #2
  (absolute monarchy vs republic −30 → −100).
- **Three bugs in WIP `3300431` found and fixed** (worth knowing — the WIP was
  pushed as a safety snapshot, not reviewed):
  1. **Drift sign inverted.** WIP wrote fixed `+2` with the limit *below* the
     value, so `step()` clamped to `min(value, limit)` and the factor jumped
     straight to #2 on the first drift step — the opposite of "drifting toward
     #2". Its test even pinned that wrong value as "a known limitation".
  2. **Spurious records.** `relationship_mut` inserts, so the WIP created a
     relationship record for *every* in-game pair on any government change.
     Now only existing records are written; factions out of the game are
     skipped (same walk as `diplomacy_round_end`).
  3. **`GovernmentType` silent fallback.** WIP string-matched with an
     `AbsoluteMonarchy` fallback, which could leave `government_key` and the
     ESF `GOV_IMP` record disagreeing (both hashed in `world.rs:137`).
     Now uses `GovernmentType::db_key`/`from_db_key` and rejects a key the
     model has no variant for (`gov_empire`, the 4th `government_types` row).
- New test `government_change_skips_the_dead_and_rejects_the_unknown`.
- **Still UNKNOWN:** callers of `0x00B1B5A0` (so no vanilla save exercises this
  path — the round 10 factor-16 save check, 506/506, was not re-run); whether
  `0x00B1B5A0` also rewrites the `religion` factor that `0x00B1B540` sets;
  whether both sides of each record are written.

### 0-B campaign rules, round 12 step 2 — spawn path mapped
- Branch: `work/0b-round12-sandbox`, commit `dea6faf` (notes only, +11).
- Ghidra (`NR-fc-ghidra`): slot 8 `0x00B5C1B0` calls `0x00B5C2D0`, then two
  context paths (`0x009D3A40` slot +0x3c; `0x006649C0` slot +0x38 gating
  item slot 5) or resets item+0x18 — structure CONFIRMED, meaning UNKNOWN.
  `0x00B71FB0` tail: pair list via queue +0x20, `0x00B0A270` records,
  `0x00B5C110` drop-gate. Placement, `num_men` size, garrison join all
  still UNKNOWN (no `unit_stats` reader on path; `rules.men` PROVISIONAL).
- Tests: `ntw_sim` 283 + `ntw_campaign`/save suites all pass.

### 0-G characters, settlement tabs mapping
- Branch: `work/sandbox/0g-characters`, commit `45783b4` (2 files, +99/-1).
- Naval tab: no `GenerateNaval*` in `lua_api.txt` — INFERRED reuse of
  `GenerateRecruitmentPanel` (generator name UNKNOWN). Infrastructure:
  `construction_manager.GenerateFortConstructionPanel` CONFIRMED, model
  takes `FORTIFICATION_SLOT`, but NO fort Build/Upgrade/Demolish commands
  exist (follow-up). Agents: `agents_manager.GenerateAgentsPanel`
  CONFIRMED, roster = `garrisoned_in == region`, show-condition UNKNOWN.
- Tests: `ntw_sim` 283 pass.

### 0-D units, round 5 — +0x1B0 trained-test trace
- Branch: `work/sandbox/0d-units`, commit `55d3044` (notes only, +38).
- File: `analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` (new "Round 5" + §9.4).
- Ghidra (`NR-s1-ghidra`, read-only): branch shape CONFIRMED — `0x006631A0`
  reads `this->vtable[+0xA8]()` as parent; if parent, stance 0, and
  `parent->vtable[+0x1B0]()` in {0,1} → stance 4 (`STAND_TRAINED_IDLE`),
  else stance 0 (`STAND_IDLE`). CONFIRMED negatives: setup vtable
  `0x01333714` returns 0 at +0xA8 (branch dead at setup); `0x0133AF38`
  slot +0xA8 is void update `0x0066B2F0` (old assumption withdrawn);
  battle-unit vtable `0x01321298` has voids at both slots. Adjacent
  CONFIRMED: `0x00664A80` reads `soldier[+0x1EC]->[+0x1B0]==0`.
  UNKNOWN: the parent class carrying the real +0x1B0 getter + semantics.
  Next lead recorded: enumerate `CALL [+0xA8]` sites + 10 unseen callers
  of `0x006631A0`. Tree byte + flag cloth untouched.
- Tests: `ntw_formats` 137 + `ntw_sim` 283 passed, exit 0.

### 0-D units, round 6 — +0xA8 getter found
- Branch: `work/sandbox/0d-units`, commit `587b137` (notes only, +71/-1).
- CONFIRMED: full `0x006631A0` caller list (13 sites, 5 previously unnamed).
  Real `+0xA8` getter: battle soldier vtable `0x0132BE3C` slot 42 =
  `0x006A7DD0` — first non-null `(*sub+0xA8)()` over sub-object array
  `[+0x670]`/`[+0x66C]`, else 0. Lifecycle CONFIRMED
  (`01333714` setup → `0132B7D8` → `0132BE3C` activator). Two more +0x1B0
  consumers found (live update `0066CE00` flag write; `00655250` test).
  INFERRED against training-level hypothesis: values 6/7 exceed enum 0–5;
  0/1 = mob/rabble contradicts "trained". Parent class + value semantics
  remain UNKNOWN. Next: find the `+0x670` array writer.

### 0-B campaign rules, round 12 step 3 — context slots traced
- Branch: `work/0b-round12-sandbox`, commit `0b56fae` (notes only, +34).
- Item slot 5 = `0x00B1B480` land / `0x00B1B4B0` naval (unregister, context
  register, item+0x18=y). `0x008CF810` = vector push-back: land →
  context+0xD4, naval → context+0xC4. Context accessors `0x009D3A40/60/80`
  = holder +0x2A8 deref then virtual +0x3C/+0x38/+0x44. No num_men reader,
  no placement branch on this path — it's list bookkeeping. Next: type P
  at the `0x00B58DD0` call, find the list consumer (force/unit creator).

### 0-G characters, demolish — PORTED
- Branch: `work/sandbox/0g-characters`, commit `0ad4339` (5 files, +144/-4).
- Ghidra (`NR-sb-ghidra-0g`): `DemolishBuilding` → queue id `0x84`,
  `DemolishFort` → `0x89` (slotless, fort implicit), `CanDemolishBuilding`
  = slot-resolve && not settlement_road && slot-free check && selection.
- Code: `CampaignCommand::DemolishBuilding` (commands.rs), `can_demolish()`
  + `demolish_building()` (capture.rs, immediate removal, PROVISIONAL no
  refund, no script event), test `buildings_and_forts_are_demolished`.
- Tests: `ntw_sim` 284 passed, 0 failed. Build/Upgrade/Repair/Cancel for
  forts already existed via `ConstructBuilding`/`RepairBuilding`/
  `CancelConstruction` + `FORTIFICATION_SLOT` (CONFIRMED by grep).

### 0-A battle, formation/scale sweep — no port, leads recorded
- Branch: `work/sandbox/0a-battle`, commit `04c5351` (notes only, +21/-1).
- Formation `+0x670`: exe-wide sweep finds NO formation writer (only =0
  stores, int counter, DB cache pointer). Consistent with always-0,
  unproven → stays UNKNOWN; `FORMATION_RADIUS = 0` stands. Next: debugger
  write-watchpoint. Garrison cap `+0x6C`: body CONFIRMED
  (`min(slots, building+4→+0x54→+0x6C)`), type-record source UNKNOWN.
  unit_scale: settings layout CONFIRMED (`0x00878060`), multiplication
  site UNKNOWN. Next: decompile the 4 converter callers, hunt the MULSS.
- Tests: `ntw_sim` + `ntw_ai` 346 passed, 0 failed.

### Ports, one-cell rims — failing-case table + H1 killed
- Branch: `work/sandbox/ports`, commits `c9be371` (+54) + `750e564` (+52).
- §15: 10/26 sample table (19 single-cell, 7 multi; 18 ours-bigger /
  7 ours-smaller; 21/26 garrisoned, no enrichment). Flood-entry locate
  failure ruled out 26/26. §16: H1 (node-point/tie-break) killed —
  `0x00B11820` cost pairs + tie = point-to-edge distance CONFIRMED, but
  our `zoc::reach` is numerically identical on every cost path; only the
  tie channel (±1e-3) is missing, ruled out by size. H2 (other-obstacles
  cut-in) next.
- Tests: `ntw_sim` 283 + `ntw_campaign` 29 + integration 64, all green.

### 0-E UI, diplomacy negotiation — layout mapped + locked
- Branch: `work/sandbox/0e-ui`, commit `79a2884` (2 files, +199/-2).
- Layout CONFIRMED read-only: `ui\campaign ui\diplomacy_panel`, Version039,
  351 components + 4 templates + 3 luac drivers. ClipChildren=1 on exactly 21
  nodes, UseGlobalClicks=0 everywhere, DrawMode 0 by inheritance (rule lives
  in `UI_LAYOUT_FORMAT.md`, not CAMPAIGN_DATA — noted). Button→panel map
  from inline layout Lua (offer/answer rows, subpopups, cancels).
- Wired: new install test `diplomacy_negotiation_layout_fields`
  (`real_install.rs` +109) locking the contract. All 16 `negotiation:*`
  script shapes UNKNOWN → actions notes-only + 6-item TODO
  (`UI_FIDELITY.md` +92, new §4). Probe file created, run, deleted.
- Tests: lib 137 passed; new test passes (`--ignored`, Steam read-only).

### 0-E UI, campaign save naming — layout mapped + locked
- Branch: `work/sandbox/0e-ui`, commit `4151c84` (2 files, +209/-2).
- Layout CONFIRMED read-only: `ui\campaign ui\load-save_game`, Version039,
  57 components (title frame, map panel, sortable list, filename panel,
  Ok/Cancel → `OnAccept`/`OnCancel`). Text entry matches `file_requester`
  contract. ClipChildren=1 on `Flags` + `list_clip` only.
- Name validation is script-side (`ValidateFilename`, `DefaultSaveName`,
  `CheckDuplicate`, overwrite confirm) — all shapes UNKNOWN, 8-item TODO.
  Locked with `campaign_save_naming_layout_fields` test; diplomacy test
  still passes (no regression).

### 0-A battle, Austerlitz window close — root cause analyzed
- Branch: `work/sandbox/0a-battle`, commit `085c964` (callback fix
  already applied).
- Root cause: battle file `duration=60` vs 64s cutscene.
  Evidence: Austerlitz script has 64s cutscene; `victory.rs` times out
  when `duration < battle.time_seconds()`; battle time advances during
  Conflict-phase cutscene. "No windows are open, exiting" at ~60s matches.
- Verification needed: read actual `duration` from
  `data/napoleon_historical_battles/austerlitz/austerlitz_battle.xml`.
- Fix options: (A) increase duration in data, (B) pause battle time
  during cutscenes, (C) ignore timeout while cutscene active.
- Tests pass: `ntw_script` 22, `ntw_sim` 283, `ntw_ai` 39, `napoleon` 37.

## Still running (sandbox branches — 4 active)

- `work/0b-round12-sandbox` step 5 — government drift (0x00B1B5A0).
- `work/sandbox/0e-ui` follow-up — diplomacy negotiation Ghidra (19 shapes).
- `work/sandbox/0a-battle` next — formation +0x670, garrison cap +0x6C,
  unit_scale, experience/fatigue port.
- `work/sandbox/0g-characters` next — spare §0 area (campaign visuals
  or 0-D units or 0-C leftovers). Characters area done.
### 0-C middleware round 7 — vtable premise REFUTED, real layouts confirmed
- Branch: `work/sandbox/0c-middleware`, commit `44fc713` (notes only, +73).
- **REFUTED: `0x01469494` and `0x0145D504` are NOT vtables.** They are global
  `UIFileIn` file-handle objects (`ptrs:` shows data, not code pointers;
  both start with the `UIFileIn` vtable `0x0130BC60`). All four xrefs are the
  **5th argument** of `0x010CB060`, stored as the new object's first member.
  Consequence: the cue-carrying anim object is **not polymorphic at +0**, so
  there is no slot-7 vcall. Round 6's "update = vtable slot 7" line is DEAD.
- Also killed: round 6's "packed-set consumer `0x00DD5E50`" is just
  `this->field_0xC = record`, a generic
  `DATABASE_TABLE<EFFECT_RECORD / ANIM_REFERENCE_POSE_RECORD>` field applier.
  It never touches a cue list.
- **CONFIRMED (new):**
  - Cue container 0x1C bytes; ctor `0x00DF0240` = {+0 tag 1, +4 count,
    +8 data, +0xC `UIFileIn*`}; parser `0x00E25C30`.
  - Battle global cue list at `0x01650414` (count `0x01650410`,
    cap `0x0165040C`, doubling via `0x00444B40`).
  - Sound-side cache: hash map at owner+0x100 / buckets +0x104; insert
    `0x00E69BF0`, hash `0x00E5F310`, compare `0x00E57960`; entry+0xD0 store
    at insn `0x00E6157C`.
  - Anim object is 0xA4 bytes (`FUN_010D3140(0xa4)`), cue list at **+0xA0**.
  - `0x00F862E0` packs 10 anim objects into a 0x34-byte walk-anim struct.
  - `0x00E615F0` walks 0x10-byte records → `"Animations/BattleConfiguration/"`
    + name → `0x00E611E0`.
- **Per-frame tick still NOT found** (negative evidence): 6286 `call [reg+0x1C]`
  sites in 0x00480000..0x00600000 and 4097 in the animation library → +0x1C is
  a generic slot; 529/538 `+0xA0]` hits unrelated; `+0xD0]` in the sound module
  has **no reader**; `+0x100]` has **no lookup**; zero direct `call 0x010…` from
  0x00E00000..0x00E80000; main loop `0x00485B90` (6186 bytes, 109 callees)
  per-frame body is the **campaign/world** branch — battle branch not isolated.
- No Rust written (brief required both halves CONFIRMED). Next leads: per-frame
  consumer of the `Animations/BattleConfiguration/` objects; the walk-anim-slot
  reader; `insn:0xa4]` scan; delta-time float fns in 0x010C..0x0122.

### Paused: `work/sandbox/0d-units` (+0x670 array writer), `work/sandbox/ports`
  (residual rim misses).

## Notes for Claude

- Sandbox workers never push to this repo and never touch `NR-old-battle`.
  Steam install treated read-only, no Python, own code only.
- Housekeeping: this checkout had an untracked 147-line `README.md` that
  blocked the fast-forward to `3b7762d` (which adds a 108-line README).
  The untracked copy is backed up outside git; nothing of it was committed.
- Fresh-target Bevy builds crash here compiling `bevy_scene`
  (`STATUS_ACCESS_VIOLATION`); sandbox workers run unit tests only, and
  game checks reuse the prebuilt `target/debug/napoleon.exe`.

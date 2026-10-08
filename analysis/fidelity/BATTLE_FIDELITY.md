# Battle-side fidelity pass (§0-A)

Worker: fidelity-battle (`work/fidelity-battle`). Ghidra: own copy `%USERPROFILE%\Documents\NR-f0a-ghidra\NTW.gpr`,
script `analysis/fidelity/ghidra_scripts/FidDecomp.java` (copy of the AI worker's `AiDecomp.java`; same commands:
`fn:`, `callers:`, `lst:`, `dw:`, `xref:`, `scal:`, `vt:`). Headless run (Git Bash):
`analyzeHeadless.bat %USERPROFILE%\Documents\NR-f0a-ghidra NTW -process Napoleon.exe -readOnly -noanalysis -scriptPath <...>\ghidra_scripts -postScript FidDecomp.java <targets> <out> [maxLines]`
(about 7 s per batch). Decompiled output stays in the scratchpad; only specs are written here.

Tags: CONFIRMED (code/bytes), INFERRED, UNKNOWN. Addresses are `Napoleon.exe` VAs.

## Where I am / what's next (updated with each push)
- **Main port of the sandbox battle rules (2026-10-05, `work/fidelity-battle-w5`).** §18a, §19a and
  §§53-58 below were researched in the sandbox fork (`Rosebuddyy/napoleonrust-sandbox`, frozen at
  `90bfc1c`) and this file is now the sandbox's copy of it. Main's tree was identical to sandbox
  `61e4109`, so each port is the sandbox diff from there. **Read the "ported" wording in §§53-56 as
  the sandbox's state; what main has is exactly this list:**
  - **In main:** (1) melee has NO experience term, CONFIRMED negative (§57 (1)), test
    `no_experience_term_in_the_melee_chain` + `kill_chance_clamp_is_1_to_990` and the `KvRules` docs,
    from sandbox `420e352` (re-landing `85d91c9`/`aee8f58`). (2) the `kv_rules` list position is the
    adapter slot, CONFIRMED (§57 (2)), test `kv_rules_list_order_is_the_exe_slot_index`, same commit.
    (3) `ntw_sim::battle::unit_scale` (steps `0x01392770`, clamp `0x004A6540`, the truncating
    `MULSS` at `0x004A67E5`, `PREFERENCE_DEFAULT = 2` from `0x00404230`) plus `Battle::unit_scale`
    (default 1.0), `men_at_scale`, `set_unit_scale_step`, `set_unit_scale_setting`, CONFIRMED (§18a),
    from `e2fdafe` and the module's later rounds `7c46287`/`880c7e2`/`66248c7`. (4) the
    `unit_stats_land_experience_bonuses` and `unit_stats_naval_experience_bonuses` decoders (v0, 10
    rows each; CONFIRMED layouts and readers `+0x20`, `+0x24/+0x28`, `+0x1C/+0x20`; the other columns
    UNKNOWN), the `GameDatabase` accessors and the XP-adjusted-cost formula of `0x00ED49A0`, from
    `bd5d4f9`, `2054325`, `e2fdafe` (§19a). (5) `unit+0xD48` is the experience level (CONFIRMED,
    §52 (4)): `LandUnit::experience` replaces the INFERRED unit-id stand-in in the waver/rout timers
    (`0x0053E4D0`/`0x0053A720`), the fatigue term column 6 of the land table (`0x00670F40`,
    `Battle::experience_fatigue`) is wired, and the battle setup fills both from the battle file's
    `unit_experience level` and the custom armies, from `631a3ae` and `2054325`. (6) §58 (1)-(3): the
    two `napoleon` install tests `battle_file_experience_reaches_every_unit` and
    `the_experience_level_moves_fatigue_and_the_morale_timers` and the build-order comment in
    `historical_armies`, from `803493a`/`887c76b`; the §58 text itself (Austerlitz `duration` = 2400 s,
    the 60 s root cause REFUTED) from `46e77d7`/`a51f430`/`3ce155f`.
  - **Not in main, on purpose:** the `crates/napoleon` `unit_scale` wiring (`SetupData::unit_scale`,
    `unit_scale_setting()`, the scaled `make_unit`). Two reasons: the link from the `gfx_unit_scale`
    preference to the battle-settings key `0x0B` that `0x004A6540` reads is INFERRED (§18a), and
    `make_unit` also builds battle-file units, which the exe does NOT scale (`0x00513440`, §39), so
    the sandbox wiring would thin historical battles the original leaves alone. The `setup_battle`
    script callback (`085c964`) is PROVISIONAL with no exe evidence and is left out. The campaign
    XP-cost wiring of §54 (1)/§55 (1)-(2) is in `ntw_sim::campaign`, `ntw_campaign` and
    `ntw_script::ui`, outside this port's files. The `ntw_formats` install test
    `battle_file_experience_coverage` (§58 (2), `a04220c`) is outside this port's files too; its
    measured numbers stand in §58 (2). `napoleon/src/battle/view.rs` (sea surface) and
    `mod.rs` (`labels` visibility) belong to the water and campaign-label ports.
  - **Still UNKNOWN, nothing invented:** formation `+0x670`, garrison cap `+0x6C`, the `unit_scale`
    reader (key `0x0B`, the modes 2/4 minimum), the `unit+0xD48` writer.
- **0-A round 19 (§58, 2026-10-05, worker `work/fidelity-battle-w3`, rebased on `fb9ad4d`).** Two
  of the four open 0-A items are now settled; **both needed no debugger**.
  - **§57 (5) — the battle-file `unit_experience`: SETTLED, and it is a load-bearing field.** The
    *consumers* were never missing: the three formula readers (§52 (4), §19a — the
    `unit_stats_land_experience_bonuses` fatigue term `0x00670F40` and both morale timers
    `0x0053E4D0`/`0x0053A720`) are already ported, and the rest of the twelve `+0xD48` readers are
    the unit-card panel's `"Experience"` (`0x005ABF40`/`0x005CD340`), the campaign script getter
    table and three unattributed getters. What is UNKNOWN is the **writer**, and it is now pinned
    down: the parser's one card call is `0x0050EB77 → 0x00513440`, a plain copy whose only byte
    store is `card+0x110` = `unit+0x12C`, **not** `unit+0xD48` — so a write watchpoint is the only
    way, and it joins the batched cdb session. **The field itself is CONFIRMED live**: 25 of the 32
    shipped battle files carry `<unit_experience level=...>` on **1267 of 1350** units, and the
    levels are **exactly 0..9** — the ten rows of the two experience tables, which is independent
    proof the field *is* the chevron level. With the real tables the three effects are
    fatigue rows `[0,0,0,0,0,-1,-1,-2,-2,-3]`, waver **40 → 85** ticks, rout **600 → 420** ticks.
    New tests: `battle_file_experience_coverage` (`ntw_formats`), and
    `battle_file_experience_reaches_every_unit` +
    `the_experience_level_moves_fatigue_and_the_morale_timers` (`napoleon`), the first of which
    walks all 25 installed land battles unit for unit. Writing it also found a real ordering trap in
    `historical_armies` (all alliances' armies first, then all alliances' reinforcements).
  - **The Austerlitz window: the recorded root cause is REFUTED and the symptom does not
    reproduce.** `AUSTERLITZ_WINDOW_CLOSE_ANALYSIS.md`'s "`duration` = 60 s" is wrong — the file
    says **2400 s** (1800–3000 s over all 32 files), so its "pause the clock during cutscenes" and
    "ignore the timeout in a cutscene" fixes would have been **regressions**; `victory::check` is
    left exactly as §9 has it. Three runs on this build: a bare `--battle-key NHB_Austerlitz` sat in
    Deployment for 150 s (window open), the same run with
    `--battle-ui-click wait,button_battle_start` at `NAPOLEON_AI_SPEED=8` ran the whole battle and
    **ended by itself at 712.1 s, `Won { side: 0 }`** with the window still open, and
    `--battle --screenshot` exits 0 **with** its picture. The old exits were harness exits: a bare
    `--battle-key` never leaves Deployment (INFERRED, matches `hud::harness_clicks`), so the
    battle clock never even started. Recipe recorded in §58 (4).
  - **Still open, all debugger-gated:** formation `+0x670`, garrison cap `+0x6C`, the `unit_scale`
    reader, and now the `unit+0xD48` writer
    (`analysis/fidelity/debugger/f0a_battle_probe.cdb.txt`). Nothing invented for any of them.
- **Manager merge, 0-A round 18 re-landed (§57, 2026-10-05).** The worker's branch arrived 246
  commits late and its section numbers collided with §§53-56, so only the new part was taken: the
  three test/doc changes. **CONFIRMED: the original's melee has NO experience term** — all eleven
  factors of `0x00DAB5F0` are named in the exe and none is experience, the chain's rules slots are
  `4/0x10/0x14/0x18/0x5C/0x60/0x64/0x68` and never slot 0, and a whole-binary sweep for
  "experience" strings only finds the battle-file parser. So `relative_melee_experience_multiplier`
  (key 0) is loaded and never read, and leaving experience out of melee is **faithful, not a gap**.
  Also CONFIRMED: the rules-adapter slot is the `kv_rules` list position. The manager's
  **known failing** `real_ai_against_the_models_default_behaviour` now passes (Austria 2/8 v 1/8,
  France 8/8 v 7/8) — assertions hold, the AI is still PROVISIONAL in strength.
  - Next: items 2-4 (formation `+0x670`, garrison cap `+0x6C`, the `unit_scale` reader) still wait
    for the debugger batch (`analysis/fidelity/debugger/f0a_battle_probe.cdb.txt`). §57 (5) narrows
    one more: whoever reads the battle-file `unit_experience` level after the parser stores it. The
    Austerlitz length comparison is still open.
- **Sandbox 0-A round N+4 (2026-10-05).** Base merged clean (fast-forward to `sandbox/main`
  `9b954e6`, no conflicts). WIRING + TESTS + two time-boxed hunts:
  - **`unit_scale` now end-to-end in `ntw_sim`** (item 3): the whole preference-index → step-table →
    clamp → truncation decision is `unit_scale::scale_for_setting` / `men_for_setting`, tested
    against a real `Battle` — the 0.75 factor (`gfx_unit_scale 2`), the **7 → 5 truncation** and the
    fact that the thinned men reach `side_strengths` (`0x00539E80`). `crates/napoleon`'s
    `unit_scale_setting()` is now two lines thinner and still unverified-for-compilation.
  - **The XP writer sweep is a clean CONFIRMED negative:** an exe-wide `scal:0xD48` finds **12 hits,
    all `MOVZX` reads and no store at all** — the experience byte is never written by a direct
    instruction, so neither the ESF index nor a post-battle gain can be found by looking for one.
    The one `LEA [ECX+0xD48]` is the **campaign model's** container in the constructor `0x008751E0`
    (0 callees), and the one campaign-region "Experience" referent `0x009AA5E0` is a script **getter**
    table, not a setter. `str:army_experience` has no referent (generic reward system). **Nothing
    invented; the gain path stays UNKNOWN and now needs a debugger.**
  - **Garrison cap `+0x6C`: both fresh ideas exhausted** (clean negative). `0x0052F3A0` — the only
    caller of `0x00688DD0` — passes the **table's own row** from
    `DATABASE_TABLE<BATTLEFIELD_BUILDING_RECORD>::record_index`, not a copy, and `0x00688DD0`'s
    whole body touches `+0x6C` zero times. Only `0x00E4E8E0`/`0x00E3A9E0` remain.
  - Item 2's integration test already existed and passes (a real start position through `read()`,
    treasury asserted); not duplicated. `recruit` legitimately only ever raises a rank-0 unit.
  - `FORMATION_RADIUS = 0` untouched, no value invented. Checks: **485 passed / 0 failed** across
    `ntw_data ntw_sim ntw_campaign ntw_ai`, 15 against the real install, no clippy warning in a file
    this round touched. `crates/napoleon` not built. Details in **§56**.
- **Sandbox 0-A round N+3 (2026-10-04).** SEAMS CLOSED. Both XP seams of §54 (1) are landed:
  `ntw_campaign::rules_from_db` copies `db.{experience,naval_experience}_cost_rows()` into
  `CampaignRules::xp_cost`, so **every loaded campaign now has live experience-adjusted costs** (it is
  the only producer of the rules, via `read_esf`), and `CampaignUnit` has an `experience` field with
  `World::unit_experience` / `set_unit_experience`, which `economy::unit_experience` really reads
  (upkeep picks a rank up as soon as anything sets one). The **ESF index is still UNKNOWN and was not
  invented**, so loaded units stay rank 0 — the seam is live, the data is not; the new tests drive
  rank 5 through `economy::recruit_cost` and through a real `CampaignModel::recruit` on a loaded
  start position. The recruitment card's price now goes through `economy::recruit_cost` too
  (`ntw_script/src/ui/campaign.rs`, surgical, `cargo build -p ntw_script --all-targets` clean — 0-E
  please note) and `unit_entry` sets `Experience` from the model. **§54 (3)'s garrison chain was
  wrong** and is re-derived: `garrison+4` is the building instance, `[instance+0x54]` is the
  `BATTLEFIELD_BUILDING_RECORD`, so `cap = [record+0x6C]` — a runtime field of the record, in the
  same tail cluster as `+0x5C`/`+0x64` that `0x00688DD0` already reads. The vtable-dispatch trick
  (0-C's) was applied to the instance's vtable `0x01338CD0` and works as a technique (all five real
  slots have 0 direct callers) but none of them is the cap; `0x01321294` is a shared base vtable, a
  recorded dead end. Details in §55. `FORMATION_RADIUS = 0` untouched.
- **Sandbox 0-A round N+2 (2026-10-04).** WIRING + TESTS round: `XpAdjustedCost` is now **called from
  the campaign's recruitment and upkeep paths** (`economy::recruit_cost` /
  `economy::unit_upkeep_with_experience`, both through `CampaignRules::xp_adjusted_cost`), the naval
  twin is wired the same way, the `gfx_unit_scale` steps are locked down by tests (setting 2 = 0.75),
  and the garrison cap's object chain is corrected (§54). Details in §18a, §19a, §53 and **§54**.
  `FORMATION_RADIUS = 0` untouched.
  - Wired: `ntw_sim::campaign::rules::{XpCostRow, XpCostTables}` + `CampaignRules::xp_cost` and
    `CampaignRules::xp_adjusted_cost(naval, rank, base)`; `economy::recruit_cost` (called by
    `CampaignModel::recruit`, so the treasury, the queue item and the "not enough money" error all
    carry the adjusted figure); `economy::unit_upkeep_with_experience` (called by
    `faction_upkeep_with`). Both reproduce `0x00ED49A0`: `flat + ROUND(base × mult)`, the naval
    table for a ship, and the untouched base for a rank with no row. **No campaign number moves
    yet**: the tables stay empty until the loader copies them in (§54 (2)), and the campaign model
    still has no per-unit experience field, so every unit reads rank 0 — both seams are one line
    each and are listed as the open items.
  - Tests: `campaign::tests::{veteran_units_cost_more_than_recruits,
    xp_costs_are_off_without_the_tables, recruiting_charges_the_xp_adjusted_cost,
    faction_upkeep_is_the_sum_over_its_units}`, `battle::unit_scale::{the_four_settings_scale_the_men,
    default_setting_is_three_quarters}` (+ the new `unit_scale::PREFERENCE_DEFAULT = 2` constant the
    head-to-head comparison maths uses), `ntw_data::{experience_cost_rows_and_rank_boundaries}` and
    the real-install rank sweep. `cargo test -p ntw_data -p ntw_sim -p ntw_ai` → **376 passed,
    0 failed** (18 ignored); `cargo test -p ntw_data -- --ignored` → 14 passed against the real install.
- **Sandbox 0-A round N+1 (2026-10-04).** `unit_scale` SOLVED and ported; the `+0x24`/`+0x28` pair
  identified (it is a COST, not a fatigue term) and the naval twin table decoded; the garrison cap's
  record narrowed but still UNKNOWN. Details in §18a, §19a and §53. `FORMATION_RADIUS = 0` untouched.
  - `unit_scale` (CONFIRMED): `0x004A6540` returns `clamp(step, 0.1, 1.0)` from the four steps
    {0.25, 0.5, 0.75, 1.0} (`0x01392770`), and `0x004A6600` (the army → battle unit creator) multiplies
    the army unit card's **u16 men at `+0xA`** by it — `CVTDQ2PS` / `MULSS XMM0,[ESP+0x30]` (which is the
    scale, because of the `PUSH 0x131005f` at `0x004A67C9`) / `CVTTSS2SI ESI,XMM0`, i.e. **truncated** —
    and hands the result to `0x005363C0` → `0x00513320`, which stores it at card `+0xC8`/`+0xCC`
    (`in_ECX[0x32] = in_ECX[0x33] = men`), the field the battle reads as the men (§6). Naval units take a
    **byte** at card `+0x0E` and are NOT scaled (`0x004A6A06`, no MULSS). Ported:
    `ntw_sim::battle::unit_scale` (`STEPS`, `step`, `clamp`, `scaled_men`), `Battle::unit_scale` +
    `men_at_scale` / `set_unit_scale_step`, and `SetupData::unit_scale` read from the `gfx_unit_scale`
    preference. The battle-file path stays unscaled (§39), as in the exe.
  - The preference is CONFIRMED: `0x00404230` registers `gfx_unit_scale` as an **environment variable**
    preference (int, storage `0x0149D880`, help "Set unit scale. 0 - lowest, 3 - ultra"), and the real
    `preferences.script.txt` has `gfx_unit_scale 2`. The exe's default is therefore **2 = 0.75**; the model
    keeps 1.0 by default (deliberate deviation, see §18a).
- **Sandbox 0-A (§52 (4), 2026-10-04).** ONE item: `unit+0xD48` is the experience level (CONFIRMED:
  exposed as "Experience" by `0x005ABF40` and `0x005CD340`; read by `0x0053E4D0`, `0x0053A720` and the
  fatigue bonus `0x00670F40`). Ported the timers: `morale::waver_timeout`/`rout_timeout` now take
  `experience` (was `unit_index`), `LandUnit::experience` wired from battle-file `unit_experience` and
  custom armies (0 elsewhere). Ghidra scratch in `target/tmp/exp*_out.txt` (read-only headless runs).
  Next lead (not ported): fatigue `+0x20` experience bonus in `0x00670F40` (needs the
  `unit_stats_land_experience_bonuses` table in `ntw_data`).
- **Sandbox 0-A follow-up (formation radius / garrison cap / unit_scale, 2026-10-04).** Own Ghidra copy
  `%USERPROFILE%\Documents\NR-sb-0a-ghidra` (copied from `NR-f0a-ghidra`, untouched); headless read-only runs,
  scratch in `target/tmp/sb0a_f67*_out.txt`. NO PORT (nothing newly CONFIRMED with a portable value):
  - `+0x670`: exe-wide sweep (0x00400000..0x01000000) finds no formation writer — only `= 0` stores in other
    classes' constructors (`0x0059EC50`, `0x0068C750`, `0x0071FC80`; entity `+0x670` is the soldier-list pointer,
    cf. `0x005DFA90`), int-counter use (`0x0071C740` INC), a campaign DB-table cache pointer (`0x00E2E0B0`), and
    stack slots. New readers via `unit+0xADC`: `0x0075BE30`, `0x0057BEB0` (float radius, as `0x00701310` /
    `0x006C9E40`). Rect overlap `0x0055B220` / `0x005774A0` do NOT read [11]: early-out uses `min(w,h)/2 + N`.
    Consistent with always-0 (tests then use N only) but not proven — still UNKNOWN; `FORMATION_RADIUS = 0` stays.
    Next lead: debugger write-watchpoint on a live formation's `+0x670` (the §52 (2) probe plan).
  - garrison cap: `0x008554F0` body CONFIRMED (`min(slot count via 0x006F22C0, (building+4)->+0x54 -> +0x6C)`;
    0 without `+0x1E8`); callers CONFIRMED (`0x00855560` capped add, `0x00855520` remaining = cap − `0x008554E0`,
    `0x0059A500` stores the cap as a UI float). Type-record source still UNKNOWN → no port. Next lead: full
    decompile of building setup `0x00688DD0` (2161 addrs; find the byte `+0x54` writer — the `in_ECX[0x54] = 0`
    seen so far is dword-index matrix init, not the byte field).
  - unit_scale: settings layout CONFIRMED (`0x00878060`: `+0` = scale float, `+1` flag byte,
    `[8..0xC]` = the 20 / per-step / remaining args); index→scale `0x00DAFBB0` callers listed (`0x004A6540`,
    `0x004765F0`, `0x009FAB10`, `0x004795F0`). Multiplication site still UNKNOWN → no port. Next lead: decompile
    those four callers to find which float feeds battle unit construction, then hunt the men MULSS there.
  - Checks: `cargo test -p ntw_sim -p ntw_ai` green (exit 0; 346 passed, 0 failed across 8 suites). NO PUSH.
- **Round 17 (§52, 2026-10-04, in progress).** S3 SOLVED statically and ported (`shooting::volley_plan`): the fire
  orders are per-drill order objects (`0x00553C30` picks them); with the default drill `fire_volley` only the front
  rank (the first `files` soldiers, `0x005694D0` / `0x00539910`) fires; skirmishers (unit `+0x1A5`) and mounted units
  fire with every man; mass_fire = every man; platoon/rank fires = 3 groups / up to 3 ranks in turn. The half-the-men
  PLACEHOLDER is gone. Combined debugger probe for the manager: `analysis/fidelity/debugger/f0a_battle_probe.cdb.txt` (copy in `target/tmp/probes/`) (S3
  confirmation, formation `+0x670`, garrison `+0x6C`, unit_scale readers).
  - Done since: Waterloo 676.1 → 333.9 s (Allies now win, §52 (5)); campaign weather pick ported; checks pass except
    the user-save test (environment). Austerlitz windows close by themselves in both builds (to rerun).
  - Next: item 6 experience/chevrons (§52 (4) lead); items 2–4 wait for the probe log
    (`analysis/fidelity/debugger/f0a_battle_probe.cdb.txt` (copy in `target/tmp/probes/`)).
- Round 7 done (§32–§36): campaign battles take each side's researched technologies (helper ready; the campaign
  does not launch battles yet); deployable defences placed as the exe does, drawn, and acting as obstacles,
  charge stoppers and missile cover; building garrisons (models_building fire lines, defendable rule); the skirmish
  default decoded (off for every shipped unit); FPS run on Austerlitz.
- Round 8 done (§37–§39): defence casualties from the soldiers' collision handlers (chevaux 0.75, stakes 1.0
  head-on); walls = `fort` category; capacity rule decoded; unit size and commander-alive resolved; checks in §39.
- Round 9 done (§40–§42): hits and hit points decoded (defence contact kills riders outright); garrison data and slots
  decoded; movement-modifier reader still not found (§41).
- Round 10 done (§43–§44): sweep table §43; movement-modifier columns, slope speed, on-field rule (reinforcements),
  garrison slot radius decoded and applied.
- Round 11 (§45–§46): Waterloo end analysed; S13 casualty rings, S4 melee cadence (5 s), S6 under-fire timers done;
  S3 per-soldier firing still open.
- Round 16 (§51): the last S3 attempt did not find the ranged engagement creator; S3 is parked (a debugger would
  settle it). The worker moved to the 0-D slot (NR-fidelity-units).
- Round 15 done (§50): the infantry issuer is not `0x005DD3D0`; the fire flag only makes every soldier ready, and
  firing comes from the engagement object at soldier `+0x628` (slot `+0x1A8` starts the musket FSM). "Every man
  fires" was tried and reverted; the half-the-men PLACEHOLDER stays. Sweep S8+ reviewed (§50).
- Round 14 done (§49): units leave the field at the playable-area edge (ported); S3 fire issue traced (every idle
  soldier of an ordered member fires; the infantry member gate is still open, half-the-men PLACEHOLDER kept);
  `+0x670` / `+0x6C` time-boxed, not found. Waterloo 676.1 s, Austerlitz 734.1 s (France wins both).
- Round 13 done (§48): routers are not easy prey, but the charge flag lasted the whole melee (now the impact only);
  the FIRE gate is "at rest"; battle weather from the preset (all dry); fatigue_effects columns ported (speed,
  charge, control, attack). Waterloo 666.1 s (France won), Austerlitz 734.1 s.
- Round 12 done (§47): S5 chance-to-hit factors ported (control, visibility, angle, woods cover, army level terms);
  S3 soldier fire state machine decoded and the cartridge pool ported (which men fire is still open); per-tick
  counter clear CONFIRMED; melee clip lengths measured. Waterloo now ends at 229.9 s (was 549.5), Austerlitz at
  754.1 s (was 442.1): see §47 for the attribution.
- Next, by impact (§43): S3 who creates ranged engagement objects (callers of `0x00662E90` with aim actions
  0xE/0xF/0x10, §50); the campaign weather pick (§5); formation `+0x670` writer; type `+0x6C`.

## Resolved / still open

| # | Question (source note) | Answer | Tag | Code |
|---|---|---|---|---|
| 1 | Reload formula (ntw_sim shooting PLACEHOLDER) | §1.1 | CONFIRMED | `shooting::reload_time_seconds`, `reload_skill` |
| 2 | Firing-drill case → kv key | §1.1 | CONFIRMED | `shooting::FiringDrill` |
| 3 | "State 7" that cuts missile range to 80 % | fire and advance (same state gates `fire_and_advance_reload_modifier`) | INFERRED (strong) | doc only |
| 4 | Weather tests `FUN_005DBB30` / `FUN_005DBD60` | weather type 0 = rain, 1 = snow (they pick `idle_rain` / `idle_snow`) | CONFIRMED | `ReloadContext::rain_or_snow` |
| 5 | Unit field `+0xC28` (reload −3/−7) | the morale state mirror: the morale component is embedded at unit `+0xC00` ([0xA] → `+0xC28`, [0xB] → `+0xC2C`) | INFERRED (strong) | `ReloadContext::morale_state` |
| 6 | Waver / rout timer formulas (W1 §12.3 UNKNOWN) | §2.1 | CONFIRMED | `morale::waver_timeout`, `rout_timeout` |
| 7 | Light morale update `0x00585BE0` | §2.2 | CONFIRMED | `morale::light_update` (also run first by `evaluate`) |
| 8 | `add_effect` list semantics | §2.3 | CONFIRMED | `MoraleComponent::add_effect` |
| 9 | Rally test (routing units) | §2.4 | CONFIRMED structure | `morale::rally_test`, `Battle::can_rally`, `battle::strength` |
| 10 | Melee direction `0x006AD890` | §3 | CONFIRMED | `model::melee_dir` |
| 11 | Fatigue action → kv key (`0x00670F40`) | §4 | CONFIRMED (our unit-level action pick is an approximation) | `Battle` step 7 in `model.rs` |
| 12 | "Moving on a slope" test | §4: entity `+0x118 == 1 && (+0x110 & 1 or +0x117 == 0)`; `+0x118` writer found (§4.1), gradient `+0x1A0` writer not found | CONFIRMED test, field meanings UNKNOWN | `FatigueInputs::moving_on_slope` (PROVISIONAL: "moved this tick") |
| 13 | Unit attribute flags (`+0x189`, `+0x18A..+0x190`, record `+0x168..`) → `unit_stats_land` columns | §5 | CONFIRMED offsets/columns, INFERRED names | `attributes::UnitAttributes`, `setup::unit_attributes` |
| 14 | Strength potentials `0x00757120` / `0x007575A0` / `0x007578E0` (class term, morale factor, bonuses) | §6 | CONFIRMED (card-list terms UNKNOWN) | `strength::*` |
| 15 | Fear / inspiration sub-evaluator `0x0053B970`, range test of the unit lists | §2.5, §6.1 | CONFIRMED (formation radius `+0x670` UNKNOWN) | `morale::sub_fear_and_inspiration`, `morale::near` |
| 16 | Fatigue flags `+0x18E/+0x18F/+0x190`, rally / shock flag `+0x189` | §5 | CONFIRMED wiring | `model.rs` (fatigue step, `can_rally`, `morale_inputs`) |
| 17 | Battle climate fatigue terms `+0x2C` / `+0x30` | §4.2: heat / cold fatigue, from `battle_climate_weather_descriptions` cols 7 / 8 | INFERRED (strong) | `Battle::climate_fatigue` (0 until the setup picks a weather: PROVISIONAL) |
| 18 | Ground-type TGA index → name (BATTLE_TERRAIN §10) | §7: the record list is built in name-table order and indexed by the cell byte | CONFIRMED | `ground` docs |
| 19 | `unit_movement_modifiers` column per unit | `0x006543D0` (§44) | CONFIRMED | `MovementClass` |
| 20 | Morale component initial state/timers | §14: Confident, timers −1, persistent bonus by army level | CONFIRMED | `MoraleComponent::default`, `for_army_level` |
| 21 | Army/general sub-evaluator `0x0053BC70`, army destruction `0x00532000` | §10 | CONFIRMED (generals not modelled: PROVISIONAL) | `morale::sub_army_and_general`, `army_destruction` |
| 22 | Neighbour / hill sub-evaluator `0x0053CBD0` | §10 | CONFIRMED (radius, fortification and speed tests INFERRED) | `morale::sub_terrain`, `Battle::neighbours` |
| 23 | `duration` unit, time-out test, battle clock | §9 | CONFIRMED | `victory::check` |
| 24 | `non_playable` scope | §9: alliance and army level | CONFIRMED reads (player choice PROVISIONAL) | `BattleSpec::player_army` |
| 25 | Generals: which unit, rank, army flags | §11 | CONFIRMED (death/flight writers UNKNOWN: PROVISIONAL timing) | `Battle::general_status` |
| 26 | Reinforcement arrival | §13 | CONFIRMED (entry point and walk-on time PROVISIONAL) | `Battle::reinforcements_step` |
| 27 | Script orders skirmish / deployable / special ability / shot type / morale / defend_building / set_invincible | §27, §28 | CONFIRMED commands and fields; skirmish check CONFIRMED (geometry PROVISIONAL) | `battle::abilities` |
| 28 | Technology unlocks of abilities and shot types | §30 | CONFIRMED tables, INFERRED rule (no campaign tech state yet) | `GameDatabase::unit_capabilities_with` |
| 29 | Deployable defence placement and effects | §33 | placement CONFIRMED; effects INFERRED/PROVISIONAL | `abilities::defence_pieces` |
| 30 | Building garrisons | §34 | defendable rule CONFIRMED, fire lines INFERRED, garrison rules PROVISIONAL | `battle::garrison` |
| 31 | Skirmish default | §35 | CONFIRMED (off for every shipped unit) | `Battle::skirmish_default` |
| 32 | Defence casualties | §37 | CONFIRMED triggers and hit strengths; kill chance PROVISIONAL | `Battle::defence_contact` |
| 33 | Wall buildings | §38 | CONFIRMED (category fort) | `setup::battle_buildings` |
| 34 | Battle-file unit size | §39 | CONFIRMED: not scaled at parse / card | `setup::apply_spec_unit` |
| 35 | Movement-modifier column | §44 | CONFIRMED | `ground::MovementClass` |
| 36 | Slope speed and gradient | §44 | CONFIRMED | `Battle::placeholder_movement` |
| 37 | Reinforcement joins the field | §44 | CONFIRMED rule (unit-level rectangle) | `Battle::reinforcements_step` |
| 38 | Chance-to-hit control, visibility, angle, cover | §47 | CONFIRMED (fatigue base column UNKNOWN) | `missile::control` / `visibility` / `angle_judgement`, `shooting::in_woods` |
| 39 | Shot launch elevation | §47 (`0x006A23D0`) | CONFIRMED (low/high, fixed); rocket INFERRED | `missile::launch_angle` |
| 40 | Ammunition | §47: one cartridge pool per unit | CONFIRMED | `shooting::AmmoPool` |
| 41 | Per-tick kill/death counter clear | §47: slot 4 clears them after the push | CONFIRMED | `casualties` |
| 42 | Soldier fire state machine; which men fire | §47, §52 (1) | CONFIRMED states and per-drill shooters (static; runtime probe pending) | `shooting::volley_plan`, `effective_drill` |
| 43 | Charge flag duration | §48: the impact only (soldier action 0xD) | CONFIRMED rule, INFERRED unit-level mapping | `LandUnit::charging_now` |
| 44 | Battle weather | §48: preset `max_weather_type_key`, all `dry` | INFERRED cap | `setup::apply_weather` |
| 45 | `fatigue_effects` columns | §48: speed, charge, control, attack | CONFIRMED readers, INFERRED `1 + value` | `fatigue::FatigueEffects` |
| 46 | Units leaving the field | §49 (`0x005857A0` state 2) | CONFIRMED rule, unit-level rectangle | `Battle::leave_step` |
| 47 | Campaign-battle weather pick | §52 (3), `0x00F5B4D0` | CONFIRMED static (flag meaning INFERRED) | `ntw_data::weather::pick_climate_weather` (not wired: no campaign battles yet) |
| 48 | Formation `+0x670`, garrison `+0x6C`, unit_scale reader | §52 (2) + sandbox 0-A follow-up | unit_scale **SOLVED** (§18a: `0x004A67E5` `MULSS`, truncated, into card `+0xC8`); formation `+0x670` and garrison `+0x6C` still UNKNOWN (§54 (3) corrects the object chain of §53 (4): the cap's `+0x54` is the **slot block's**, not the building's) | `unit_scale::scaled_men`, `Battle::unit_scale` |
| 49 | Experience / chevrons | §2.1, §52 (4), §19a, §54 (1) | CONFIRMED (timers, the `+0x20` fatigue term, and the `+0x24`/`+0x28` XP cost pair, now **wired into the recruitment and upkeep paths**; land + naval tables decoded) | `LandUnit::experience`, `morale::waver_timeout`/`rout_timeout`, `fatigue::experience_bonuses`, `GameDatabase::*experience_adjusted_cost`, `CampaignRules::xp_adjusted_cost`, `economy::recruit_cost`, `economy::unit_upkeep_with_experience` |
| 50 | Naval experience bonuses `unit_stats_naval_experience_bonuses` | §19a, §54 (1) | CONFIRMED (getter `0x00E31710` names it twice; v0, 10 rows, decodes with no leftover bytes) and **wired** as the naval branch of the XP-adjusted cost | `ntw_data::UnitStatsNavalExperienceBonuses`, `XpCostTables::naval` |
| 51 | The `gfx_unit_scale` steps and the exe's default | §18a, §54 (2) | CONFIRMED: settings 0/1/2/3 = 0.25 / 0.5 / **0.75** / 1.0, exe default **2**; `CVTTSS2SI` truncates (7 × 0.75 = 5). UNKNOWN: what fills battle-settings key `0x0B` and the per-mode minimum of modes 2/4 | `unit_scale::STEPS`, `unit_scale::PREFERENCE_DEFAULT`, `unit_scale::scaled_men` |
| 52 | Experience in the melee chain | §57 (1) | CONFIRMED **negative**: no experience term exists. `0x00DAB5F0` names all eleven of its factors and none is experience; the chain reads rules slots `4/0x10/0x14/0x18/0x5C/0x60/0x64/0x68`, never slot 0; a binary-wide sweep for `experience` strings finds only the battle-file parser `0x0050CAE0`. Key 0 is loaded and never read, so omitting experience from melee is faithful | `melee::hit_number` (deliberately no term), test `no_experience_term_in_the_melee_chain` |
| 53 | `kv_rules` adapter slot numbering | §57 (2) | CONFIRMED: vtable method `+4*i` returns key `i`, so the list position is the slot. Anchored by fatigue `+4`, height `+0x10/0x14/0x18`, attack direction `+0x5C..0x68` | test `kv_rules_list_order_is_the_exe_slot_index` |
| 54 | Battle-file `unit_experience`: the consumer, and whether it matters | §58 (1)–(3) | **SETTLED**: the three formula readers were already found (§52 (4), §19a) and are ported — the fatigue term `0x00670F40` and both morale timers `0x0053E4D0`/`0x0053A720`; the rest of the twelve `+0xD48` readers are the unit-card panel's `"Experience"`, the campaign script getter table, and unattributed getters. **CONFIRMED the field is live data**: 25/32 shipped battle files, 1267/1350 units, levels exactly 0..9 (the ten chevron rows). **UNKNOWN is now the *writer***: the parser's only card call `0x0050EB77 → 0x00513440` is a plain copy whose one byte store is `card+0x110` = `unit+0x12C`, not `unit+0xD48`, so a debugger write watchpoint is needed | `LandUnit::experience`, tests `battle_file_experience_coverage`, `battle_file_experience_reaches_every_unit`, `the_experience_level_moves_fatigue_and_the_morale_timers` |
| 55 | The Austerlitz window closing about a minute in | §58 (4) | **The recorded root cause is REFUTED with data**: the battle file's `duration` is **2400 s**, not 60 (1800–3000 s over all 32 files), so its "pause the clock during cutscenes" / "ignore the timeout in a cutscene" fixes would be regressions. **The symptom does not reproduce**: `--battle-key NHB_Austerlitz` ran a full battle to **712.1 s** and the window stayed open; `--battle --screenshot` exits 0 *with* its picture. New harness fact (INFERRED cause of the old runs): a bare `--battle-key` never leaves Deployment, so any exit those runs saw was a harness exit | nothing to change; `victory::check` left exactly as §9 has it |

## 1. Shooting

### 1.1 Reload time, `0x00639E40` (CONFIRMED)
Called per soldier when he fires (`0x006FB740`, soldier entity vtable `0x0133AF38` slot 19 → `0x006F4660`), and by two
artillery/ship crew paths (`0x00805DFD`, `0x008192DA`). The result is stored as an `f32` at `entity+0x730`.
```
s = unit.reload_skill (+0x164, unit_stats_land col 27)
if unit+0xC28 == 4: s -= 3 ; elif == 5: s -= 7            // shaken / wavering (INFERRED, see #5)
if unit state 7 (FUN_0055C9A0(7)): s += fire_and_advance_reload_modifier
if entity+0x388 (formation/drill object):
    if on walls (FUN_0056AE70): s += fire_on_walls_reload_modifier
    else switch drill (FUN_005498B0): 1 mass_fire, 2 platoon_fire, 3 improved_platoon_fire, 5 rank_fire
         (firing_drill_*_reload_modifier); 4 and others: nothing
if entity+0x10 && entity.vfunc+0x2C() && !object(+0x60→+0x14).flag(+0x231): s += unit.reload_skill   // in a building, not walls
army = unit+0x1EC: flag army+0x224, level army+0x234:
    flag clear: level 1 → +15, level 2 → +30 ; flag set: level 1 → +15
s -= {tired 3, very tired 5, exhausted 10}[entity fatigue level (vfunc +0xE0)]
if weather type 0 or 1 (rain, snow): s -= 6
s = max(s, 0)
t = max((150 - s) * 0.01, 0.0) * (float)weapon.reload_time (+0x98, projectiles col 24)
return max(t, 1.0)                                               // seconds, f32
```
The ship/crew branch (no unit) uses `object+1000` as the skill, +25/+50 (+30 with the flag) army bonuses and
`object+0x1EFC` 3 → −3, 4 → −7.
How the soldier's timer counts down is not decompiled; the model converts to ticks with `ceil(t * 10)` (INFERRED).

## 2. Morale (land)

### 2.1 Timers (CONFIRMED; corrected 2026-10-04: the byte is experience, not a position index)
- Waver timer on entering Wavering, `0x0053E4D0`: `waver_base_timeout + experience * 5` (experience = byte `unit+0xD48`; decompiled `0x0053E4D0`: `(uint)*(byte *)(unit+0xD48)*5 + base`; the byte is exposed as "Experience" by `0x005ABF40`/`0x005CD340`).
- Rout timer on starting to rout, `0x0053A720`: `max(0, broken_finish_base_timeout - experience * 20)` (decompiled `0x0053A720`: `base + exp*-0x14`, floored at 0).
- Sandbox 0-A ports this (`LandUnit::experience`, `morale::waver_timeout`/`rout_timeout` take `experience`; battle-file `unit_experience` and custom-army xp wired, 0 elsewhere). The old "unit index ... staggers ... late units" note was the misreading.

### 2.2 Light update `0x00585BE0` (CONFIRMED) — also called first thing by the full evaluation `0x00584020`
```
if skip flags (+0x55/+0x56/+0x57): return
if state == 5 and waver_timer >= 0: waver_timer -= 1
elif state == 6 and rout_timer >= 0: rout_timer -= 1
if surprise_timer >= 0: surprise_timer -= 1
if FUN_0055AC20() (current order object's vfunc +0x20; INFERRED "charging") and charge_timer <= 0:
     charge_timer = kv charge_timeout
elif charge_timer >= 0: charge_timer -= 1
for each transient effect (id, value, f32 seconds left): seconds -= 0.1; remove it when <= 0
```
So every timer runs every tick (the full evaluation runs the light update first), not only on the 4 off ticks.

### 2.3 Active effect list, `add_effect 0x0054E3C0` / clear `0x0054E4C0` (CONFIRMED)
- The list is cleared at the start of each full evaluation (`0x0054E4C0`, also zeroes component `+0x30`).
- No de-duplication: the same id can appear several times.
- At most 43 entries: when the list already holds more than 42, new effects are dropped.
- Sorted insert by key descending, key = `v` for `v >= 0`, `|2v|` for `v < 0`; a new entry goes before the first entry
  with a smaller key (after equal keys). The morale value adds **all** entries (order does not change the sum).
- The full evaluation clears the bytes `+0x51` (suppress) and `+0x52` at its start (16-bit store).

### 2.4 Routing units and rally (in `0x00584020`, test `0x0055C500`, rally `0x00572AC0`; CONFIRMED)
While behaviour == 2 (routing):
- `can_rally` = behaviour 2 && skip flag `+0x56` clear && times_routed < 3 && unit not in the "4/5" order state
  (`FUN_0055ADC0`) && unit `+0xAA0 != 2` && state != 7, and
  - times_routed == 1: men (`+0x204`) >= `unit+0x30 / 4`; times_routed == 2: men >= `unit+0x30 / 2`;
  - not `FUN_0054EE90()` (any linked unit list member busy; the AI reads it as "engaged");
  - for every enemy unit within 80 m that has not left the field (`FUN_0055B0A0`): enemy strength <
    own strength × 1/3 (× 1/2 when unit flag `+0x189`). Strength = `0x006AFB70` (melee) + `0x006B05B0` (missile).
- not can_rally → only the "state 6 below broken_lower → 7" check runs; no hysteresis, no behaviour change.
- can_rally and rout_timer < 0 → rally (`0x00572AC0`): state 5, behaviour 0, byte `+0x54` = 1, rout_timer = −1,
  waver_timer = waver timeout; then the normal hysteresis runs (a gated Wavering, so no further change).
- can_rally and rout_timer >= 0 → normal hysteresis (Broken is gated by the timer).
- Behaviour after the hysteresis: state 6/7 → mode 4 when `unit+0xC84 != 0 && unit+0xC88 == 0 && unit+0xC80 == 0 &&
  FUN_0055C1C0() && component byte +0x50`; otherwise 3 (state 7) or 2 (state 6). Other states → 0.
- On a state change to < 5: bytes `+0x53`, `+0x54` cleared; to 5: waver timer set.

### 2.5 Sub-evaluators (CONFIRMED unless noted)
- Fatigue `0x0053B7B0`: unit fatigue level `+0xC70` 3 → effect 0x1D `ume_concerned_tired`, 4 → 0x1E `very_tired`,
  5 → 0x1F `exhausted` (key reads at 0x53B945 / 0x53B92E / 0x53B7E9). A zero value adds nothing; formation class
  (`+0x194`) 4 halves it, class 5 adds 0. The first exhausted evaluation also fires a UI event.
- Column formation `0x0053BC20`: in state 0x15 → effect 0x12 `ume_encouraged_column_formation` if non-zero.
- Flanks `0x0053BB40`: if state 0, state 1 or `FUN_0055B140()` → effect 7 `ume_encouraged_flanks_secure`. Otherwise the three
  neighbour slots `+0xD5C/+0xD60/+0xD64` (0 = exposed, 2 = covered): 1 exposed → 0x1B `exposed_single`, ≥2 → 0x1C
  `exposed_multiple`; none exposed and ≥2 covered → 7 `flanks_secure`.
- Fear/inspiration `0x0053B970` (100 m): an enemy with `+0x18C` → 0x2A `unit_frightened`; an enemy with `+0x18B` while we
  are cavalry not in state 0xF → 0x29 `horses_frightened`; a friend with `+0x18D` → 0xF `encouraged_inspired`.
- Fortification `0x0053E450`: in a fortified position → 8 (`ume_encouraged_fortification`) or 9 (`_compromised`).
- Shock persists `0x0053E980`: `!state(1) && !unit flag +0x189`.
- May break `0x00532370`: returns false only under a battle-wide condition (`battle+0x2DB4` object, flag `+0x245`) for
  one army while one of its units is steadier than shaken; true in ordinary battles (INFERRED).
- Army/general `0x0053BC70` and terrain/neighbours `0x0053CBD0`: decompiled, not yet modelled (need generals and
  neighbour strengths).

## 3. Melee direction `0x006AD890` (CONFIRMED)
Inputs: attacker position A, defender position D (x, z), defender facing vector F (table `0x0176CFF8` by u16 angle).
```
v = D - A ; if |v|^2 < 0.010000001: return 0 (front)
v = normalise(v) ; c = clamp(dot(F, v), -1, 1) ; a = acos(c) in degrees
if c >= 0: if a <= 45: return 3 (rear)  ; return atan2(v.x, v.z) < 0 ? 2 : 1
else:      if a >= 135: return 0 (front); return atan2(v.x, v.z) >= 0 ? 2 : 1
```
Front/rear boundaries match the old placeholder (45° / 135°, inclusive). The flank side is taken from the sign of the
world-space x of `v`, not from the facing (as compiled).

## 4. Fatigue `0x00670F40` (soldier entity; CONFIRMED)
- Action → key (byte table `0x006711D8`, jump table `0x006711AC`; action = entity `+0x1B8`, 0..0x4E):
  - idle + `idle_rain`/`idle_snow`: 0–7, 0x23, 0x29, 0x2C–0x33
  - idle (no weather term): 0x1D
  - walking: 8–10, 0x17, 0x18, 0x28, 0x35–0x3E, 0x4C–0x4E
  - running: 0x0B, 0x0C, 0x3F–0x42, 0x44, 0x45, 0x48, 0x49
  - charging: 0x0D, 0x25
  - ready: 0x0E–0x11, 0x24
  - combat: 0x14–0x16, and any action while entity `+0x22C` vfunc `+0x20` is true
  - shooting: 0x19, 0x1A, 0x26 ; reloading: 0x1B, 0x1C, 0x27 ; working: 0x1E–0x22, 0x2B
  - nothing (delta 0): 0x12, 0x13, 0x2A, 0x34, 0x43, 0x46, 0x47, 0x4A, 0x4B
- The soldier code never reads `idle_in_building`, `limbering`, `reloading_artillery`, `walking_artillery`,
  `walking_horse_artillery`, `running_artillery_horse`, `running_cavalry`, `running_cavalry_light`, `tight_formation`,
  `under_fire_artillery`, `under_fire_small_arms` (other entity kinds may; their users are UNKNOWN).
- Slope: when entity `+0x118 == 1` and (`+0x110 & 1` or `+0x117 == 0`), `g = +0x1A0`: `g > 0.2` very steep, `> 0.1` steep,
  `> 0.05` shallow; `delta = (m * delta) / 100` (int).
- Then: `-1` if unit flag `+0x18E`; `+ battle climate +0x2C` unless unit flag `+0x18F`; `+ battle climate +0x30` unless
  unit flag `+0x190`; `+ alliance table[unit index]+0x20`. Without a unit the two climate terms are added unconditionally.
- Unit averages (`0x0057F070`): `+0xC74` = u32 mean of the soldiers' vfunc `+0xE4` (fatigue), `+0xC70` = u32 mean of
  vfunc `+0xE0` (level); inactive units (`+0xAA0 == 0`) zero every soldier's `+0x370/+0x374` and `+0xC70`.

### 4.1 The slope test fields (partly resolved)
- `+0x118` is rewritten every tick by `0x006E0F00` (called from the soldier vtable `0x0133AF38` slot 121, `0x006D7720`):
  when the soldier's position object (`+0x6DC`) byte `+0x24` is clear (or `+0x117` is set and `FUN_006D00D0` fails) it copies
  that object's byte `+0x25`; otherwise it is 1 when every type-0/1 entry of the soldier's list `+0x690/+0x694` reports done
  and `FUN_0062DF10` passes; it is forced to 1 while `+0x700 != 0`. CONFIRMED code; meaning UNKNOWN (something like "settled
  at its formation slot").
- The same predicate (`0x0064CAA0`) is used by the soldier collision test `0x0062F1A0` (returns 4 = ignore the pair).
- The gradient `+0x1A0`: no direct float store to `+0x1A0` of the soldier class was found (all `MOVSS/FSTP/MOVQ/MOVUPS` stores to
  `+0x1A0`/`+0x19C`/`+0x198` in the exe belong to other classes: a matrix in `0x006EAA00`, a position in `0x00603D10`, …).
  It may be written through a pointer. UNKNOWN; the model keeps "height change / distance moved" (PROVISIONAL).

## 5. Unit attribute flags (CONFIRMED offsets and columns; names INFERRED)
`LAND_UNIT_RECORD` constructor `0x00E8CFF0` copies a stat block into `record+0x138` (`0x00E8F1D0`); the battle unit holds the
same block 0x20 higher (`unit+0x158`, W1 §12.9), and the strength functions are called with `unit+0x20` as the record.
Record offset → BUILDER offset (column):
```
+0x138 armour (0x6C, col 11)      +0x13C (0x7C, col 13)       +0x140 accuracy (0x134, 26)  +0x144 reload skill (0x138, 27)
+0x148 firing mechanism enum (0x148)  +0x14C melee weapon type enum (0x170)
+0x150 attack (34) +0x154 charge (35) +0x158 col 36 +0x15C col 37 +0x160 morale (42) +0x164 col 51
+0x168 col 65  +0x169 col 71  +0x16A col 72  +0x16B col 73  +0x16C col 74  +0x16D col 75  +0x16E col 76
+0x16F col 78  +0x170 col 79  +0x171 col 80  +0x174 training level enum (0x1B0)  +0x178/+0x17C = 1.0
+0x180 ammunition (31)  +0x184..+0x190 cols 52..64  +0x194/+0x198/+0x19C cols 66..68
+0x1A0 col 77  +0x1A1 col 81  +0x1A2 col 82  +0x1A3 col 83  +0x1A4 col 84  +0x1A5 col 88
```
Readers, so each flag's role is CONFIRMED (names from the units that set them, `attr_flag_probe`):
- unit `+0x189` = col 71 (108 units: heavy cavalry, guards …): rally needs enemies below ½ instead of ⅓; shock never
  persists; melee strength +50. Named "steadfast" here.
- unit `+0x18A` = col 72 (8 British/Austrian heavy cavalry): may become Impetuous. ("impetuous")
- unit `+0x18B` = col 73 (6 camel units): frightens mounted enemies within 100 m. ("frightens horses")
- unit `+0x18C` = col 74 (Old Guard only): frightens enemies within 100 m. ("frightens enemy")
- unit `+0x18D` = col 75 (80 guard units): inspires friends within 100 m. ("inspires")
- unit `+0x18E` = col 76 (74 lancers/hussars): fatigue −1 per tick. ("good stamina")
- unit `+0x18F` = col 78 (29 Ottoman/Mameluke): no battle climate term `+0x2C`. ("heat resistant", INFERRED)
- unit `+0x190` = col 79 (27 Russian): no battle climate term `+0x30`. ("cold resistant", INFERRED)
- record `+0x188` = col 56 (18 rifles/jäger/guerrillas): melee strength +30. record `+0x18F`/`+0x190` = cols 63/64 (only the
  Austrian Windbüchse jäger): melee +60 / missile +70.
- Other columns, data only: 52 grenadiers (12), 53 chasseurs/camel gunners/irregulars (52), 54 (131, elite and line
  infantry), 55 dragoons (23), 57 foot artillery (39), 58 generals (42), 59 (250), 60 (91 elite), 61 skirmishers (20),
  62 guerrillas (4), 65 = 84 mounted infantry/chasseurs (15), 69 light cavalry (53), 70 militia (20), 77 (132), 80 (89),
  81/82 never set, 83 guards (85), 88 Spanish guerrilla cavalry (23). Their readers are not traced (UNKNOWN roles).

## 6. Strength potentials (CONFIRMED)
`0x006AFB70` (melee) / `0x006B05B0` (missile): 0 if shattered (state 7 or behaviour 3); else
`potential(unit+0x20, card=(unit+0x1C)+0x78, men=(unit+0x1C)+0xC8) * (1 - unit+0xCB4)`, floored at 0. Melee ×10 and missile = 0
when `unit+0xE14 != 0 && unit+0xE10 == 0` (UNKNOWN switch). The missile term is also reduced by the share of crew with
soldier byte `+0x6F8` set (`FUN_0055AB90` gate; INFERRED: abandoned guns).

`0x007578E0` per-man missile value: 0 without the record's own projectile (`+0xE0`, column 30; artillery rows have none);
else `(accuracy*0.7 + proj.range(+0x60)*0.5) / ((100 - reload_skill)*0.01*proj.reload_time(+0x98) + 12)`, +0.4 for class
0xF (`infantry_grenadiers`), ×0.5 for category 0 (cavalry).

`0x00757120` melee potential:
```
class_term = category 0: (class 8 cavalry_missile ? 4 : 8); 3 dragoons: 4; 4 elephants: 20; 5 camels: 8; else 0
P = ((col37 + col36)*0.05 + armour*0.3 + attack*0.06 + charge*0.04 + col51*0.03 + class_term) * men
  + morale * (missile_per_man == 0 ? 10 : 5)
  + 60 [rec+0x18F] + 30 [+0x188] + 50 [+0x169] + 50 [+0x16B] + 70 [+0x16C] + 30 [+0x16E] + 70 [+0x16D]
  + card list (16-byte entries, count +0xC, data +0x10): any id 8 → +20, any id 1 → +50
P = max(P, 0)
```
`0x007575A0` missile potential: 0 if no own projectile and category != 1 (artillery). Base: artillery → the gun type's
(`rec+0xC4`) projectile list `+0x58/+0x5C`: the first whose shot type `(+0x24)+0x10 == 0`, else the first;
`(2*accuracy + 3*damage(+0x78) + 0.3*range(+0x60)) / max((100 - reload)*0.01*reload_time, 0.01) * men`; others →
`men * missile_per_man`. Plus: morale*5 if missile_per_man > 0; 70 [rec+0x190]; card list id 7 → 20; card field [0] in 2..5
→ 70; class 2 `artillery_horse` 250, class 1 `artillery_foot` 150; card list 2 (count [7], data [8], ints): 3 → 110,
10 → 300, 4 → 70, 1/2/5/6 → 50 each. Floored at 0. No ammunition test.
The unit card (`unit+0x1C`) is UNKNOWN (INFERRED: the campaign unit with its upgrades/abilities); its terms are 0 in the model.
(`ntw_ai::battle::rating` mirrors the old PROVISIONAL version of these functions; it should switch to `ntw_sim::battle::strength`.)

### 6.1 Unit-list range tests (CONFIRMED)
`0x00701310` (enemies) and `0x006C9E40` (friends; skips the unit itself by id) keep a unit when the centre distance in the
x/z plane is `< r_self + N + r_other`, `r` = formation object (`unit+0xADC`) `+0x670`. N = 100 m for fear/inspiration, 80 m
for the rally test. The radius writer was not found (UNKNOWN); the model uses `FORMATION_RADIUS = 0` (PROVISIONAL).
The validity test `FUN_0055CBD0` = active state not 0 or 2 and `unit+0x2CC != 0`. `FUN_0055C2A0` (horses frightened) =
category cavalry or camels, or dragoons not in unit state 0xF (INFERRED: dismounted).

### 4.2 Battle climate fatigue terms (INFERRED source)
The fatigue function reads `+0x2C` and `+0x30` of the object at `battle(+0x28)→+8→+0xB0→+0x198` (CONFIRMED reads), plus
`+0x20` of the entry for the unit index in the list `FUN_00E31490()` (`+0xC/+0x10`; UNKNOWN). The data source of the two
climate terms is INFERRED to be `battle_climate_weather_descriptions` (660 rows: key, climate, season, weather, then 7
four-byte ints): column 7 holds 0..3 in hot climates (desert summer dust, jungle summer rain) and column 8 holds 0..3 in
winter rain/snow — and the units exempt from `+0x2C` are Ottoman/Mameluke (col 78) while those exempt from `+0x30` are
Russian (col 79). So `+0x2C` = heat fatigue (col 7), `+0x30` = cold fatigue (col 8). Columns 4 and 5 look like weather
weights (UNKNOWN use). Not traced from the loader to the object (the record/object offset shift is UNKNOWN). The model
carries them as `Battle::climate_fatigue`, 0 until the battle setup chooses climate, season and weather (BATTLE_FLOW).

### 6.2 The AI ratings are these strengths (CONFIRMED) and what the switch changed
The army update `0x00539E80` stores `0x006AFB70` at unit `+0xBE8` and `0x006B05B0` at `+0xBEC` (the two ratings the battle
AI analysers read) and sums both into the army strength `+0x64`. `ntw_ai::battle::rating` now calls
`ntw_sim::battle::strength`. The balance `0x006A31E0` counts routing units (only shattered ones drop out).

Why the AI lost a win in `real_ai_against_the_models_default_behaviour` (France v Austria, 8 seeds):
- The old stand-in ratings mixed up role codes (infantry got class term 8 and cavalry 20, instead of 0 and 8) and used
  the artillery formula for muskets (3–4× too big). Real data, seed 1: line battalion melee 1628 → 398, missile
  1700 → 525; 60 cuirassiers melee 1524 → 909 (class 8, morale ×10 without a musket, steadfast +50).
- With the exe ratings a 60-man cavalry unit is worth more than a 160-man line battalion (target value
  `0.3·missile + 0.7·melee`: 637 v 436–479). The melee priority is `value² × shape(potential) × …` (CONFIRMED), so the
  Austrian hussars now go for the French cuirassiers instead of the nearby French line (seed 1, t ≈ 900; with the old
  numbers they charged the line and won). Austria-AI wins per seed: old ratings 1, 2, 4, 5; exe ratings 3, 5, 7.
  Mixing them (experiment only): old melee + exe missile 4/8; exe melee + old missile 8/8.
- Test change (justified): the strict `wins > austria_none` (now 3 v 3) rested on the buggy numbers. The test now asks
  that the AI is no worse than the default on either side, better over both (11/16 v 8/16) and wins at least 9 of 16.
  How well the hussars' choice works in the original also depends on AI parts still PROVISIONAL (unit-card terms in the
  ratings, class matchup details).

## 7. Ground-type grid (`0x0061E430`, "Ground State Grid"; CONFIRMED)
Built by the battle grid set-up `0x00506C00` (stored at its `+0xA0`). The grid object: 512 × 512 cells (`+0xC` = 0x200,
shift `+0x8` = 9), 4 bytes per cell, cell size `extent / 512` (`+0x20`), origin at `-extent/2`. Each cell's first byte
is copied from the first byte of the matching 4-byte pixel of the battle's ground image (`image+0x34`, stride
`image+4`; INFERRED: `ground_type_map_0.tga`). A 25-entry list (`+0x40/+0x44/+0x48`) holds one
`UNIT_MOVEMENT_MODIFIER_RECORD` per name of the pointer table `0x01452520` (`field_ploughed` … `none`), looked up by name
(error string "is not a valid key"). So the cell byte → ground type name mapping is the name-table position (CONFIRMED;
it was INFERRED before). Which of the record's four floats a unit uses: the reader was not found (searched the small
`+0x48` readers and the grid's neighbours); `MovementClass` stays PROVISIONAL.

## 8. Frame-rate drops seen in battle runs (round 2, item 2)
Runs of `NAPOLEON_FPS_LOG=70 NAPOLEON_AI_SPEED=8 cargo run -p napoleon -- --battle` (2026-10-04). The battle is
deterministic (same men counts at the same moments in every run of one build), so differences between runs of the same
build come from the machine, not the game:
- origin/main, 2 runs: 56–221 FPS (the 60 FPS stretches are the battle-over/results phase).
- this branch, 2 runs made while Ghidra headless jobs were running on the same machine: 8–35 FPS from real time ≈ 38 s
  onwards, in the same simulated state that a third run gave 60–195 FPS. The third run (no Ghidra running) had no drop:
  battle over at 406.9 s battle time (real ≈ 51 s), then 60–142 FPS on the results phase.
- The first report's drop (10–20 FPS at real 44–54 s) was also a run made alongside other work on the machine.
Conclusion: the drops are CPU contention from other processes (other workers' builds, Ghidra), not this branch. The sim
and AI cost per tick is small (the 48 headless real-data battles of `ntw_ai --test real_battle` take 0.2 s). For clean
numbers, measure with nothing else running.

## 9. Battle file and battle clock (BATTLE_FLOW questions; CONFIRMED unless noted)
- Battle-file parser: `battle_description` in `0x00511210`; alliances in `0x0050AFE0`; armies via `0x0050CAE0(…, elem,
  reinforcement)`.
- `duration`: parsed as `f32` into the battle setup `+0x84` (default −1.0 = no limit); `timeout_winning_alliance_index`
  into `+0x88` (default −1), with a flag `+0x94` = 1 when present.
- Battle clock: the battle object's `+0x24` gets `+0.1` per model tick (constant `0x0131A7B0`) only while the battle state
  `+0x14` is 2..5 (`FUN_0055AC60`), i.e. not during deployment; `+0x1C` gets +100 per tick (milliseconds).
- Time-out (`0x00582FB0`, state 2): `if 0 <= duration && duration < clock` → winner = `timeout_winning_alliance_index`,
  battle-over flag `+0x21` = 1, result 3. It runs after the per-alliance test (`FUN_00550240`) and overrides its winner in
  the same tick. So `duration` is seconds of battle time, counted from the end of deployment, strict `<`. Code:
  `victory::check` (now strict and checked first).
- State 5 (`0x00582FB0`): the battle ends (result 6) when every alliance but the winner has no units in its armies'
  lists `+0xF0` and `+0x108` (INFERRED: units on the field and units still to arrive).
- `non_playable` is read at two levels (CONFIRMED): as a child of the **alliance** (`0x0050AFE0`, clears alliance `+0x38`)
  and as a child of an **army** (`0x0050CAE0`, clears army `+0xCD`). XML elements are name → children maps
  (`0x005560A0`), so an alliance-level element counts wherever it stands. Friedland has one between its two French
  armies and one in the Russian alliance, so both alliances are flagged; how the front end then picks the player is
  UNKNOWN (ours: first unflagged alliance, else fall back; `BattleSpec::player_army`, PROVISIONAL). `rout_position` reads
  two floats into alliance `+0x28`/`+0x24`.
- `reinforcement_army approach_angle` is in degrees (× 0.017453292 to radians, flag set when present). When reinforcements
  arrive: not found yet (UNKNOWN).

## 10. Army/general and neighbour sub-evaluators (CONFIRMED; code `morale::sub_army_and_general`, `sub_terrain`)
kv offsets in these functions are `kv_morale` index × 0x60 + 0x10 (checked against the fatigue, flank and fear keys), so
0x11B0 = `ume_concerned_army_destruction`, 0x1210 = `general_dead`, 0x1270 = `general_fled_recently`, 0x12D0 =
`general_died_recently`, 0x1330 = `ume_encouraged_on_the_hill`.

`0x0053BC70` (army = unit `+0x1EC`):
- army `+0x1B0 == 0`: `+0x1B4 == 0` or unit steadfast (`+0x189`) → 0x15 `general_dead`; else 0x14 `general_died_recently`.
- else `+0x1B8 != 0` → 0x16 `general_fled_recently`; else the general is present: `rank` = general unit (`+0x214`) card
  `+0xBC`, `d` = distance to him (0 when he is this unit or there is no general unit). `d <= 225` → effect 2 =
  `trunc(f32(w(d) · ((rank+1)/2 + 5)))`, `w` = 1 below 75 m (`FUN_00555F20` = 75), linear to 0 at 225
  (`0x00536D20` = clamp-then-line); always effect 4 = `rank/2 + 1`.
- `FUN_0054C840` = units in the alliance's armies after the first → effect 1 = `min(n/4 + 1, 6)` when n > 0.
- `0x00532000` army destruction → 0x13: own alliance strength ratio (`+0x64 / +0x60`) ≤ 0.1 and the other alliances'
  summed ratio ≥ 7 × ours. The start strength `+0x60` is stored once by `0x00539E80` (model: first step).
- The model has no generals (`GeneralStatus::NotModelled` adds nothing, PROVISIONAL) and one army per side.

`0x0053CBD0` (lists of units within 160 m, weight `w = trunc(f32(clamp_line(d, −0.01, 2.0, 48, 160)))` = 1 up to
100 m, else 0):
- friends: routing (behaviour 2/3, `FUN_0055C780`) and inside a ±100 m × ±75 m box in our frame (`FUN_0055C7B0`) →
  r += 2; non-routing → men_f += men·w, str_f += (int)strength·w.
- enemies: any enemy at least as high as us − 5 m cancels "on the hill"; routing → r −= 3 if we are Impetuous, else 2;
  others (only when `FUN_0055B100` matches for both, INFERRED fortification state) with w halved when we ride and are
  faster (`FUN_00565150(1)`, INFERRED speed) → men_e, str_e.
- r ≥ 2 → 0x26 = −2·min(r/2, 4); r < −1 → 0x0C = −2·max(r/2, −4).
- men_f += own men × 2, own = (int)(own strength + str_f); then k = min(men_e/men_f, str_e/own) → 0x27 = −7 (k ≥ 6),
  −5, −3, −2, −1 (k = 5..2).
- "on the hill" (at least one enemy, all ≥ 5 m lower) → 0x0B; str_e·3 ≤ own → 0x0D = +4 (also with no enemy near).
- Truncations: the x87 results are stored as f32 before `CVTTSS2SI`, which matters (e.g. rank 3 at 50 m: the x87 weight is
  0.99999999, the stored product 7.0 → 7).
- Model: distances are centre distances and the formation radius is 0 (PROVISIONAL), heights from `BattleGround`.
- Effect on the AI test: with these effects the Austrian AI wins 4/8 against the default behaviour (default 2/8), France
  8/8 (default 6/8).

## Handed over to 0-D (work/fidelity-units), 2026-10-04
Scope change: units/animation/terrain/trees questions (ANIM_FORMAT.md, CAVALRY.md, BATTLE_TERRAIN.md §8–10, SPEEDTREE.md
and their render/format code) belong to 0-D. What I found or started there:
- Ground-type grid `0x0061E430` (§7): 512 × 512 cells, the cell byte indexes the 25-name table `0x01452520` (the TGA index
  → name mapping is CONFIRMED). The grid object is built by `0x00506C00` (battle grid set-up, also the "Soft Collision
  Grid" next to it); the image it copies from is `image+0x34` with stride `image+4` (INFERRED `ground_type_map_0.tga`).
- Heightfield scale (/65535 v /65536): a quick search for the constants 65535.0 (`0x477FFF00`), 1/65535, 65536.0 and
  1/65536 found no terrain loader (the hits are an angle-wrap helper `0x00E23470`, a mode switch `0x00DB2350` and two
  unrelated renderers). Not resolved; the loader may use an integer shift or a shader.
- Not started: ground sampled per man or per unit, default deployment layout, tree list U8 / LOD, billboard picture
  order, the terrain shader.
Stays with me (battle sim): the `unit_movement_modifiers` column per unit (reader not found yet), the slope test fields and
the gradient `+0x1A0` writer (§4.1), ground and slope effects on movement and combat in `ntw_sim`.

## 11. Generals in the battle model (round 3, item 1)
CONFIRMED:
- Battle-file `general` element (`0x0050CAE0`): attribute `general_category` (`normal` 0, `invincible` 1, `napoleon` 2,
  `0x0057C870`), children `star_rating level` (int), `name`, `portrait`. **`experience` is not read.** `0x00513A10` stores
  (star rating, has-general flag, category) at +0x20/+0x24/+0x28; the unit card builder `0x00513440` copies them to card
  `+0xBC` (rank), `+0xC0` (general flag) and `+0xC4` (category).
- Army constructor `0x00505B00`: army `+0x1B0` = 1, `+0x1B4` = `+0x1B8` = 0, `+0x214` = 0; then for every unit card in order,
  a card with `+0xC0` in {1, 2, 4, 5} makes that unit the army general (`+0x214`), so the last one wins.
- `FUN_0055AC40` (unit is its army's active general) = `army+0x214 == unit && army+0x1B0`.
- Army update `0x00580390` decrements `+0x1B4` and `+0x1B8` by 1 per tick while non-zero (countdowns).
- So an army **without** a general unit takes the "present" branch of `0x0053BC70` with distance 0 and rank 0: every unit
  gets effect 2 = 5 and effect 4 = 1.
Not found (searched all byte/dword stores, LEAs and SETcc-free forms to `+0x1B0/+0x1B4/+0x1B8`): who clears `+0x1B0` and
who starts the two countdowns on the general's death or flight, and their lengths. Model (PROVISIONAL): general unit
destroyed → "died recently" for `GENERAL_DIED_RECENTLY_TICKS` = 600 (60 s), then "dead"; general unit routing or
shattered → "fled recently".
Code: `LandUnit::general_rank` / `army_index`, `Battle::general_of`, `general_status`, `allied_units`, `general_died`;
`setup::apply_spec_unit` (rank = `star_rating level`, army index per battle-file army). The test armies have no general
unit (so the CONFIRMED +5/+1 applies). Campaign battles (startpos characters) are not wired to the battle yet: when they
are, the card's rank should come from the commander's character record (UNKNOWN field).
AI test after this: Austria AI 5/8 v default 3/8, France 8/8 v 5/8.

## 12. "The battle stays in deployment" in `NAPOLEON_AI_SHOT` runs (bug report, 2026-10-04)
- Nothing in the game ends deployment by itself: the only path is `BattleUiRequest::DeploymentFinished`, which the HUD
  scripts send through `BattleUI.InformOfDeploymentFinished` / `DeploymentFinishYesStart` (the deployment panel's Start
  Battle button, or its RETURN-key confirm). The `NAPOLEON_AI_SHOT` harness never clicked it.
- No culprit commit: the same build behaves both ways. At 12ced63, 1 of 4 runs started the battle (7 s after the window
  opened, the first run after a rebuild); at 01c5155, 1 of 5. Six runs with every HUD mouse/key event logged never
  started it and logged no event. The successful runs most likely picked up a live click or key on the window (INFERRED:
  the harness read the live mouse, and the game window pops up over the desktop). `git bisect` cannot work on that.
- Fix (`napoleon::battle::hud`): an `NAPOLEON_AI_SHOT` run without `--skip-deployment` now waits a second and clicks
  `button_battle_start` through the HUD like a player (`harness_clicks`), and such runs ignore the live mouse. Tests:
  `napoleon` `harness_tests::ai_shot_runs_click_start_battle`; `ntw_script --test battle_deployment` (original scripts:
  30 s of HUD time without a click end nothing; a click on Start Battle sends `DeploymentFinished`). Checked by hand:
  `--battle-ui-click wait,wait,button_battle_start` and the plain AI-shot run both reach the battle and the screenshot.

## 13. Reinforcements (round 3, item 2)
CONFIRMED:
- Battle file: `reinforcement_army` armies are parsed by the army parser with the reinforcement flag (`0x005738E0` →
  `0x0050CAE0(…, 1)`; army setup `+0x116` → army `+0x222`); `approach_angle` is in degrees; a unit's
  `manually_deployed` lands in its card `+0x110` and the battle unit's `+0x1E5` (`0x0051B862`).
- Arrival is automatic, `0x00608200`, run on every battle update from `0x00603A40` (the first call of the battle update
  `0x00583480`): for each entry group, while it holds fewer than 20 units, if none of its arriving units is still inactive
  (`+0xAA0 == 0` with soldiers), the next unit of the reinforcement army (in order) arrives — skipping units already on
  their way, units of class 0 (`artillery_fixed`) and units with `+0x1E5` set (held). An "arrived" UI message (id 0x21)
  is posted for the first one.
- The battle script releases held units: Lua `unit:deploy_reinforcement(b)` (binding `0x006172E0`, name string at
  `0x01333118`) queues `BCQ_UNIT_ORDER_SET_MANUAL_DEPLOYMENT` with `!b`; its handler `0x005C2AA0` stores it in unit
  `+0x1E5`. So `deploy_reinforcement(true)` = may arrive now. Waterloo's script does this (all 9 Prussian units are
  `manually_deployed`), first after 20 minutes or when the British are down to 70 % of their men.
INFERRED/PROVISIONAL (model): entry point = the playable-area edge in the approach direction (0° = +y), facing the
centre; a unit walks on for `REINFORCEMENT_WALK_ON_TICKS` = 100 (10 s) and then becomes active (the exe activates it when
it has entered the map); held units count as out of the fight because our battles do not run the battle scripts yet;
waiting and arriving ones count as still in the fight (like the exe's army list `+0x108`).
Code: `Reinforcement`, `Battle::reinforcements_step`, `deploy_reinforcement`, `reinforcement_entries`;
`setup::historical_armies` places them; the view hides units still off the field.

## 14. Morale component start values (round 3, item 3; CONFIRMED)
The unit constructor `0x0051B7D0` builds the morale component at unit `+0xC00` with `0x0051D0F0`: state `[0xA]` = 2
(**Confident**, not Steady as our placeholder had), behaviour 0, morale 0, the four timers `[0x17..0x1A]` (surprise, rout,
waver, charge) = −1, effect list empty, flag bytes `+0x50..+0x5A` (suppress, rally, skip flags, rout count) 0, `[0x1B]` =
`FUN_00E203C0()` (UNKNOWN). Persistent bonus `[0xD]` from the army setup (`army+0x1EC → +0xD4`): level `+0x88`, flag
`+0x78` (the army's `+0x234` / `+0x224`, the same pair as the reload and melee-attack army bonuses): flag clear: level 1 →
+2, 2 → +4, −1 → −1; flag set: level 1 → +3, −2 → −1; otherwise 0. Code: `MoraleComponent::default` (Confident, timers
−1) and `MoraleComponent::for_army_level`. The model's armies have level 0 (no difficulty setting yet).

## 15. Round 3 status, checks and what is still open
- Battle runs for the report (`NAPOLEON_FPS_LOG=70 NAPOLEON_AI_SPEED=8 --battle`, default armies, battle over at about 455 s
  battle time): two runs of the same build; other workers' cargo/rustc builds were running both times (6–10 cargo, up to
  38 rustc processes). Run 1: 76–129 FPS until real 52 s, then 4–27 FPS for 10 s (while other builds ran), then 70–106.
  Run 2: 52–122 FPS, one dip to 22 FPS in the last 2 s, when 38 rustc processes had started. The game never froze or
  panicked; the dips do not repeat in the same game state, so they come from the machine (as in §8).
- Unit-size option: the setting is `unit_scale`, saved next to `Turns`, `campaign_difficulty` and `battle_difficulty` in the
  game-settings block (`0x0044C9C5` load, `0x00471B44` save). Which code multiplies the men by it was not found yet (SOLVED
  later, §18a); the battle file's `num_soldiers` is parsed as an int (`0x0050CAE0`) and stored on the unit card unchanged
  (CONFIRMED unscaled, §18a). INFERRED: `battle_difficulty` is where the army level (§14, reload/melee/morale bonuses) comes from.
- Still open: who clears army `+0x1B0` and starts `+0x1B4/+0x1B8` (general death/flight), the reinforcement entry point,
  the unit-size multiplier, the `unit_movement_modifiers` column reader, the soldier gradient `+0x1A0` writer, the
  formation radius `+0x670`, and the unit-card terms in strength.

## 16. Battle scripts in live battles (round 4, item 1)
- The engine's Lua bindings: (name, function) pairs in the exe's tables around `0x01450E00..0x01453000` (dumped with the
  `bind:` command of `FidDecomp.java`): unit (`name type position bearing is_moving unit_in_range initial_number_of_men
  number_of_men_alive is_leaving_battle is_routing missile_range ammo_left starting_ammo is_cavalry is_infantry
  is_artillery unit_distance is_limbered_artillery is_currently_garrisoned can_perform_special_ability
  current_special_ability deploy_reinforcement`), army (`units ships create_unit_controller create_ship_controller
  is_commander_alive get_reinforcement_units get_reinforcement_ships`), battle (`out alliances buildings camera weather
  subtitles register_*_handler register_singleshot_timer register_repeating_timer unregister_timer show_advisor_message
  … game_time modify_battle_speed … end_battle_to_frontend`), unit controller (`add_group add_units take_control
  release_control clear_all halt withdraw … fire_at_will kill attack_unit … goto_location goto_location_angle_width
  morale_behavior_* set_invincible select_deployable_object`), vector (`get_x … set distance to_screen_position length
  length_xz`), building, camera, sound (`load play3D is_playing stop`), event (`get_name get_bool1 …`). Names CONFIRMED;
  argument meanings INFERRED from the scripts. `battle_manager`, `cutscene`, `convex_area` and `rout_manager` are Lua in
  the game's `data/scripting_library.lua`.
- Ours: `ntw_script::battle_script` (Rust primitives `__nb`: snapshot, timers, handlers, requests, log) and
  `battle_script_prelude.lua` (our classes on top). Unknown methods are logging stubs (UNKNOWN). Timers run on battle
  time (INFERRED), due-time then registration order. Engine position = (x, height, map y): INFERRED (Waterloo's
  Planchenoit point lies on the French side).
- The game (`napoleon::battle::scripts`) loads `<battle>.battle_script` next to the battle file, sends "Deployment" /
  "Deployed" phase events and the "Battle Results" command, fires due timers every model tick and applies the requests:
  `take_control` / `release_control` (`LandUnit::script_controlled`; the battle AI leaves such units alone — one line in
  `ntw_ai`), `halt`, `fire_at_will`, `attack_unit`, `goto_location(_angle_width)` (angle/width not used yet), and
  `deploy_reinforcement`. Other orders are logged (PROVISIONAL). Buildings are an empty list (PROVISIONAL).
- Waterloo: the script runs without errors (test `ntw_script --test battle_script`: with nobody hurt the Prussians are
  deployed at 1,264 s = the 63 s intro cutscene + the 40th 30-second check). In a live run at speed 8 the British fall
  below 70 % of their men and the script deploys the first six Prussian units at 154 s battle time; a screenshot at
  185 s (camera on the eastern entry, `NAPOLEON_BATTLE_CAMERA=600,-150,450,-1.3,0.5`) shows them arriving (they enter
  stacked at one point — the entry point is PROVISIONAL).
- `NAPOLEON_BATTLE_SCRIPTS=off` turns the scripts off.

## 17. The test battle's generals (round 4, item 2)
The `--battle` test armies (France v Austria, 1805) now each have a general unit, defined the way Austerlitz_Battle.xml
defines them: France `Gen_Late_Napoleon` with `star_rating level 5`, Austria `Gen_Generals_Staff` with level 6 (both
`general_category napoleon`). They are the last general slot of each army, so they are the army generals (§11) and the
units no longer get the "no general unit" bonus. Screenshot check: "Napoleon Bonaparte, 24 men" stands at the right of
the French line.

## 18. General death/flight and the unit-size option (round 4, items 3–4)
- Army `+0x1B0` is CONFIRMED to be "commander alive": the script binding `army:is_commander_alive()` (`0x00612780`)
  returns that byte, and `army:kill_commander()` (`0x00611BE0`) refuses with "error: army does not have a commanding
  unit" when it is 0, else kills the commanding figure among the general unit's soldiers (the one whose soldier vfunc
  `+0x1B0` returns 0) through a queued battle command. No direct store that clears army `+0x1B0` or starts the
  `+0x1B4`/`+0x1B8` countdowns was found (byte/dword/qword/xmm stores, LEAs, the 0x45xxxx–0xDxxxxx ranges): INFERRED,
  it happens in the handler of the commander soldier's death, reached through the command/message queue. The model keeps
  the PROVISIONAL rule of §11 (general unit destroyed → died recently for 60 s, then dead; routing → fled).
- Unit size: the option is a float preference (id 0x6D) holding the scale itself; the four steps are the table
  `0x01392770` = {0.25, 0.5, 0.75, 1.0} (CONFIRMED: `0x00DAFBB0` index → scale, `0x00DAFBC0` scale → index, small /
  medium / large / ultra INFERRED). At battle start (`0x00485B90`) the scale goes into the battle settings object (`+0`,
  setter `0x00878060`) together with 20 (`0x00851550`) and a per-step 6 / 8 / 10 / 20 (`0x00DACE60`). SOLVED in
  sandbox 0-A round N+1, see §18a.

## 18a. The unit-size option, end to end (sandbox 0-A round N+1; CONFIRMED, ported)
> **Main:** `ntw_sim::battle::unit_scale` and the `Battle` methods are ported; the `crates/napoleon`
> setup wiring described below is NOT (see "Where I am", main port).

The step table and the two converters were already known (§18). What was missing is **where the scale is used**,
and it is the campaign army → battle unit creator `0x004A6600` (`0x004B61A0` → `0x004AA900` call it):

```
0x004a66bc  CALL 0x004a6c90                ; entry+0x10: the unit's list of 0x20-byte unit cards
0x004a66c9  CALL 0x004a6540                ; clamp(step(index), 0.1, 1.0)
0x004a66d2  FSTP float ptr [ESP + 0x2c]     ; the scale
   ... per card ([EBP] = card, cards are 0x20 bytes apart, `ADD ECX,0x20`):
0x004a67c5  MOVZX EAX,word ptr [EBP + 0xa] ; the card's men (u16)
0x004a67c9  PUSH 0x131005f                  ; <- this push is why the operand below is [ESP+0x2c]
0x004a67de  MOVD XMM0,EAX
0x004a67e2  CVTDQ2PS XMM0,XMM0
0x004a67e5  MULSS XMM0,dword ptr [ESP + 0x30]   ; * scale
0x004a67eb  CVTTSS2SI ESI,XMM0                  ; (int), TRUNCATED
0x004a6820  PUSH ESI                           ; -> FUN_005363C0 (and again at 0x004A6831)
0x004a6840  CALL 0x005363c0
```
`0x005363C0` builds the 0x114-byte battle unit card and calls the card constructor `0x00513320`, whose
`in_ECX[0x32] = param_4` and `in_ECX[0x33] = param_11` are that count: **card `+0xC8` and `+0xCC` are the
men** (CONFIRMED, and the same pair §6 reads as `men = (unit+0x1C)+0xC8`).
- **Naval** cards (`card+0x00 == 0`) take a **byte** at `card+0x0E` instead (`0x004A6A06`), pushed unscaled —
  no MULSS on that path. So ships are not thinned by this option (CONFIRMED).
- The battle-file path is different and stays unscaled: `0x0050CAE0` hands `num_soldiers` to the card builder
  `0x00513440`, which writes both `+0xC8` and `+0xCC` from the same value with no multiplication (§39).
- Where the index comes from (CONFIRMED code): `0x004A6540` reads the battle-settings entry with key `0x0B`
  (`FUN_004A2B80(0x0B)` = a byte-keyed map lookup, `0x0049F940` = the settings object), except in battle
  modes 2 and 4, where it takes the **smallest** per-unit size-class byte (`*army_unit`, the first byte of the
  0x60-byte army unit entry) over the army's units whose flag byte `+0x5D` has `&4`, `!(… &8)` and `&2`
  (0xFF = none). INFERRED: that map key is fed by the `gfx_unit_scale` preference — it is the only unit-size
  setting with these four steps, and the Lua binding `UnitScaleFactor()` (`0x004795F0`) returns the same
  table for "the current setting". UNKNOWN: which preference/mechanism fills map key `0x0B`, and what sets
  the per-unit size class.
- **The preference itself (CONFIRMED)**: `0x00404230` registers `gfx_unit_scale` through `0x00454300` as an
  **environment variable** preference — int, storage `0x0149D880`, default **2**, help text
  `"gfx_unit_scale <int>"` / `"Set unit scale. 0 - lowest, 3 - ultra"`, registered into the global array
  `0x014A06D8 + id*4`. The player's own `preferences.script.txt` has `gfx_unit_scale 2`.
  So the exe's **default battle has 0.75 × the men** (index 2), not 1.0.
- **Port** (`ntw_sim::battle::unit_scale`): `STEPS = [0.25, 0.5, 0.75, 1.0]`, `step(i)`, `clamp(s)` (0.1 … 1.0),
  `scaled_men(card_men, scale) = trunc(card_men * scale)`; `Battle::unit_scale` (default 1.0),
  `Battle::men_at_scale`, `Battle::set_unit_scale_step`; the setup reads `gfx_unit_scale` into
  `SetupData::unit_scale` and `make_unit` builds the unit (and its drawn formation) from the scaled count.
  Round N+2 added `PREFERENCE_DEFAULT = 2` — the exe's own default for `gfx_unit_scale` (CONFIRMED,
  `0x00404230`), i.e. **0.75 ×** — so the head-to-head comparison maths has one named constant.
  Tests: `unit_scale::steps_and_clamp`, `unit_scale::men_are_truncated` (160 → 40/80/120/160, 159 → 79 at
  0.5, i.e. truncation), `unit_scale::the_four_settings_scale_the_men` (all four settings),
  `unit_scale::default_setting_is_three_quarters` (7 × 0.75 = 5, 158 × 0.75 = 118 — the truncation of
  `CVTTSS2SI` at `0x004A67EB` made explicit), `model::unit_size_option_thins_the_men` (which now also
  goes through `PREFERENCE_DEFAULT`).
- **Deliberate deviation**: the model keeps **1.0** when there is no `gfx_unit_scale` preference, while the
  exe's own default is index 2 (0.75). Changing the default would shrink every unit by a quarter and move
  every tuned expectation (AI balance, battle lengths); the option is honoured as soon as the preference says
  so. Recorded as the one open fidelity item in §53.
- `crates/napoleon` note: `SetupData::unit_scale`, `unit_scale_setting()` and `make_unit` live there and were
  NOT compiled by this round (the sandbox rule forbids building `-p napoleon`). The ntw_sim/ntw_data halves are
  covered by tests.

## 19. Unit-card terms in the strength potentials (round 4, item 5; CONFIRMED)
The card block the potentials read at card `+0x78` is the battle file's `unit_capabilities` as parsed by `0x0050CAE0`:
`[0]` firing drill (`firing_drill`, enum `0x0057C8C0`: fire_volley 0, mass_fire 1, platoon_fire_dispersed 2,
platoon_fire_grouped 3, platoon_fire_column 4, rank_fire 5), a list of 16-byte special-ability entries (`special_ability`
with `active_time`, `recharge_time`, `num_uses`; enum `0x0057C950`, table `0x0131C3A0`: square_formation 1,
fire_and_advance 7, plug_bayonets 8, rally 0x11, …) and a list of shot types (`shot_type`, enum `0x00F59030`, table
`0x0145C1B8`: canister 3, shrapnel 4, rocket 10, …). So the formerly UNKNOWN card terms are:
- melee: plug_bayonets +20, square_formation +50;
- missile: fire_and_advance +20, firing drill 2..5 (the platoon and rank fires) +70, shot types canister +110, rocket +300,
  shrapnel +70, explosive shell / percussive shell / carcass / quicklime +50 each.
Code: `attributes::UnitCapabilities` (from the battle file's names), `strength::card_melee_terms` / `card_missile_terms`,
set in `setup::apply_spec_unit`. Units of the test armies and campaign battles have no card block yet (their abilities
come from campaign technologies/DB, not traced) — PROVISIONAL 0 there. The drill enum also explains the reload drill
switch of §1.1 (2 = dispersed "platoon fire", 3 = grouped "improved platoon fire").
Still open in item 5: the `unit_movement_modifiers` column reader, the soldier gradient `+0x1A0` writer and the formation
radius `+0x670` (no new leads this round).

## 19a. The experience table's flat + multiplier columns are the XP-adjusted COST (sandbox 0-A round N+1)
Commit `2054325` CONFIRMED that `0x00ED49A0` returns `row+0x24 + ROUND(base * row+0x28)` off
`unit_stats_land_experience_bonuses` but left "which stat?" open. Its three callers answer it, and the answer is
**not a battle stat** — it is the campaign's experience-adjusted cost:

- `0x005CD340` (the unit info panel) calls it right between the labels it writes:
  `"Experience"` = `unit+0xD48`, then `FUN_00ed49a0(...)` labelled **`"XpAdjustedCost"`**, then
  `"RecruitCost"` = `card+0x38` and `"UpkeepCost"` = `card+0x3C`. The panel's `param_3` is `1`, so `base` is
  `this+0x30`.
- `0x0045D170` is a 22-byte wrapper (`FUN_00ed49a0(param_1, param_3, param_4)`) whose callers add the result to
  a running total; `0x0045CB50` (called from `0x004765F0`, the army auto-build screen) compares it against a
  budget from `FUN_004A2910` and refuses a purchase when the total would pass it, and it looks at each unit
  type's experience (`< 9`) before buying. So it is what the campaign **pays** for a veteran unit.
- `0x004C2770` sums the same call over a list of 0x114-byte unit cards (`+0xEC`) and 0xFC-byte ones (`+0x68`) —
  the army's total XP-adjusted cost.

So: `this+0x2C` / `this+0x30` are the two cost fields (recruit / upkeep, chosen by `param_3`) and the pair is
"how much more a veteran costs": rank 0 = ×1.0 + 0 (unchanged), rank 9 (land) = +360 and ×1.9, i.e. a 100-cost
recruit becomes 550.
`param_3` selection re-confirmed on the decompilation itself: `0x00ED49A0` starts
`if (param_3 == '\0') iVar7 = *(int *)(in_ECX + 0x2c); else iVar7 = *(int *)(in_ECX + 0x30);` and picks the
naval table on `*(int *)(in_ECX + 0xa0) != 0` (evidence `0a/sb0a_b1_out.txt`). The rank key is built through
`FUN_00453d40` into a `UniString` and looked up with `record_index`, i.e. by key, not by row position.

> **Main:** the decoders and `GameDatabase::{experience_adjusted_cost, naval_experience_adjusted_cost}`
> are ported; the campaign wiring in the next paragraph is NOT (outside the battle-rules port).

**Wired in round N+2** (§54 (1)): `0x00ED49A0` is now called from the campaign's real cost paths —
`ntw_sim::campaign::economy::recruit_cost` (used by `CampaignModel::recruit`, so the treasury, the queue item
and the refusal all use the adjusted figure) and `economy::unit_upkeep_with_experience` (used by
`faction_upkeep_with`), both via `CampaignRules::xp_adjusted_cost(naval, rank, base)`. The naval twin is the
`naval` table of the same call. Ported as data before, used now.

### The naval twin table, decoded (`unit_stats_naval_experience_bonuses`)
The same proven technique as `2054325` — match the getter's own string to the table name, then validate the
decode by "no leftover bytes":
- `0x00E31710` is the naval getter: it caches into `this+0x2AC`, its first act is
  `FUN_004c4b60("Loading database: %s\n", "unit_stats_naval_experience_bonuses_table")` and it closes with
  `FUN_00e86c80("unit_stats_naval_experience_bonuses_table")` — and `0x00E86C80` is exactly the **name getter**
  DB_BUILDERS.md lists for `unit_stats_naval_experience_bonuses_tables`. The name is CONFIRMED twice.
- Its row struct is `EMPIREUTILITY::UNIT_STATS_NAVAL_EXPERIENCE_BONUS_RECORD` (the `record_index` error string
  inside `0x00ED49A0`'s naval branch); row reader `0x00E862E0`, 7 columns at builder `0x00, 0x0C … 0x20`.
- **Decoded: v0, 10 rows, ranks "0".."9", zero leftover bytes** → the 7-column layout is CONFIRMED. Columns:
  `+0x0C` = the rank itself (0..9), `+0x10` = 0,5,10,10,15,15,20,20,25,25, `+0x14` = 2 × rank,
  `+0x18` = rank / 2, `+0x1C` = 0,10,20,40,60,90,125,160,205,255 (flat), `+0x20` = 1.00 + 0.05 × rank.
  `+0x1C`/`+0x20` are the pair `0x00ED49A0` reads (rank 9: +255 and ×1.45 → a 100-cost ship becomes 400).
  `+0x10..+0x18` have no reader yet (UNKNOWN; they look like three more per-rank stat bonuses, as on the land
  table's `+0x0C..+0x1C`).
- In `ntw_data`: `UnitStatsNavalExperienceBonuses` + `GameDatabase::unit_stats_naval_experience_bonuses`,
  `naval_experience_bonuses(rank)`, `naval_experience_adjusted_cost(rank, base)`; install test
  `naval_experience_bonus_table` (prints every row) and `loads_every_table_exactly` now checks it (v0, 10 rows).

## 20. Round 4 checks
`cargo build --workspace` and `cargo test --workspace` pass; clippy adds no warnings in the code I touched (the remaining
ones in `ntw_ai::battle` and `setup.rs:250` were already on main). Battle run (`--battle`, speed 8, with the new
generals): no panic, battle over at 480 s; 45–116 FPS, dips to 31–33 FPS at the very start and end while other workers'
builds ran (6–10 cargo, up to 39 rustc). Waterloo with the scripts on: the script deploys the first six Prussian units at
154 s battle time (British below 70 % men); screenshot at 185 s with the camera on the eastern entry shows them arriving.

## 21. Reinforcement entry (round 5, item 1)
CONFIRMED:
- At battle creation (`0x005055E0`, per alliance) each army after the first that has an approach angle (army setup `+0xA0`
  flag, `+0xB4` radians) gets an entry group in the battle's reinforcement manager (battle model `+0x2D84`):
  `0x005B0590` first snaps the angle to the nearest entry direction the battle map allows (`0x00EBE420`: a list of u16
  angles in the map object `+0x281AC` → `+0x564`), then builds the group (`0x005A0460`: angle, army, parent army; all its
  units are made inactive) and chains it behind an earlier group with the same angle (`+0x30`), so groups entering at the
  same point come one after another.
- Arrival (`0x00608200`, §13): one unit at a time per group. Once a unit is on the map (`0x00606CE0`, `FUN_005DBD70`) it is
  activated and ordered to walk to the centre of the battle area (`(min+max)/2` of the bounds at battle `+0x84..+0x90`,
  radius 5 m); the AI or a script takes over from there. An "arrived" event (id 0x1A) with its position is posted.
- Not found: the map's list of allowed entry angles (its loader), and exactly where on the edge a unit appears.
Model (PROVISIONAL where noted): entry = playable-area edge in the (unsnapped) approach direction; each unit of a group
enters one slot to the side of the previous one (0, +1, −1, +2, … × 40 m across the direction of travel), is active at once
and walks to the playable-area centre (CONFIRMED target); the next unit waits 10 s. Units still off the field (held or
waiting) are not drawn at all (figures, outline, label). Waterloo at speed 8: the six released Prussian units come in
spread along the eastern edge (screenshot at 225 s).

## 22. Unit capabilities outside battle files (round 5, item 2)
The DB has the per-unit and per-class ability lists: `unit_to_unit_abilities_junctions` (838 rows; earthworks,
chevaux_de_frise, diamond/wedge/square formation, socket_bayonets, fire_and_advance, light_infantry_behaviour, rally,
inspire_unit, gabionade, artillery boosts, fougasse, wooden_stakes) and `unit_class_to_unit_ability_junctions` (7 rows: the
infantry classes get `fire_volley`). Firing drills, more abilities and shot types are unlocked by campaign technologies
(`effect_bonus_value_unit_ability_junctions`: volley fire → mass_fire, platoon firing → platoon_fire_grouped, fire by rank
→ rank_fire, square formation, plug/ring/socket bayonets, …; `effect_bonus_value_shot_type_junctions`: canister,
shrapnel, carcass, explosive / percussive shells, quicklime, …). INFERRED: the unit card is filled from these (the exe
reader that copies them was not traced). Code: `ntw_data` tables `unit_abilities` / `unit_class_abilities`,
`GameDatabase::unit_capabilities` (drill names set the card's drill, the highest wins — PROVISIONAL), used by the battle
setup and the AI's real-battle test; technologies are not applied yet (PROVISIONAL: no campaign technology state in
battles). Battle files keep their own `unit_capabilities` list (it replaces the DB one — INFERRED). Install test
`ntw_data unit_capabilities_from_the_ability_junctions`. AI test after this: Austria 6/8 v default 3/8, France 8/8.

## 23. Battle-script engine, round 5 (item 3)
Survey (`ntw_script --test battle_script survey -- --ignored`): every shipped land battle script run for 40 minutes of
battle time. Before this round the scripts of Austerlitz and Friedland failed on every timer: they look units up by
**name** (`units:item("French_General")` = the battle file's `script_name`, CONFIRMED use) and Lodi looks buildings up by
index (`battle:buildings():item(2)` = the town hall). Now:
- `units:item(name)` finds a unit by its `script_name` (carried in the snapshot; `UnitInfo::script_name`).
- `battle:buildings()` = the map's near building list in file order (INFERRED, Lodi: item 2 `south_euro_townhall`,
  item 52 `south_euro_farmhouse` as the script names them); buildings have `name`, `position`, `health` (100,
  PROVISIONAL), `is_garrisoned` / `currently_garrisoned` (false). Probe: `ntw_formats --example building_order_probe`.
- `camera:move_to(target, position, seconds)` moves our battle camera (applied at once; the argument order is INFERRED
  from the scripts' variable names; `NAPOLEON_BATTLE_CAMERA` overrides). `camera:position()/target()` return the last
  view the scripts set, else the battle file's start view — the cutscenes save and restore it.
- `battle:ui_component(name):set_visible(b)` shows/hides that HUD component (menu_bar, veneer_DY; the radar stays hidden).
- `controller:guard_mode(b)` = hold position (PROVISIONAL meaning).
- Input/escape-key capture, contextual advice, advisor window and camera locks are known no-ops (not logged).
Result: no UNKNOWN stubs left in any shipped script; errors only in the tutorial (TUT_Land, it needs map markers).
Orders the model still ignores (counts over all battles): `select_deployable_object` 44 (deployables),
`perform_special_ability` 30 (formations, unlimber, …), `skirmish` 21, `defend_building` 2, `change_shot_type` 1,
`morale_behavior_fearless` 1. Austerlitz with the scripts on: no script errors, the intro cutscene runs and restores the
start view (screenshot at 150 s battle time shows the armies engaged).

## 24. Open items, time-boxed (round 5, item 4): what was tried and the leads
- Commander alive (army `+0x1B0`): every store form was searched across the whole exe (byte/word/dword MOV, AND/OR/XOR,
  SETcc, INC/DEC, LEAs, copies): the only writer in battle code is the army constructor (`= 1`); `0x00D28415` is a copy
  constructor of another class and `0x0060DAD0` clears a different `+0x214` (a list count). Lead: the flag may never be
  cleared in a land battle, and "general dead / died recently" may only come from campaign-side state or the
  `+0x1B4/+0x1B8` countdowns, which nothing found in battle code starts either. Next step: break on writes in a debugger
  run of the original (needs the game), or trace the commander-death event (`kill_commander` → command queue handler).
- Unit size: the scale (0.25/0.5/0.75/1.0) goes into the battle settings object (`0x00878060`, `+0`) built on the stack in
  `0x00485B90` and passed on with the battle setup. Lead: follow that object into the battle construction and find the
  `MULSS` of the men count by settings `+0`.
- `unit_movement_modifiers` column: the only scaled-index float read `[base + i*4 + 0xC]` in the soldier range is
  `0x006C0260` (a ring-buffer path point, not ground). Lead: find the readers of the ground grid's record list
  (`grid+0x48`, grid at battle-grid `+0xA0`) through `0x0061E430`'s object's methods.
- Gradient `+0x1A0` and formation radius `+0x670`: no new leads beyond §4.1/§6.1.
- 0-B's finding on `battle::autoresolve::engage` (four differences from the exe's `0x00759860`): its exact port
  `ntw_sim::campaign::autoresolve::engagement` is not on main yet; when it is, `battle::autoresolve::engage` should
  call it (only caller: `ntw_sim::campaign::battles` line 87). Not done this round.

## 25. Round 5 checks
`cargo build --workspace` / `cargo test --workspace` pass; clippy adds no warnings in my code (the ntw_ai and
`setup.rs:250` ones were already on main). Battle run (`--battle`, speed 8): battle over at 479 s, no panic;
37–47 FPS for the first 24 s while 3 rustc builds ran, then 83–113 FPS, a dip to 18–20 FPS around the battle end while
three Java (Ghidra) processes of other workers started. Waterloo with scripts at speed 8: six Prussian units released at
154 s enter spread along the eastern edge (screenshot at 225 s). Austerlitz with scripts: no script errors, cutscene
camera restored, armies engaged at 150 s.

## 26. Autoresolve engagement (round 6, item 1)
`ntw_sim::battle::autoresolve::{engagement, PairResult, PairSample}` re-export 0-B's exact port of `0x00759860`
(`campaign::autoresolve::engagement`), and `engage(rates, rout_a, rout_b, men_a, men_b, level)` calls it with a
`KillRates`. The old simplified loop is gone (it fuzzed the rates itself, summed melee and missile, had no range
pre-phase and no shaken rule). It had no callers left: `campaign/battles.rs` already uses `campaign::autoresolve::resolve`.

## 27. Script orders (round 6, item 2)
Every order is a battle command queued by the Lua binding and applied to each unit of the selection (CONFIRMED):

| call | binding | command → handler | unit level |
|---|---|---|---|
| `skirmish(b)` | `0x00613B10` (bool argument) | `BCQ_MULTIPLE_SELECTION_ORDER_CHANGE_SKIRMISH` → `0x005C0770` | `0x005602A0`: `+0xD9C` = b |
| `select_deployable_object(s)` | `0x00613D50` (spaces → `_`, ability enum `0x0057C950`, "unit does not support this special ability") | `..._DEPLOYABLE_ITEM_SELECTION` → `0x005C0120` | `0x005773A0`: `+0xDC0` = ability (`0x00542D60` clears it to 0x16) |
| `perform_special_ability(s)` | `0x00613D30` → `0x00645970` (same checks) | `..._CHANGE_SPECIAL_ABILITY` → `0x005C0820` | `0x006A74E0` mode 0: nothing if already active or `0x0053EA60` refuses; else `0x005612D0` |
| `morale_behavior_fearless/default/rout` | `0x00612F00` / `0x00612EC0` / `0x00612F40` | `BCQ_UNIT_MORALE_CHANGE` mode 0/1/2 → `0x005C2860` | `0x00555970` on the morale component |
| `set_invincible(b)` | `0x00612F80` | `BCQ_UNIT_SET_INVINCIBLE` → `0x005C2B80` | `+0x2E8` = b; the hit test `0x00679B20` skips its soldiers |
| `change_shot_type(s)` | `0x00614090` → `0x00645170` (enum `0x00F59030`, "unit '%S' does not support this shot type") | not followed | |
| `defend_building(b, run)` | `0x00613460` → `0x00645350` ("specified building is not defendable": `+0x1E8 == 0` or `+0x231`) | order `0x005990D0` | not followed |

Morale modes (`0x00555970`, CONFIRMED writes): fearless = state 1, behaviour 0, flag `+0x55`; default = flags `+0x55`
and `+0x56` cleared; rout = UI event 5, state 7, behaviour 5, flag `+0x56` (no rally). `morale::set_script_morale`.

Deployables: at the end of deployment `0x00551BA0` walks every unit and, for each deployable it has (fougasse 9/10,
chevaux de frise 12, earthworks 13, gabionade 14), keeps the selected one and removes the others (CONFIRMED).
`Battle::end_deployment` builds the selected one (PROVISIONAL placement: 3 m in front, the unit's width). The
defences have no effect on movement or fire yet (PROVISIONAL). `wooden_stakes` (11) is a battle-time ability.

Special abilities: `0x005612D0` performs the abilities 2, 3, 7, 17 directly (`0x00578860`) and queues the others as
order 0x30. Model effects: square formation sets the melee square flag; stakes are built at once (PROVISIONAL);
`unlimber` is recorded only (the model's guns are always ready). `0x005646A0`: range × 0.8 while ability 7 (fire and
advance) is active — not applied yet.

Shot types: artillery units get `shot_options` (their gun's other projectiles from `gun_type_to_projectiles`; 48
units can change) and `change_shot_type` swaps the weapon.

`defend_building`: PROVISIONAL walk/run to the building and hold (the model has no garrisons).

### Skirmish (CONFIRMED structure: `0x00585DC0`, `0x0054C9D0`, `0x005859D0`)
- Every fifth tick (`tick % 5 == id % 5`) the check runs if `+0xD9C` is set and the unit has no attack order
  (`0x0053F9E0`). The result is kept in the behaviour object (`+0x24`) and dropped at once when the gate closes.
- Check: for each enemy within 150 m (`0x00701310`; active, not routing) that is moving, a strip from it along its
  movement, its width wide and speed × 8 s long (13 s if the last result was 1), that reaches the unit means danger,
  unless a friendly non-skirmisher nearer that enemy stands on the strip's centre line (`0x00559E20`). Danger gives 1
  if there is room to evade, else 2. Otherwise, if the unit is moving, a strip along its own movement (its width,
  missile range capped at 50 m) that reaches an enemy other than its target gives 2.
- Room (`0x0057A6F0`): a strip along the evade direction, the unit's width wide, `max(depth, speed × 3 s)` long; all
  four corners inside the playable area (battle `+0x84..+0x90`).
- Evade direction (`0x00533BA0`, `0x005341F0`, `0x0054C010`): 32 sectors. Each enemy within 150 m adds `1 − d/150`
  (× 1.5 under 30 m, × 1.5 if it has more than twice the men) to the sectors its formation covers. The histogram is
  smoothed with the kernel `k[n] = 1/2^min(n, 32−n)` (`0x00514BE0`; constant 1.0 at `0x01318048`). The lowest sector
  (first on ties) gives the angle `sector × 2048 + 1024`.
- Evading state (`0x005859D0`): while the result is 1, run 5000 m along the evade direction (kept while the new one is
  within 0xCCD ≈ 18° of it); otherwise stop.
- Sector of a direction (`0x00586B40`): `floor(atan2(x, z) mod 2π × 32/2π)`, at most 31.
- PROVISIONAL: the formation shapes (unit-level model: width `men / 3` m, depth 3 m); the enemy list order (nearest
  first); how a unit enters the evading state (only without a move order of its own).
- The default (`0x005357B0`, called by the unit constructor and later): `+0xD9C` = col 53 ("may skirmish", unit
  `+0x1A5`) && `0x0055C230` && `(unit+0x2C)+0x20 == 2`. `0x0055C230` tests the deployable, `(unit+0x2C)+0x1C == 1` and
  the unlimber ability; what `unit+0x2C` points at is UNKNOWN, so the default stays off (PROVISIONAL). The Borodino
  and Ligny scripts switch skirmish off for whole armies at load time.

## 28. TUT_Land (round 6, item 3)
The land tutorial needed `battle:marker(name)` (`0x00612C30` → `0x0064BC10`: an object with position, rotation in
radians from degrees `+0xC`, scale `+0x10`, visible `+0x14` = 0 when made, model `+0x18`; `set_position` takes a
battle_vector or three numbers, CONFIRMED). The model is `rigidmodels\waypointmarkers\<name>\<name>.rigid_model`
(INFERRED from the pack). The markers are drawn by `napoleon::battle::markers`.

It also needs the player's actions:
- `register_unit_selection_handler(f)`: `f(unit, selected)` on every selection change (INFERRED order: the old unit
  deselected, then the new one selected).
- `register_command_handler(f)`: events named with the exe's strings ("Move", "Move Orientation Width", "Change
  Speed", "Attack Unit", "Change Skirmish", "Change Melee", "Change Formation", "Attack Building", "Special Ability",
  "Shot Type", "Fire At Will", "Withdraw", "Halt", "Double Click", "Double Click Unit Card", "Unit Left Battlefield",
  "Battle Results", … CONFIRMED list). The game sends "Attack Unit" (the target as `get_unit`), "Move", "Halt", "Fire
  At Will" and "Change Speed". `unregister_command_handler()` without a name drops them all (INFERRED).
- `register_input_handler(f)`: camera actions by the exe's names (table `0x014518E0`, CONFIRMED: "move forward",
  "move forward fast", "move backward", "move left", "move right", "rotate right", "rotate left", "move up", "move
  down", "rotate up", "rotate down", "edge rotate/move …"). Keys: arrows or W/A/S/D, Q/E, X/Z (INFERRED).

`suppress_unit_voices` (`0x00612DF0`) and `ui_component:set_highlight` (`+0xD9`, `0x00615630`) are PROVISIONAL no-ops.
The test `tutorial_script_follows_the_players_actions` checks the opening: the script asks for the cannon, hears the
selection, then the attack order on the militia, and orders the counter-attack.

## 29. Round 6 survey
All shipped land scripts: 0 UNKNOWN stubs and 0 orders passed as "Other". Handled: select_deployable_object 45,
perform_special_ability 30, skirmish 21, morale_behavior_* 9, set_invincible 7, defend_building 2, change_shot_type 1.

## 30. Technology unlocks (round 6, item 4)
`technology_effects_junction` (106 rows: technology, effect, value) links to `effect_bonus_value_unit_ability_junctions`
(16 rows: `enable_*` → square, wedge and diamond formation, fire and advance, rank/platoon/volley fire, bayonets,
light infantry drill, inspire, the artillery boosts, improved fougasse) and `effect_bonus_value_shot_type_junctions`
(9 rows: canister, carcass, explosive/percussive/quicklime shells, improved grenades, …). `GameDatabase::
unit_capabilities_with` and `napoleon::battle::setup::shot_options` keep a gated ability or shot only when a
researched technology enables it (INFERRED rule). The campaign has no researched-technology state yet
(`ntw_sim::campaign::world` TODO), so battles pass `None` = everything (PROVISIONAL). This needs 0-B's tech state.

## 31. Round 6 checks
Build and tests pass, except `ntw_campaign::save_compat::user_saves_pass_the_checks`, which reads the user's own
`auto_save.save` (environment; that code was not touched here). Clippy: no new warnings. Runs with scripts at speed 8,
no panics: Austerlitz (cutscene, armies engaged at 150 s), Waterloo (Prussians released at 154 s, 8 defences built),
Dresden (10 earthworks/gabionades built), TUT_Land (advice 1 and 2 reached; the script-fearless militia shows Eager).

## 32. Researched technologies in campaign battles (round 7, item 1)
The campaign model keeps each faction's `FACTION_TECHNOLOGY_MANAGER` entries in `FactionDetails::technologies`
(key, state; state 0 = researched, `effects::TECH_RESEARCHED`, EFFECTS_FIDELITY.md §3). This state is read-only here.
- `napoleon::battle::setup::researched_technologies(model, faction)` returns the researched keys.
- `BattleStart::technologies` (one list per side) → `SetupData` → `make_unit` → `unit_capabilities_with` and
  `shot_options`.
- Refinement of the §30 rule: an enabling effect gates something only if some technology actually has it. Only 11
  technologies carry `enable_*` effects: fire and advance, diamond formation, inspire, artillery barrage and accuracy,
  carcass and quicklime shells, plus naval ones. The Empire leftovers (`enable_square_formation`,
  `enable_canister_shot`, …) belong to no technology and gate nothing (INFERRED).
- France in the Europe start position has `military1_conscription`, `military2_army_corps_organisation` and
  `economy1_division_of_labour`. They unlock nothing, so the line fusiliers keep square formation and lose fire and
  advance (test `campaign_technologies_gate_abilities_and_shots`).
- Not called yet: the campaign does not launch real-time battles (`campaign::play` HOOK). The launch needs to fill
  `BattleStart::technologies` with this helper for each side.

## 33. Deployable defences (round 7, item 2)
Placement (CONFIRMED). The unit's facing is `(sin a, cos a)` in (x, z), the formation angle at formation `+0x28`;
"across" is `(cos a, −sin a)`.
- **Chevaux de frise** (`0x005F14A0`): `n = clamp(floor(width × 0.14492753), 1, 4)` pieces (`0x005EA900`, floor), each
  6.9 m wide, centred across the front, 2 m in front of the formation centre.
- **Earthworks** (`0x005F1710`): `n = clamp(floor(width × 0.21739131), 1, 10)` pieces (`0x005EA960`), 4.6 m apart,
  3.4 m in front. Each piece is 5.35 m deep and 4.6 m wide (`0x0059B120` corners ±2.675 / ±2.3).
- **Gabionade** (`0x005F19C0`): one emplacement per gun. The model has no gun positions, so it uses one
  earthwork-sized piece per gun across the front (PROVISIONAL).
- The width is the formation's `+0x24`. The model now carries `formation_width` / `formation_depth` (from the drawn
  block: files × file spacing, ranks × rank spacing). The skirmish strips use it too.
- At the end of deployment (`0x00551BA0`) the selected defence is built from its placement data
  (`0x00546B40` / `0x00546C80` / `0x00546CC0`). The pieces are physical battle objects (`0x0059BB70`; size arguments
  `0.5, 1.0, 4.0` for chevaux and `0.5, 1.48, 4.0` for earthworks, not decoded).
- Drawn with `rigidmodels\deployableitems\…` (cheval_de_frise, infantry_earthworks, gabionade emplacement,
  wooden_stake, fougasse; paths INFERRED from the pack). Dresden builds 71 pieces, Waterloo 32 (+1 for stakes).

Effects in the unit-level model:
- **Obstacle (PROVISIONAL):** an enemy unit does not walk into a piece; its own side passes.
- **Charges (INFERRED):** chevaux de frise and stakes end a charge that runs into them. `0x005B6B40` returns
  per-soldier factors (0.01, 0.1, 0.2, 0.35, 1) by soldier type and state. Its inputs are not decoded, so no
  casualties are applied.
- **Missile cover (INFERRED trigger):** a unit up to 10 m behind its side's earthwork or gabion piece, shot at from in
  front, gets the CONFIRMED "in cover" −0.2 on the chance to hit.
- **Morale:** the fortification sub-evaluator `0x0053E450` is not about field defences. It needs `0x0055B100`: the
  unit's location is a fort area, or its building is a wall (`+0x231`).

## 34. Building garrisons (round 7, item 3)
- **`models_building`** (reader `0x00DD2660`, CONFIRMED layout; `ntw_formats::models_building`, reads all 538 rows):
  key, model, an int, then entries of a name plus ten values via `0x00DF1930`. The entries are
  `EFLine_piece<NN>_destruct<NN>_line<NN>`: an int (2), a start point, an end point and an outward normal. These are
  the windows the garrison fires from (INFERRED). Lodi's town hall has 83 intact lines and the farmhouse 39; plain
  houses have none.
- **Defendable (CONFIRMED rule of `0x00645350`):** `+0x1E8` set and `+0x231` clear. `+0x1E8` is INFERRED to be the
  fire-line data, so it is set for the buildings with EF lines. `+0x231` marks walls: `0x0056AE70` makes a unit whose
  building (`unit+0x11C` → `+0x14`) has it "on walls" for the range and accuracy wall modifiers. The data does not
  say which buildings are walls yet (PROVISIONAL: none).
- **Capacity** (`0x008554F0`): 0 without `+0x1E8`, else a limit from the building type (`+0x54 → +0x6C`, source not
  found).
- **Model (`ntw_sim::battle::garrison`, PROVISIONAL rules):**
  - one unit per building; it enters within 15 m and is placed at the centre;
  - it fires with at most one man per intact fire line;
  - it is in cover from every side;
  - a move order or routing takes it out;
  - the figures are hidden.
- The scripts' `defend_building` now passes the building's index. `unit:is_currently_garrisoned()` and
  `building:is_garrisoned()` answer from the model. Lodi: the script's two garrisons enter the town hall and the
  farmhouse.

## 35. Skirmish default (round 7, item 4)
`unit+0x2C` is the unit's type record: category at `+0x1C`, class at `+0x20` (CONFIRMED by the AI worker,
`ntw_ai::battle::classes`). So `0x005357B0` sets skirmish on only for a unit that:
- may skirmish (column 53, not dismounted),
- has no gabionade selected,
- is in category artillery (1),
- can unlimber and is not unlimbered,
- is in class `artillery_horse` (2).
No shipped unit has column 53 and is horse artillery, so every unit starts with skirmish off (CONFIRMED by exe and
data together). `Battle::skirmish_default` is applied to every unit at setup.

Related find: `0x0055C9A0(n)` tests "ability n is active", so the missile range's "state 7" (`0x005646A0`, × 0.8) is
ability 7, fire and advance. `LandUnit::missile_range` applies it.

## 36. Round 7 checks and FPS
- Build passes; `cargo test --workspace`: all 47 result lines ok (the save test passes again).
- Clippy: no new warnings (`setup.rs:294` is the old `deployment_area.clone()` one).
- Script survey: 0 stubs, 0 orders passed as "Other".
- Austerlitz and Waterloo with scripts, speed 8: no panics. Waterloo releases the Prussians at 154 s; 32 defences
  built.
- FPS, Austerlitz, debug build, `NAPOLEON_FPS_LOG=90`, 40 units / 4350 men: mean 70 FPS, 19.5 in the first 2 s
  (loading), 40–43 FPS from 6 to 22 s, 70–106 FPS from 24 to 56 s, 59–69 FPS to the end. Worst frame 65 ms at
  16 s.
- Machine: the user's original `Napoleon.exe` was running all the time. Two cargo processes were running before the
  run and none after. CPU load was 100 % before and 68 % after.

## 37. Defence casualties (round 8, item 1)
`0x005B6B40` is not damage. It reads the battle's environment state object (battle `+0x28 → +8 → +0xB0 → +0x198`, the
one the fatigue code reads: its current state descriptor `0x005B9510` with fields `+0x10` and `+0xC`) and the
object's own `+0x28 / +0x2C / +0x38`. It returns 0, 0.01, 0.1, 0.2, 0.35 or 1. These are generic battle-object
methods (the slots beside it, `0x005B7010` and others, read a unit at `+0x28` for sound or priority weights), not
defence-specific.

The damage is in the soldiers' collision handlers, which call the hit dispatch `0x0080A4E0` (the same dispatch
projectile and blow hits go through; it skips invincible soldiers, `0x00679B20`):
- **Chevaux de frise `0x006E94E0`** (CONFIRMED): the soldier has a unit (`+0x1F0`), belongs to another alliance,
  its vfunc `+0xF0` answers true and it is alive (byte `+0xEC`) → one hit of strength 0.75
  (`{0, 0, 0, 0, 0.75}`), sent through the soldier's vfunc `+0x104`.
- **Stakes `0x006E9160`** (CONFIRMED): the soldier is an enemy, alive, its `+0xF0` or `+0xE8` answers true, and its
  heading is within 0x238E (≈ 50°) of head-on to the object → one hit of strength 1.0.

Port (`abilities::Battle::defence_contact`, called when a move runs into an enemy piece):
- INFERRED: `+0xF0` = running or charging, `+0xE8` = moving.
- PROVISIONAL: each man of the front that touches the piece (`min(unit width, piece width) / 1 m`) is hit once per
  contact, and a hit kills with a chance equal to its strength. What the soldier's `+0x104` does with a hit is not
  decoded.
- Test: a 60-man charging unit loses some men against chevaux and stops.

## 38. Walls and garrison capacity (round 8, item 2)
- **Walls (CONFIRMED):** the building setup `0x00688DD0` sets `+0x230` and `+0x231` when the building record's
  category (`+0x14 → +4`) is 8. The category enum `0x00E4E9A0` is: armoury 0, barracks 1, boulder 2, bridge 3,
  church 4, command_HQ 5, farmhouse 6, fence 7, fort 8, gate 9, house 10, hut 11, incidental 12, ruins 13, rural 14,
  townhall 15, warehouse 16, windmill 17.
- So the walls are the `battlefield_buildings` rows of category `fort` (column 1; that the record is this table is
  INFERRED). The setup marks them, so they are refused by `defend_building`.
- **Capacity (`0x008554F0`, CONFIRMED structure):**
  - 0 without `+0x1E8`;
  - otherwise `0x006F22C0` counts the garrison object's lines (0x58 bytes each) and, in each, the soldier slots
    (0x30 bytes each) whose vfunc answers true (`0x006DB8E0`);
  - the count is capped by the building type's `+0x54 → +0x6C`.
- Who fills `+0x1E8` and `+0x6C` was not found among the writers scanned (they belong to other classes). Capacity is
  not enforced in the model (PROVISIONAL).

## 39. Open items, time-boxed 1 h (round 8, item 3)
- **Unit size (resolved for battle files):** the parser `0x0050CAE0` reads `num_soldiers` and passes it to the card
  builder `0x00513440`, which stores it at card `+0xC8` and `+0xCC` unscaled (CONFIRMED). The battle-file men are
  used as is. The unit-size setting (settings `+0`, `0x00878060`) scales DB-built units in `0x004A6600` — FOUND,
  see §18a (truncating MULSS on the army card's `+0xA`, into card `+0xC8`).
- **Commander alive (resolved meaning):** army `+0x1B0` is "the army has a commanding unit". `kill_commander`
  (`0x00611BE0`) refuses with "error: army does not have a commanding unit" when it is clear, and otherwise kills
  the commander unit's soldiers through a command. The flag is set once at construction and never cleared, so
  "commander dead" comes from the commanding unit's state. That is how `Battle::general_status` already works (the
  general's unit with men left).
- **Formation radius `+0x670`:** the skirmish room test `0x0057A6F0` uses formation `+0x670` as a length beside
  `+0x650` (width) and `+0x654` (depth). The writers found (`0x0059EC50`, `0x0068C750`, `0x0071FC80`) are other
  classes. Still UNKNOWN; `FORMATION_RADIUS` stays 0 (PROVISIONAL).
- **Movement modifiers and the slope gradient:** not reached in the time box; leads as §24.

### Round 8 checks
Build and `cargo test --workspace` pass (all 47 result lines ok). Clippy: no new warnings (`setup.rs:309` is the old
`deployment_area.clone()`, `battle_markers.rs` is untouched). Waterloo and Austerlitz with scripts at speed 8: no
panics; Waterloo builds 32 defence pieces plus the Rifles' stakes.

## 40. What a hit does to a soldier (round 9, item 1)
- **Hit points:** a soldier entity has hit points at `+0x624`. `0x00816F70` takes `damage` off them and reports
  death when they reach 0 (CONFIRMED). The starting value is INFERRED to be `battle_entities` column 20: men and
  horses 1, camels 1, elephants 3, guns 25, caissons 50, gun trains 300.
- **Projectile hit `0x006A6400`** (CONFIRMED structure):
  - the impact roll `0x00DAADF0` (`missile::impact`) gives a code;
  - code 5 (kill) goes to `0x007F1860`: hit points −= `max(1, round(damage))`, and if dead the death dispatch;
  - codes 3/2 knock the soldier down (`0x007F19B0`) and code 1 staggers him (`0x007F1C60`), unless the projectile's
    damage is below 1;
  - soldiers for which vfunc `+0xEC` answers true skip the roll and take the hit-point damage only when the damage
    is ≥ 1;
  - kills are counted by range bin (unit `+0xB78…`).
- **Melee kill `0x007F1770`** (from the melee encounter `0x006AFE20`): outcome 5 takes the damage off the hit
  points (dead → 5). A soldier who survives (more than one hit point) gets one battle-LCG roll: < 0.6 → 1,
  < 0.9 → 2, else 3 (stagger / knockdown).
- **Death dispatch `0x0080A4E0`:** checks `0x00679B20` (invincible units are skipped; there is also a short
  per-soldier immunity window `+0x5DC`), then the soldier's `+0x104` (die, with the dispatch's impulse) and, if
  `+0xF0` (a rider), the same for his mount `+0x90`.
- **Check of our kill paths:** the model's men have one hit point, so "Kill → one man less" in shooting and melee
  matches the exe. The stagger/knockdown outcomes take no men, as in the model. Multi-hit-point entities (guns,
  caissons) are not targets in the unit-level model.
- **Defences (replacing the round 8 placeholder):** chevaux de frise (`0x006E94E0`) and stakes (`0x006E9160`) call the
  death dispatch directly, with impulse 0.75 / 1.0, for an enemy rider (`+0xF0`); stakes also take a mount (`+0xE8`)
  and need a heading within 0x238E of head-on.
  - There is no roll, and neither hit points nor armour are involved. The 0.75 / 1.0 is the impulse, not a chance.
  - Infantry is not hurt.
  - `Battle::defence_contact`: the cavalry front touching the piece dies, once per contact (unit-level, PROVISIONAL).

## 41. Movement modifiers and gradient (round 9, item 2)
- Ground-grid readers: `0x006C3260` reads the grid cell under an entity and maps the type to 5 classes. These are
  INFERRED to be footstep or dust effect classes:
  - 0 field_ploughed, 4 mud, 9 sand, 11 scree, 20 → 3;
  - 2 field_forest, 13 stone_masonry, 14 dense forest, 16, 21 → 1;
  - 15 light scrub, 17, 19 → 4;
  - anything else → 0.

  This is not speed.
- The record-list readers checked: the 9 functions that both scale by the grid's inverse cell size (`+0x24`) and load a
  `+0x48` list. None reads `UNIT_MOVEMENT_MODIFIER_RECORD` floats.
- The column semantics from the data alone (INFERRED): column 4 is the best in woods (dense forest 0.4/0.4/0.4/0.7),
  column 3 next. Columns 1–2 are lowest everywhere. `MovementClass` stays as it was (PROVISIONAL).
- The gradient `+0x1A0`: not reached.

## 42. Garrison data, capacity, formation radius (round 9, item 3, 45 min)
- **Garrison data (CONFIRMED):** `+0x1E8` is set by `0x006FA380` (called from the building setup) to the garrison
  object `0x0068CE40` built from the building model's `models_building` entries. The entry stride is 0x34, the same as
  the reader `0x00DD2660`.
  - Each entry becomes a garrison line (`0x0068D160`, 0x58 bytes).
  - `0x006AA630` lays `max(1, floor(length / spacing))` soldier slots along the line.
  - The spacing (`0x00E60E80`) is `2 × r`, or `2.5 × r` on a wall building. `r` (`0x00E619D0`) is the largest
    `+0x4C` (a radius) among the entity types in a list.
- The setup now counts slots this way, with `r` = 0.5 m (PROVISIONAL; the source of `r` was not followed). This
  gives the garrison's shooters.
- **Type cap `+0x6C`:** not found.
- **Formation radius `+0x670`:** not found (the three writers are other classes' constructors setting 0).

### Round 9 checks
Build and `cargo test --workspace` pass (47 result lines ok). Clippy: no new warnings (`setup.rs:327` is the old
`deployment_area.clone()`). Waterloo, Austerlitz and Lodi with scripts at speed 8: no panics.

## 43. PROVISIONAL / PLACEHOLDER sweep (round 10, item 1)
`git grep -n "PROVISIONAL\|PLACEHOLDER" crates/ntw_sim/src/battle crates/napoleon/src/battle` gave 120 lines before this
round. They are grouped here by what they change in a battle, highest impact first. "Done" marks items resolved this
round; the rest keep their lead.

| # | Item (file) | Impact | Status / lead |
|---|---|---|---|
| S1 | Straight-line unit movement, auto-advance on the nearest enemy, contact range 10 m, advance-stop fractions (`model.rs` 36/75/588/672–718, `shooting.rs` 48/331) | very high: all manoeuvre and contact | The exe moves soldiers (slot 2 `0x005857A0` is only the on-field check, done below). Real locomotion is per soldier (`0x006543D0` speed, `0x00819770` step). The unit-level model keeps its own movement; the goal choice is the battle AI's job (0-D/AI). Lead: the soldier path follower around `0x00819770`. |
| S2 | Speed: ground column and slope | high | **Done:** column per unit type `0x006543D0` (§44); slope speed and gradient `0x00819770` (§44). Still open: the fatigue speed factor (type `+0xE4` + state × 4, `0x00649520`). |
| S3 | Men firing per volley `1/2` (`shooting.rs` 28/41), target choice (`shooting.rs` 290), fire-hold in melee (279), reloading every tick (367) | high: firefight lethality | **Round 17 (§52):** DONE. Shooters per drill ported (`volley_plan`); the half-the-men PLACEHOLDER is gone. Target choice and the melee hold stay PLACEHOLDER. |
| S4 | Melee frontage and exchange rate (`model.rs` 77, melee doc) | high | **Round 11:** exchange every 5 s (INFERRED from the fight object `+0xA8`). Round 12: the musket clips (attack 2.0–2.9 s, combat idle 1.0–9.3 s) fit a cycle of about 5 s (§47). Pairing still open. |
| S5 | Chance-to-hit control / visibility / angle factors 1.0 (`shooting.rs` 50) | medium | **Done (round 12, §47):** control, visibility, angle judgement, woods cover and the army level terms are ported. Still open: the `fatigue_effects` column read as the control base (1.0). |
| S6 | Under-fire memory 50 ticks (`shooting.rs` 45, `model.rs` 581) | medium (morale) | Slot 4 `0x005828A0` (CONFIRMED) counts the unit's `+0xC78` timers `+0xC80/+0xC84/+0xC88` down by 1 per tick and computes a casualty-rate state. The start value (the setter) was not found; the model's 50 stays. |
| S7 | Reinforcement entry point and walk-on (`model.rs` 46/449/1070/1104, `setup.rs` 358) | medium | **Done:** a reinforcement joins (`+0xAA0` = 1) when its whole formation is inside the playable area (`0x005857A0`, §44). The edge spawn point and the 40 m spread stay PROVISIONAL. |
| S8 | General "died recently" 600 ticks (`model.rs` 42, 1033) | medium (morale) | Army `+0x1B0` resolved (§39); the `+0x1B4/+0x1B8` start values were not found. |
| S9 | Formation shapes in the unit-level model (`abilities.rs` 19/51/222/227, `FORMATION_RADIUS`) | medium | Width and depth now come from the drawn block (files × spacing). `+0x670` was not found (§42). |
| S10 | Defences: obstacle rule, cover zone 10 m, chevaux depth, gabion pieces per gun, stakes strip (`abilities.rs` 154–204, 631, 652) | medium | Placement CONFIRMED (§33); the contact kill is CONFIRMED (§40). Obstacle and cover are unit-level stand-ins for the physics. |
| S11 | Garrisons: one unit per building, 15 m entry, cover from every side, `+0x6C` cap (`garrison.rs`) | medium | Data and slots CONFIRMED (§42). **Done:** slot radius from `battle_entities` (§44). |
| S12 | Skirmish geometry and enemy order (`abilities.rs` 300/321/411) | low–medium | Structure CONFIRMED (§27). |
| S13 | Morale bookkeeping: recent losses, kill ratio, extended casualty ratio, flank slots (`model.rs` 149/151/613/928/985/986, `morale.rs` 791) | medium (morale) | **Done (rounds 11–12):** the casualty rings (§46); the per-tick counter clear is CONFIRMED in slot 4 (§47). |
| S14 | Unit retirement at 0 men (`model.rs` 575) | low | — |
| S15 | Strength category mapping, start-men as card count (`strength.rs`) | low | Category enum CONFIRMED by the AI worker (`ntw_ai::battle::classes`); the mapping could use it. |
| S16 | Orders and scripts: charge speed, stand-off, guard mode, step sizes (`orders.rs`, `scripts.rs`, `hud.rs` 539) | low | UI/script stand-ins. |
| S17 | Presentation (`view.rs`, `skin.rs`, `labels.rs`, `hud.rs` 137/150/440/469/484, `input.rs`, `setup.rs` 11/72/74/283/504/681) | none on the outcome | Owned by the UI/units workers (0-D/0-E) or by the campaign launch (§5). |
| S18 | Drill names with several drills (`attributes.rs` 107) | low | — |

Stale comments fixed this round: the held-reinforcement note ("scripts do not run" — they do), the general status
note ("no generals" — there are), the fortification sub-evaluator (its test is now known), the garrison shooter
comment.

## 44. Round 10 decodes (items 2 and 3)
- **Movement-modifier column (CONFIRMED, `0x006543D0`).** The soldier speed update reads the ground record of its cell
  (soldier `+0x19C`, set by `0x0081AC80` from the ground grid, or from the building it stands on):
  - artillery with guns (`0x0055AB90`): `artillery_foot` → column 0, `artillery_horse` → column 1, others → 1.0;
  - mounted (`0x0055ABF0` / `0x0055C2A0`: cavalry, camels, dragoons not dismounted) → column 2 (dismounted → 3);
  - infantry and the rest (`0x0055C1C0`) → column 3.

  It multiplies the soldier speed `+0x1A4`. `MovementClass` now follows this. Before, infantry used column 2 and
  cavalry column 1.
- **Slope (CONFIRMED, `0x00819770`).**
  - The step ahead is `clamp((speed + `+0x158`) × 0.1, 0.01, `+0x58`)`.
  - The gradient `+0x1A0` = (ground height there − current height) / step.
  - Speed factor: uphill `1 / (1 + 3g)`, downhill `min(1 − g, 1.5)`.
  - Applied in the movement; the fatigue gradient multipliers read the same gradient.
- **On the field (CONFIRMED, `0x005857A0`, update slot 2).** Unit `+0xAA0` becomes 1 once every soldier's circle
  (position, radius `+0x58`) is inside the playable area (battle `+0x84..+0x8C`). It becomes 2 when a leaving unit
  has a soldier outside. Reinforcements now join that way.
- **Update slot 4 (CONFIRMED, `0x005828A0`).** It decrements the unit's under-fire timers (`+0xC80/+0xC84/+0xC88`) and
  computes a casualty-rate state.
- **Garrison slot radius (`0x00E619D0`).** It is the largest radius `+0x4C` over the `battle_entities` records (table
  loaded by `0x00E100B0`, "battle_entities_table") whose `+0x18` is 0. INFERRED:
  - the radius is column 12 (men 0.35, horses 2.0, guns 1.5);
  - `+0x18` is the class with infantry 0;
  - so the radius is 0.35 m and slots are 0.7 m apart (0.875 m on walls).

  `setup::garrison_radius` reads it from the data.

### Round 10 checks
- Build and `cargo test --workspace` pass (47 result lines ok).
- Clippy: no new warnings (`setup.rs:338` is the old `deployment_area.clone()`).
- Waterloo and Austerlitz with scripts at speed 8: no panics.
- Waterloo plays out faster now. The Prussians were released at 124 s (before: 154 s) and the battle ended at 211 s
  (side 1 won). The decoded speed rules make infantry use the faster column 3 and add downhill speed.

## 45. Why Waterloo ended at 211 s (round 11, item 1)
Run: `NAPOLEON_BATTLE_TRACE`, speed 8, scripts on (trace in `target/tmp/wat_trace.csv`).
- **Rule:** `kill_or_rout_enemy`. At 211 s every French unit was out of the fight (19 of 20 routing or shattered, the
  20th destroyed). There was no time-out and no script end.
- **Course:** first losses at 30–50 s. France went from 1992 to 128 men in 150 s, the Allies from 2872 to 1676. Losses
  by the nearest enemy's distance:
  - France: 1194 in melee (≤ 12 m) and 670 to fire;
  - Allies: 599 in melee and 596 to fire.

  Nobody lost men while routing. Units fought on to a handful of men (40 trace rows of units under 40 % still
  "Normal"), e.g. Fusiliers of the Line 113 → 27 men in 10 s at morale −5.
- **Causes (by the CONFIRMED rules):**
  1. The "recent casualties" ratio was our PLACEHOLDER: the losses since the last full evaluation (0.5 s). The penalties
     (−1 at 6 % … −15 at 50 %) almost never triggered.

     The exe keeps the ratios in the unit's `+0xC78` object (`0x005828A0`, slot 4, CONFIRMED). Each tick it pushes
     (kills, deaths, men + deaths) into a ring buffer (`0x00582B60`). Then:
     - recent `+0xCBC` = Σ deaths / max men over the ring (`0x005692F0` / `0x00569200`);
     - `+0xCC0` = Σ kills / max men (`0x005695D0`);
     - every 10 ticks the same over a second, longer ring gives the extended ratio `+0xCC4` (and `+0xCC8`).

     The ring sizes are being read.
  2. The Wavering clamp (CONFIRMED) holds morale at `ums_wavering_threshold_lower` (−5) while the waver timer runs.
     The timer is `waver_base_timeout` 40 + 5 × unit index ticks, so up to about 24 s for late units. A unit cannot
     break during it. With our fast melee that is long enough to lose most of the unit.
  3. Melee cadence (PLACEHOLDER): up to 40 pairs per unit pair, one exchange per pair every 2 s, so about 20 blows per
     second per encounter. This is item S4.
- **Next:** port the casualty ratio rings (S13), then the melee cadence (S4), and rerun.

## 46. Round 11 fixes
- **S13, casualty ratios (CONFIRMED structure, `ntw_sim::battle::casualties`).** Slot 4 `0x005828A0` keeps a 40-tick
  ring and a 60-entry ring (one push per 10 battle ticks) of (kills, deaths, men before the deaths). From them it
  computes:
  - the recent ratio `+0xCBC`, the kill ratio `+0xCC0`, the extended ratio `+0xCC4` and its kill ratio `+0xCC8`
    (Σ / max men);
  - the fighting category `+0xD08`: 1–3 winning, 4 even, 5–7 losing, 0 idle;
  - all of these feed the morale modifiers.

  The model counts deaths as the drop in men since the unit's last update. Where the exe clears its per-tick
  counters was not found (INFERRED: every tick). On its own this barely changed Waterloo (it ended at 207 s).
- **S4, melee cadence.** The fight object `0x00664E80` resolves one exchange (`0x006AFE20`), picks the combat animations
  and sets its `+0xA8` to a time + 5.0 s (20.0 s for a charge) (CONFIRMED constants). The pair exchange interval is
  now 5 s (INFERRED; was 2 s). Waterloo then lasted 549 s: France held, rallied and broke over 9 minutes instead of
  3.5.
- **S6, under-fire timers (CONFIRMED).** Hits set the unit's timers, and slot 4 counts them down:
  - small-arms `+0xC84` = 60 ticks (`0x00566B90`);
  - artillery `+0xC80` = 300 ticks (`0x00566910`, which also starts 300 on the shooter's unit);
  - "in melee" `+0xC88` = 50 ticks (`0x00566A70`, from the fight object `0x0066C3B0`; not used by the model yet).
- **S3, per-soldier firing:** not ported this round. The model fires half the men per volley at the unit's reload
  cadence. Lead: which soldiers fire is decided per soldier by the drill (front ranks); the callers of `0x006A5CE0`
  (`0x005B6710`, `0x005CCA60`, `0x006FB740`, `0x00806AC0`).

### Round 11 checks
- Build and `cargo test --workspace` pass (47 result lines ok). Clippy: no new warnings.
- Full runs with scripts, speed 16, no panics:
  - **Waterloo:** over at 549.5 s, side 1 won (all French units out of the fight). France 1992 → 441 men, 19 of 20
    routing; Allies 2872 → 562. Before the round it ended at 211 s.
  - **Austerlitz:** over at 442.1 s, side 0 (France) won. Russia/Austria 2158 → 708 men.
- The harness exit code 124 is the screenshot timer, set past the end on purpose.

## 47. Round 12: shot factors, soldier firing, ammunition, counter clear, melee clips
### S5, the chance-to-hit factors (CONFIRMED, ported in `missile`)
`0x006A5CE0` builds two adapters for `0x00DAB9D0`:
- the shooter adapter (vtable `0x0133A590`, ctor `0x00691930`): the soldier, the army level flag and the level;
- the shot adapter (vtable `0x0133A5CC`, ctor `0x006919A0`): the shot, the same flag and level.

The slots:
- **Core marksmanship** (shooter `+0x10`, `0x006B8D50`): the unit's accuracy `+0x160`, or 50 with no unit. Army level
  multiplier: flag clear: level 1 ×1.3, 2 ×1.5, −1 ×0.85; flag set: 1 ×1.25, −1 ×0.9, −2 ×0.75.
- **Marksmanship bonus** (shot `+0x4`, `0x006D8B00`): the projectile's `+0x70`, which is `projectiles` col 15
  "accuracy modifier" (howitzer shells −50, rifled naval shot +50, muskets 0). On walls a kv value is added
  (slot not identified). Army level bonus: flag clear: level 1 +20, 2 +30; flag set: 1 +15.
- **Control** (shooter `+0x28`, `0x006B8AA0`). The base is `+0x18` of the unit's fatigue-effects record
  (`0x00649520`), or 1.0 without one. Then:
  - morale state `+0xC28` 4 (shaken): −0.1; 5 (wavering): −0.2;
  - under small-arms fire `+0xC84`: −0.1;
  - under artillery fire `+0xC80`: −0.1;
  - in melee: −0.2. The test is `0x0055B200` on the formation `+0xADC`; it is the predicate behind the script
    function `BattlePlayerUnitEngagedInMelee` (binding at `0x004072B0`).

  Which `fatigue_effects` column lands at `+0x18` is UNKNOWN. The model uses 1.0 there (PROVISIONAL).
- **Visibility** (shot `+0xC`, `0x00700EB0`): `1 − 0.1 × intensity`. The intensity is `+0xC` of the battle's
  current weather record (environment object battle `+0x28 → +8 → +0xB0 → +0x198`, current entry `0x005B9510`). That
  record is a `battle_weather_types` row:
  - col 1 at builder `0xC` is the intensity: dry 0, light 1, heavy 2, torrential 3;
  - col 2 at `+0x10` is the kind: 0 rain, 1 snow, 2 dust, 3 none. It is the kind that the rain and snow tests
    `0x005DBB30` / `0x005DBD60` read.

  The model has `Battle::weather_intensity`, which stays 0 until a battle picks a weather.
- **Angle judgement** (shot `+0x10`, `0x006A3410`): `1.5 − (clamp(θ°, −90, 90) + 90) / 180`. θ is the shot's
  launch elevation `+0x4C`, from the solver `0x006A23D0` (called by `0x006A2C90` in the FIRE state):
  - **low / high:** `tan θ = (v² ∓ √(v⁴ − g(g·d² + 2·y·v²))) / (g·d)`, with constants 9.8, 96.04 and 19.6;
  - **fixed:** θ = the maximum elevation;
  - **rocket:** θ = atan(y / d) (INFERRED: CRT call);
  - **no solution, or θ above the maximum elevation:** no shot.

  The projectile runtime fields are at the builder offset − 0x1C (INFERRED):
  - `+0x5C` trajectory class (col 10; low 0, high 1, fixed 2, rocket 3);
  - `+0x60` range;
  - `+0x64` minimum range;
  - `+0x68` maximum elevation;
  - `+0x6C` muzzle velocity;
  - `+0x70` accuracy modifier.

  A musket (150 m/s) at 80 m on the level aims 1.0° up (factor 0.994). A mortar at 45° gets 0.75.
  `MissileWeapon::ballistics` carries these columns.
- **In cover** (shot `+0x14` → shot byte `+0x34`). The FIRE state sets it from `0x00817130`: the target soldier's
  ground type `+0x19C` is 2, 14 or 16 (field_forest, vegetation_dense_forest, vegetation_medium_woodland). That is
  −0.2 on the chance to hit. A soldier in a building has ground type 0x15, so he is not in cover by this test. The
  model's earthworks, gabions and garrison cover stay PROVISIONAL stand-ins for the projectile physics, as before.

### S3, soldier firing (state machine CONFIRMED; who fires still UNKNOWN)
The musket soldier's fire behaviour is an FSM. Its state names are at `0x01453D40`: WAIT, PIPE, FINISHED, START,
TURN, GET_AMMO, FIRE, RELOAD, RELOAD_TO_ST…, FINISH, STOP. The state functions are thunked at
`0x007DE8A0`–`0x007DEA40`:
- **GET_AMMO** (enter `0x00805ED0`): it takes one round from the unit's pool (`0x00576480`, below). With no round
  left the soldier's behaviour ends. The way on is gated by `0x00806950`: soldier `+0x1EC` → byte `+0x117`. That byte
  is read in about 60 places; its meaning is UNKNOWN.
- **FIRE:**
  - enter `0x00805B00`: counts the shot (`+0x70`), stores the start time and picks and plays the fire clip
    (`0x0064A210`);
  - update `0x00806AC0`: once the clip reaches its fire point (`0x0064A430`), it builds the shot, solves the
    trajectory, rolls the chance to hit and starts the reload timer (`0x00639E40` through soldier vfunc `+0xD0`);
  - exit when the clip ends (`0x0062DF10`).
- **RELOAD:** plays clip 0x1B (`0x00806010`). It leaves when the clip has ended and soldier vfunc `+0xDC` agrees.

So each man fires on his own cycle: GET_AMMO, then FIRE (shot at the fire point), then RELOAD (the reload time runs
from the shot). There is no unit-wide volley in this machine. The musket clips are FIRE 1.0 s, RELOAD_1 1.4 s and
RELOAD_2 0.7 s (`clip_durations man_musket`), short next to the reload time, so the cycle is about reload time +
fire point. What decides which men enter it (front ranks, the drill's rank rotation) was not found. The lead is the
gate `+0x1EC → +0x117` and the drill objects (`0x005498B0`). The model keeps half the men per volley (PLACEHOLDER).

### Ammunition: one cartridge pool per unit (CONFIRMED, ported as `shooting::AmmoPool`)
- At unit creation (`0x0051B7D0`): `+0xE10 = +0xE14 = (men +0x204 − +0x27C) × +0x1A0`, the men times the rounds per
  man. `+0x27C` is UNKNOWN (taken as 0).
- Every GET_AMMO takes one round (`0x00576480`). At 0 the unit raises event 0x1E (out of ammunition).
- A soldier who dies takes `ceil(pool / men)` rounds with him (`0x00574E90`, from the hit dispatch `0x0080A4E0`).
- The UI value `+0xE18` = `pool / men × start men / start pool`.

Before this, the model used one round per volley. Now each man who fires takes one round, so a unit that fires half
its men per volley lasts twice as many volleys. `LandUnit::ammunition` is now the rounds per man left. The label
reads "rounds per man", and the scripts' starting ammunition is the starting rounds per man. Artillery keeps one
round per volley (PROVISIONAL: its crew FSM is another caller, `0x006FB740`).

### S13, the per-tick counters (CONFIRMED)
Slot 4 `0x005828A0` clears the per-tick kill and death counters `+0x54` / `+0x58` right after the recent push. It
clears the 10-tick counters `+0x5C` / `+0x60` right after the extended push (every tick with battle tick % 10 == 0).
The model's step already counts the same way.

### S4, the melee clips
The musket man clips: ATTACK 2.0 / 2.9 s; COMBAT_IDLE 1.0–9.3 s (mean about 3.1 s). An exchange plus an idle is
about 5 s, which fits the fight object's +5 s (INFERRED). The interval stays 5 s.

### Round 12 checks
- Build and `cargo test --workspace` pass (49 result lines ok). Clippy: no new warnings (`setup.rs:347`
  `deployment_area.clone()` is old).
- Full runs with scripts, speed 16, no panics. Exit code 124 is the screenshot timer, set past the end.
  - **Waterloo:** over at 229.9 s, side 1 won (it was 549.5 s in round 11). France 1992 → 426 men; Allies 2872 →
    1757.
  - **Austerlitz:** over at 754.1 s, side 0 (France) won (it was 442.1 s). France 2192 → 1102; Russia/Austria
    2158 → 606.
- **Attribution (Waterloo).** I switched each new rule off in turn, with a temporary environment switch that is not
  committed. All off gives 549.5 s, so the merge of main changed nothing. Each rule off alone:

  | Rule off | Waterloo ends at |
  |---|---|
  | control | 229.9 s |
  | cartridge pool | 229.9 s |
  | marksmanship bonus | 229.9 s |
  | woods cover | 251.9 s |
  | angle judgement | 259.1 s |
  | woods cover and angle judgement | 578.9 s |

  So the change comes from the two terrain rules together:
  - Allied units in the Hougoumont woods take −0.2 on every French shot.
  - The French fire up the ridge (angle < 1), and the Allies fire down it (angle > 1).

  The French attack loses the first firefight, routs early, and the pursuing Allies cut down the routers: France
  goes from 1200 to 426 men between 184 s and 226 s. Both rules are CONFIRMED in the exe. The battle is very
  sensitive to who wins the first exchange, so its length moves a lot with small changes to hit rates.

## 48. Round 13: routing casualties, the charge flag, the fire gate, weather, fatigue effects
### (1) Routing-casualty check
I logged every kill of the round 12 Waterloo run with its cause, using temporary prints that are not committed.

**Routers are not easy prey.**
- No missile kill hit a routing unit. Fire at will skips units that are out of the fight.
- Melee blows on routing units were 68 of 1487 melee kills (round 12 rules) and 154 of 1630 (after the fix below).
  - A routing soldier has selection weight 0, so he never strikes. He can still be struck while an enemy is within
    the 10 m contact range.
  - Routers run directly away from the nearest enemy at their run speed. Chasers only reach them when ordered to.
- No unit leaves the map in the model: the exe sets unit `+0xAA0` = 2 and removes such units. That changes morale
  and victory only, not kills.

**The real cause of the 1200 → 426 drop: a charge that never ended.** France lost those men while its units were
still fighting (behaviour Normal). In the round 12 run:
- every Allied melee kill (1177) was a charged blow;
- no French kill (310) was a charged blow.

A charging attacker has top selection priority and adds its charge bonus, so the charging side strikes almost every
blow. The AI orders a charge and keeps `charging` set until its own hold timer ends (`ntw_ai`, `charge_hold_ticks`).
Each new approach sets it again.

In the exe the combatant's charging flag `+0x40` is the soldier's action `+0x1B8 == 0xD`, the charge run
(CONFIRMED, combatant builder). A soldier who is fighting is in a combat action instead: 0x14–0x16 in the fatigue
table's "combat" group. So a charge counts for the impact only.

**Fix (INFERRED unit-level mapping), `LandUnit::charging_now`:**
- A charge order counts for the unit's first `MELEE_EXCHANGE_INTERVAL_TICKS` (5 s) in contact. That is one charged
  exchange per engaged pair.
- It comes back only after the unit has been out of contact for the same 5 s.
- `charging_now` feeds the combatant and the selection priority. The morale charge timer still follows the order
  (`0x0055AC20`).

After the fix, with the round 12 rules otherwise unchanged:
- Waterloo ends at 638.1 s (side 1 won). France 1992 → 423, Allies 2872 → 439.
- Austerlitz ends at 754.1 s, as before.
- 325 Allied kills were still charged blows: one per pair at each new contact.

### (2) S3: the gate into FIRE, and the drill names
- **The gate `0x00806950`** is the soldier's `+0x1EC` entity, byte `+0x117`. It is an entity byte, not an army one:
  - `0x006EDE70` reads `+0x1EC` and falls back to the entity itself when it is null;
  - `0x00646C20` returns "stopped" when `+0x117 == 1` and the speed `+0x194` is below 0.01;
  - the slope test (§4) uses `+0x117 == 0` as "moving".

  INFERRED: `+0x1EC` is the entity that carries the soldier's position, and `+0x117` means "at rest". So a soldier
  leaves GET_AMMO for FIRE only when he stands still. The model already does not fire while moving.
- **Drill names** (`0x00554990`): 0 fire_volley, 1 mass_fire, 2 platoon_fire_dispersed, 3 platoon_fire_grouped,
  4 platoon_fire_column, 5 rank_fire (CONFIRMED).
  - The drill comes from the formation's current order object: the stack `+0x418`, the index `+0x460`, vfunc `+0x30`.
  - Its vfunc `+0x14` is "engaged in melee" (§47).
- **Which soldiers fire** is decided by that order object: it hands soldiers the fire behaviour. Its class was not
  found this round. The lead is the vtables whose slot `+0x30` returns a drill value 0..5. The model keeps half the
  men per volley (PLACEHOLDER).

### (3) Battle weather
- The battle files (`<weather>`) hold only the prevailing wind and the lighting.
- The preset's `weather.xml` holds `max_weather_type_key`, read by the exe at `0x00F5C528`. All 56 shipped presets
  say `dry` (survey: `target/tmp/wsurvey.sh`), and `heat_fatigue` / `cold_fatigue` are 0 in every one.

INFERRED: the preset key caps the battle's weather. So every preset battle, historical ones included, is dry,
whatever the original picks inside the cap.

`setup::apply_weather` reads the preset key and looks it up in `battle_weather_types`, now in `ntw_data` as
`BattleWeatherType`. From the row it sets:
- `Battle::weather_intensity` (visibility);
- `Battle::weather`: rain for kind 0, snow for kind 1, clear otherwise.

Waterloo and Austerlitz log "Battle weather dry: intensity 0, Clear". Campaign battles (§5) will need the
climate/season pick from `battle_climate_weather_descriptions`; that is not decoded.

### (4) The fatigue_effects columns (CONFIRMED readers)
`0x00649520` returns the unit record's `+0xE4` table entry for the unit's fatigue level `+0xC70`. That entry is null
for fresh, active and winded, which have no row. Four readers use it, at the builder offsets + 4:

| Offset | Read by | Column |
|---|---|---|
| speed `+0x10` | `0x006543D0`, multiplies the soldier speed | col 2 |
| charge `+0x14` | `0x006B30E0` | col 3 |
| control `+0x18` | `0x006B8AA0` | col 4 |
| attack `+0x1C` | `0x006A8290` | col 5 |

- The data fits this order. Galleys, the only rowed ships, have a speed value; every naval row has an attack value
  (boarding); col 4 is small (−0.05 / −0.1 / −0.15, accuracy-sized) and 0 for ships.
- INFERRED: the runtime value is `1 + the table value`, because the readers multiply by it.
- The category is `units.category`. Its land values are exactly the table's categories (infantry, cavalry,
  dragoons, artillery, elephants).

Ported:
- `ntw_data::FatigueEffect`;
- `fatigue::FatigueEffects`, with `LandUnit::fatigue_effects` per level, set by `setup::fatigue_effects_of`;
- it now scales melee attack and charge, the missile control base and the movement speed.

### Round 13 checks
- Build and `cargo test --workspace` pass (50 result lines ok). Clippy: no new warnings (`setup.rs:352`
  `deployment_area.clone()` is old).
- Full runs with scripts, speed 16, no panics. Exit code 124 is the screenshot timer, set past the end. Men are from
  the last trace line, at most 12 s before the end.
  - **Waterloo:** over at 666.1 s, **side 0 (France) won**. France 1992 → 623 men, Allies 2872 → 241.
  - **Austerlitz:** over at 734.1 s, side 0 (France) won. France 2192 → 1253, Russia/Austria 2158 → 786.
- What moved Waterloo:
  - the charge fix: 229.9 s to 638.1 s, Allies still won;
  - then the fatigue effects: tired units lose up to 50 % of attack, charge and speed and 15 % of control. That
    turned the late fight, and France won at 666.1 s.
  - Weather is dry, as before, so it changed nothing.

## 49. Round 14: who fires (partly), leaving the field, the sweep
### (1) S3: how the fire behaviour reaches the soldiers
I traced the soldier fire behaviour back to where it is created:
- The FIRE state's descriptor is `0x01346AE8`; the state objects are at `0x01454310…`. The behaviour class has
  vtable `0x01348D60`, and its constructor is `0x007E1E30`.
- `0x007E1E30` is called only from `0x005DFA90`. That function gives the fire behaviour to **every soldier** in an
  entity's soldier list (`+0x66C` count, `+0x670` list), except those that are busy. If a soldier already has a
  behaviour, it is ended first.
- `0x005DFA90` has two callers:
  - `0x005EB730`, from `0x005849E0`. This is the issuer for **mounted** units only: it is gated by `0x0055ABF0`
    (category cavalry or camels, or dragoons not dismounted) and unit `+0x1C4`. It walks the formation's members
    (formation `+0x18` count, `+0x1C` list, member vfunc `+0x98`):
    - a member gets a fire order only when none of its soldiers is busy (`0x005F2F00`);
    - one path collects the first `min(members, shape +0x10)` members and fires them as a group, with a facing.
      Shape `+0x10` is the formation shape's front width: the shape object `+0x628` is one of seven classes set by
      `0x00531680`, and the square one returns `ceil(√n)`. This path is gated by `0x0057EA80`, meaning UNKNOWN;
    - the other path fires every idle member.
  - An entity method at `0x005DD3D0`, in the vtable slot at `0x0132BB6C`. It is presumably the infantry path, but
    its caller was not found.
- The formation object `unit+0xADC` is the unit's largest sub-formation: the one with the most members, from the
  lists `+0xAD0` / `+0xAB4` (`0x0054C6D0`).

What this means for who fires (INFERRED):
- every idle soldier of a member that has a fire order fires, on his own cycle;
- the only cap found, the front width, belongs to the mounted group-volley path;
- for infantry, the member-level gate was not found.

The model keeps half the men per volley (PLACEHOLDER). Replacing it with "all men" or "front width" is not justified
by what was found.

### (2) Units leaving the field (ported, `Battle::leave_step`)
`0x005857A0`, the second half of update slot 2 (CONFIRMED):
- A unit that is leaving and has a soldier outside the playable area gets `+0xAA0` = 2, once.
  - "Leaving" is `0x0053EEE0`: its current order says so (vfunc `+0x34` of order stack `+0x398 + 0x90·i`), or
    `0x0055C480` holds while it is on the field.
  - The battle then raises its "unit left" events (battle `+0x94` → `+0x608` and `+0xAB8`).
- State 2 is read by:
  - the validity test `0x0055CBD0`, so the morale neighbour terms and fear/inspiration skip the unit;
  - the rally test `0x0055C500`;
  - `0x0053EA00` (also requires morale state ≠ 7) and `0x0055CB50`.

The model:
- a routing or shattered unit whose formation rectangle is no longer fully inside the area gets `left_field`. It is
  not active, has no orders, and keeps its men;
- inactive units were already excluded from contact, targets, morale neighbours and fear;
- the rally test now also skips departed enemies (`can_rally`);
- scripts see `leaving`;
- the battle trace CSV has a new `left` column;
- victory is unchanged, because a routing unit already counts as out of the fight.

Not modelled: leaving by order (withdraw). The meaning of `0x0055C480` (INFERRED: routing) is not decoded.

### (3) Sweep items, time-boxed
- **Formation `+0x670`:**
  - no direct store exists in the battle code;
  - formation `+0x644` is an oriented-rectangle struct (`0x0055B220`, `0x005774A0`): `[0]`, `[1]` centre,
    `[3]`, `[4]` width and depth, `[7]`, `[8]` the axis;
  - so `+0x670` is its field `[11]`, presumably a bounding radius (INFERRED).
  - The writer was not found, so `FORMATION_RADIUS` stays 0 (PROVISIONAL).
- **Type `+0x6C` (garrison cap, `0x008554F0`):**
  - the building's `+0x54` is not the `battlefield_buildings` record, whose builder ends at `0x5C`;
  - not found.

### Round 14 checks
- Build and `cargo test --workspace` pass (50 result lines ok). Clippy shows no new warnings in my files. The
  `ntw_ai` and `terrain` ones belong to other workers' merged code; `setup.rs:352` is old.
- Full runs with scripts, speed 16, no panics. Men are from the last trace row (10 s grid).
  - **Waterloo:** over at 676.1 s, side 0 (France) won. France 1992 → 618 men; Allies 2872 → 310. Five French and
    three Allied units left the field.
  - **Austerlitz:** over at 734.1 s, side 0 (France) won. France 2192 → 1253; Russia/Austria 2158 → 749. No unit
    left the field.
- Compared with round 13: Waterloo moved from 666.1 s to 676.1 s; Austerlitz is unchanged.

## 50. Round 15: the infantry fire path (time-boxed), sweep review
### (1) Who fires: two mechanisms, and the one that decides is still open
- **The `0x005DD3D0` method is not the infantry issuer.** It sits in slot `+0x1A8` of an entity vtable; I aligned
  the slots through `0x00650A90`, which is at `+0x1A4` in every soldier class. Its only virtual caller is the soldier
  update `0x0066CE00`, which calls `+0x1A8` while the soldier's `+0x628` holds an engagement object. That object is
  set by `0x00662E90` from the fight/engagement pairing code (`0x00672B80`, `0x00672F40`, `0x006735E0`,
  `0x00672580`).
- **Slot `+0x1A8` per class (CONFIRMED by the slot alignment):**
  - base soldier: `0x006D0890`, the melee behaviour;
  - the musket classes (vtables near `0x01348050`, `0x0134820C`, `0x013483C8`, `0x0132B940`): `0x00650B30`, which
    starts the **musket state machine** through `0x007E26F0` → `0x007E2720` (vtable `0x01348738`);
  - other classes: `0x00809FC0` (`0x007E2830`) and `0x005DD3D0` (the entity with a crew list).
- **The musket state machine** (state names at `0x01453D70`): START, TURN, GET_AMMO, MISFIRE, AIM, AIM_FINISHED,
  FIRE, RELOAD, plus MOVE_TO_RELOAD_LOCATION.
  - AIM plays action 0xE (0xF in mode 1) and records when the clip ends.
  - AIM_FINISHED waits a further hold of `+0x54` seconds (`0x00805920`).
  - FIRE enter is a behaviour virtual (`+0x6C`).
  - The FSM of §47, without AIM, belongs to the mounted issuer `0x005849E0` (cavalry carbines) and to crews.
- **The fire flag `+0x5C0`.** The fire order `0x00551FE0` (virtual `0x0056A610`) sets it on every soldier;
  `0x00542C20` clears it on all. The soldier update `0x006718F0` then starts behaviour `0x007E26C0` (vtable
  `0x01348704`). That behaviour only brings the soldier into the ready pose: actions 8/9 until his action is
  0xE/0xF (`0x00818C00`).

So every soldier makes ready, and the engagement pairing decides who actually fires.
- I briefly ported "every man fires" from the flag alone: commit bc8f7ad. With it, Waterloo ended at 848.7 s (Allies
  won) and Austerlitz at 588.1 s. I reverted it (cfadfc4) once the flag turned out to mean "ready".
- The half-the-men PLACEHOLDER stays.
- Next lead: who creates engagement objects for ranged fire, i.e. calls `0x00662E90` with the aim actions
  0xE/0xF/0x10 (the `0x00664B40(10)` test upgrades 0xE to 0x10).

### (2) Sweep review (S8+)
| Item | Status |
|---|---|
| S8 general died recently | The `+0x1B0/+0x1B4/+0x1B8` writers were searched exhaustively in rounds 8–10; nothing new to try without a debugger. |
| S9 formation shapes | New CONFIRMED structure. The formation's shape object `+0x628` is one of 7 classes (`0x00531680`, from unit `+0x510` → `+0x2F0..+0x2FC`). Its vfunc `+0x10` is the front width in men. Line: `ceil((W + 0.001 − a) / b) + 1` (`0x00642C90`). Square: `ceil(√n)`. The model's drawn block (files × spacing) already gives the width; nothing to change. |
| S10–S12 defences, garrisons, skirmish | Unit-level stand-ins; no new exe leads. |
| S13 bookkeeping | `LandUnit::recent_losses` is now only counted and hashed; the morale code no longer reads it. |
| S14 retiring a destroyed unit | Low impact; the unit is already inactive and shattered. |
| S15–S18 | Low or no effect on the outcome. |

No S8+ item had a decodable lead that changes play this round.

### Round 15 checks
- Build and `cargo test --workspace` pass (50 result lines ok). Clippy: no new warnings in my files.
- Full runs at HEAD (the round 14 rules; the merge of main changed nothing): Waterloo over at 676.1 s, France won
  (1992 → 618, Allies 2872 → 310). Austerlitz over at 734.1 s, France won (2192 → 1253, 2158 → 749).

## 51. Round 16: last S3 attempt (time-boxed 1 h, not found)
- Every caller of `0x00662E90` (`0x00672580`, `0x00672B80`, `0x00672F40`, `0x006735E0`) is melee choreography. They
  place two combatants and give them combat actions; action 0xE, in the "ready" group, becomes 0x10 when
  `0x00664B40(10)` holds. None of them creates a ranged engagement. The other writers of `+0x628` in the
  `0x0070`–`0x00C0` range belong to other classes.
- The state machine started through slot `+0x1A8` (vtable `0x01348738`) has a FIRE enter (`0x00805B80`) that really
  shoots: it calls the chance to hit `0x006A5CE0` and the reload time `0x00639E40`. It first finds its own target
  (`0x00805980` → `0x00808E90` / `0x0063CE30`). Its MISFIRE, AIM and MOVE_TO_RELOAD_LOCATION states fit a gun crew as
  well as a musket (UNKNOWN which).
- The question stays open: which machine the line-infantry soldier runs, and who starts it. Candidates:
  - the `0x01346xxx` FSM, started by `0x005DFA90` from the mounted issuer and the crew entity;
  - the `0x01348738` FSM, started through `+0x1A8`.

  The half-the-men PLACEHOLDER stays. A debugger breakpoint on `0x00805B80` and `0x00806AC0`, with a line-infantry
  unit firing, would settle it in minutes.
- S3 is parked here; the fidelity-battle worker moves to the 0-D slot.

## 52. Round 17: S3 who fires (SOLVED statically), probes, campaign weather pick
### (1) S3: the per-drill fire orders (CONFIRMED static)
- **Slot alignment fixed.** The soldier vtable pointer is the class vtable +0x40 from what round 15 assumed. In the
  real vtables (base `0x01333714`, musket classes `0x01347EE8`, `0x013480A4`, `0x01348260`, `0x013473D0`):
  `+0x164` = `0x00650A90` (start the ready behaviour), `+0x168` = `0x00650B30` (start the musket state machine,
  vtable `0x013487B8`, FIRE enter `0x00805B80` at its `+0x6C`), `+0x1A8` = `0x0067A050` (perform the melee engagement
  of `+0x628`). So the engagement object is melee only, as the probe saw.
- **Who calls `+0x168`:** the forwarder `0x00809F30` (this `+8` → soldier). Its callers include `0x0080A1B0`, the
  enter of the soldier's fire-order behaviour (vtable `0x01348640`, built by `0x007E3ED0` from `0x006530E0`). That
  behaviour waits until the soldier's current behaviour is finished (`0x008196E0`), then starts the musket machine
  with the order's arguments (target point, arc, mode, kneel flag, aim hold).
- **Who gives soldiers that order (`0x006530E0`, 26 call sites):** the unit's missile-attack state (`0x00553C30`)
  builds one order object per firing drill (`0x00554920`: the card drill `+0x78`, or 0 when mounted/special):
  - drill 0 `fire_volley`, vtable `0x013239F0` (its vfunc `+0x30` returns 0): fire step `0x0055EB30`. The candidates
    are the first `n` soldiers (`0x00539910`); `n` = `0x005694D0`: every man if unit `+0x1A5` (column 53, "may
    skirmish") or mounted (`0x0055ABF0`), else the formation shape's vfunc `+0x10` (files; line shape `0x00642D00`:
    `floor((W + 0.001 − a) / b) + 1`, capped at the men). Formation slot `i` is in rank `i / files` (`0x00619E60`) and
    the reform (`0x00581200`) puts soldier `i` on slot `i`, so `n` = the front rank. Loaded candidates
    (`0x0063E290`) fire once `max(1, round(0.025 × men))` are loaded, each with an aim hold drawn in [0, 0.5) s, mode 1.
    The battle also lets only the "current" unit plus 3 more start such a volley per tick (`0x0057EA80`,
    `0x00576690`; not modelled).
  - drill 1 `mass_fire`, vtable `0x013253E0` (+0x30 → 1): RELOAD_STATE (all men mode 4) ↔ FIRE_STATE (`0x0056C4C0`:
    every man, aim hold [0, 0.4) s).
  - drills 2/3/4 platoon fires (`0x0051B120`/`0x0051B180`/`0x0051B090`, +0x30 → 2/3/4): 3 groups (dispersed:
    soldier `i` in group `i mod 3`, `0x0054FF00`), fired in turn (WAIT_FOR_GROUP_ALL_LOADED_AND_READY,
    ORDER_GROUP_FIRE, WAIT_FOR_GROUP_FIRE_COMPLETED, ADVANCE_GROUP).
  - drill 5 `rank_fire` (`0x0051B240`, +0x30 → 5): groups = ranks (`0x0054FF50`), at most 3 (`0x00569530`).
  - fire and advance (state 7): `0x00519E80`, CURRENT_ROW_* states (`0x00549950`, `0x00549C20`).
  - square: `0x005541E0` → `0x005622E0`, each face fires at enemies in its quarter.
- **Arc:** each shooter is paired with a target soldier (`0x00500380`, pairs re-swapped to cut total distance,
  `0x00502C50`) and kept only if the bearing is within half the arc of the order's facing (`0x005869A0`). The arc is
  `battle_entities` column 17 in radians (`0x00E52F40` converts; infantry 70°). The order faces the target, so the
  model does not filter.
- **Port:** `shooting::volley_plan` (men per volley and volleys per reload cycle) and `effective_drill`; the reload
  formula now gets the drill (`FiringDrill::from_value`). `LandUnit::formation_files` is set by the setup from the
  drawn block. APPROXIMATION: the model fires the same men once per reload cycle (grouped drills: one group every
  reload/groups) instead of each man as he is loaded.
- Model before → after (default drill, 3 ranks): half the men → a third (front rank); light infantry and cavalry
  carbines → every man.

### (2) Probes for the manager (one session): `analysis/fidelity/debugger/f0a_battle_probe.cdb.txt` (copy in `target/tmp/probes/`)
S3 confirmation (`0x006530E0` callers and slot indices), formation rectangle dump at `0x00701310` (item 2), garrison
cap at `0x008554F0` (item 3), battle-settings constructor `0x00878060` plus a read watchpoint on the unit scale
(item 4). The formation rectangle is formation `+0x644`: centre `[0..1]`, width/depth `[3..4]` (`+0x650/+0x654`),
axes `[5..8]`; `+0x670` is `[11]`.

### (3) Campaign-battle weather pick, `0x00F5B4D0` (CONFIRMED static, not yet ported)
Inputs: climate record, season (enum SUMMER 0, WINTER 1, SPRING 2, AUTUMN 3, table `0x0145BF04`), time of day
(MORNING..EVENING), optional desired weather. Over the `battle_climate_weather_descriptions` rows it applies four
filters in turn (`0x00F5B2A0`; a filter that keeps nothing falls back): weight (col 4) > 0; climate equal; season
equal (fallback: the SUMMER rows); desired weather equal (fallback: unchanged). Then a weighted pick by col 4 with
the setup LCG (`r = rand16 / 65536 × Σweights`). The picked row gives the weather key, a flag `rand ≤ col 5 × 0.01`
(col 5: 60 torrential rain, 50 heavy, 15 light), and cols 7/8 (heat/cold fatigue, as §4.2). No rows match → the
first record.

### (4) Experience / chevrons (item 6, sandbox 0-A resolves the timers)
Sandbox 0-A (CONFIRMED, this file §2.1): `unit+0xD48` is the experience level — exposed as "Experience" by
`0x005ABF40`/`0x005CD340` (and `0x008590B0`), read by `0x0053E4D0`/`0x0053A720` (timers) and indexed into the
experience-bonus table by `0x00670F40` (`+0x20` term, PORTED in the next sandbox round with
`unit_stats_land_experience_bonuses` in `ntw_data`, §19a). The old lead (kv `relative_melee_experience_multiplier`, card builder `0x00513440`) is unchanged:
`0x00DAB5F0` (hit number) has no experience term, and that kv key has no battle consumer found. The battle file's
unit `experience` is now wired to `LandUnit::experience` (was `UnitInfo` only).

### (5) Round 17 checks
- Build passes; clippy 96 warnings, none in my files; determinism `twice campaign 3` IDENTICAL.
- `cargo test --workspace`: one failure, `ntw_campaign` `save_compat::user_saves_pass_the_checks`, on the user's own
  `auto_save.save` in the original's save folder (AI block components) — environment, not touched by this round.
- Waterloo, full run at speed 8: before 676.1 s, France won (1992 → 618, Allies 2872 → 310); after 333.9 s, the Allies
  won (France 1992 → 372 at 300 s, Allies 2872 → 1372). Cause, from the per-unit plans: French line infantry now fires
  with its front rank (about a third: 42–56 of 160) instead of half; the Allied light infantry, riflemen and jägers
  (skirmishers, column 53) and the cavalry fire with every man; British two-rank lines with 42–79 of 160.
- Austerlitz: both the old and the new build's windows closed about a minute into the battle ("No windows are open,
  exiting"), and the `--battle --screenshot` check exited 0 without saving its picture for the same reason. Not caused
  by this change (the old binary does it too); the Austerlitz length comparison is still to do.

## 53. Sandbox 0-A round N+1: unit_scale solved, the XP cost columns, the naval table, the garrison cap
Addresses worked this round (all Ghidra read-only runs against this worker's own project copy
`%USERPROFILE%\Documents\NR-sb-0a-ghidra`, scratch in `target/tmp/sb0a_c*_out.txt` and `sb0a_d*_out.txt`):
`0x004A6540` `0x00DAFBB0` `0x00DAFBC0` `0x004A6600` `0x004A6C90` `0x005363C0` `0x00513320` `0x00513440`
`0x00513A10` `0x004B4AC0` `0x00572200` `0x004765F0` `0x0045CB50` `0x0045D170` `0x004C2770` `0x005CD340`
`0x00ED49A0` `0x00E31710` `0x00E31490` `0x00404230` `0x00454300` `0x008554F0` `0x00688DD0` `0x0052F3A0`.

### (1) `unit_scale`: SOLVED, ported — see §18a
The scale multiplies the army unit card's u16 men at `+0xA` (`0x004A67C5` `MULSS` `0x004A67E5`,
`CVTTSS2SI` = truncation) and the product becomes card `+0xC8`/`+0xCC`, the field the battle reads as the men.
Ship cards use an unscaled byte at `+0x0E`; battle-file units are unscaled (§39). The setting is
`gfx_unit_scale` (int 0..3, **exe default 2 = 0.75**, "Set unit scale. 0 - lowest, 3 - ultra",
`0x00404230`), which the player has set to 2 in `preferences.script.txt`.

### (2) The `+0x24`/`+0x28` pair is the XP-adjusted COST — see §19a
Not a fatigue term: `0x005CD340` labels the call's result `"XpAdjustedCost"` and `0x0045CB50` spends it
against a budget. Ported as `GameDatabase::experience_adjusted_cost` / `naval_experience_adjusted_cost`.

### (3) `unit_stats_naval_experience_bonuses`: DECODED — see §19a
Getter `0x00E31710` (own "Loading database: %s" string + the `0x00E86C80` name getter that DB_BUILDERS.md
lists). v0, 10 rows, ranks "0".."9", **no leftover bytes** → the 7-column layout is CONFIRMED. In `ntw_data`.

### (4) Garrison cap `+0x6C`: still UNKNOWN, but the record is narrowed
`0x008554F0` = `min(slots via 0x006F22C0, (*(*(garrison+4)+0x54)) + 0x6C)` when `*(garrison+4)+0x1E8 != 0`
(unchanged, §42). New this round:
- `garrison+4` is set by the battle building setup `0x00688DD0`: `MOV [EBX+4],EAX` at `0x00688E1F`, where
  `EDI = [[building+0x48]+8]+0xDC` and `EAX = [EDI+0x14]` — so `garrison+4` is the **building type object**,
  and `+0x1E8` / `+0x54` are its fields.
- `building+0x54` is set at `0x00688E5B` (`MOV [EBX+0x54],EDI`) from the **5th argument** of `0x00688DD0`, and
  the only caller, `0x0052F3A0`, passes the row of **`battlefield_buildings`** (its `record_index` error string
  names `BATTLEFIELD_BUILDING_RECORD`). So the record behind `building+0x54` is a battlefield-buildings row.
- **Negative result (new):** the cap is NOT read through `building+0x54` (the cap chain goes through
  `garrison+4` → `+0x54`), and **no column of any building table sits at `+0x6C`**: `battlefield_buildings`
  ends at `0x58`, `building_levels` at `0x84` but `+0x6C` is inside its column 17 string slot, and
  `battle_city_buildings` / `battlefield_building_categories` / `battlefield_building_transformations` /
  `building_chains` are smaller still. There is also no `garrison` string anywhere in the exe (only
  `CCQ_SABOTAGE_GARRISON` / `CCQ_CAMPAIGN_EDIT_MODE_JOIN_GARRISON` UI ids), so the cap is a **runtime field of
  the building type object**, not a DB column — which is why three rounds of "find the column" failed.
  Next lead: read `[type+0x1E8]` and `[type+0x54]`'s type instead (find the building type object's own setup /
  vtable from `0x00688DD0`'s `[EBX]=0x1321294` → `0x1338CD0`), or a read watchpoint on `type+0x6C`.
  Not ported; `garrison::BattleBuilding` keeps the model-derived slot count only.

### (5) Formation `+0x670`: UNKNOWN, unchanged
No write-watchpoint debugger is available in this sandbox, so the writer was not hunted again. `FORMATION_RADIUS
= 0` stays and no value was invented.

### (6) Checks
- `cargo test -p ntw_data -p ntw_sim -p ntw_ai` → **369 passed, 0 failed** (12 suites), plus
  `cargo test -p ntw_data -p ntw_script -- --ignored` → 15 passed against the real install
  (read-only), including the new `naval_experience_bonus_table` and `loads_every_table_exactly`.
- `crates/napoleon` was **not** built (the sandbox forbids `-p napoleon`): `SetupData::unit_scale`,
  `unit_scale_setting()` and the `make_unit` change in `crates/napoleon/src/battle/setup.rs` are unverified
  for compilation. Everything else is in `ntw_sim` / `ntw_data` and is covered by tests.

### (7) Still UNKNOWN after this round
1. Which preference/mechanism fills the battle-settings map key `0x0B`, and what sets the per-unit size class
   (army unit entry `+0x00`) that modes 2/4 take the minimum of (§18a). INFERRED `gfx_unit_scale`.
2. The `+0x1E8` / `+0x54` / `+0x6C` fields of the building type object, i.e. the garrison cap's source (§53 (4)).
   **§54 (3) corrects the object chain of item 2.**
3. The identity of the land table's `+0x0C..+0x1C` and the naval table's `+0x10..+0x18` columns (no reader).
4. Formation `+0x670` (needs the debugger).
5. Whether the XP-adjusted cost is ever charged anywhere else (only the panel and the auto-build were found).

## 54. Sandbox 0-A round N+2: the XP-adjusted cost wired in, the scale steps tested, the garrison chain corrected
A wiring-and-tests round: no new CONFIRMED behaviour beyond what §19a/§18a already established, except the
exact `param_3` selection of `0x00ED49A0` (re-read on the decompilation) and the garrison cap's object chain,
which §53 (4) had wrong. Addresses worked: Ghidra read-only runs against this worker's own project copy
`%USERPROFILE%\Documents\NR-sb-0a-ghidra`, evidence `analysis/fidelity/ghidra_evidence/0a/0a_garr{2,3,4,5}_*`:
`0x00688DD0` (listing `0x00688DE0..0x00688EA0`) `0x005784C0` `0x00E691E0` `0x00E69D50` `0x00E69FB0`
`0x00E4F4D0` `0x00E55EB0` `0x00E6A030` (scalar-`0x6C` sweeps).

### (1) `XpAdjustedCost` wired into the campaign economy (CONFIRMED structure, ported)
`0x00ED49A0(unit_type, rank, which)` = `row.flat + ROUND(base × row.mult)`, where `base` is the unit type's
`+0x2C` (`which == 0`) or `+0x30` (any other value) — read straight off the decompilation:
```
if (param_3 == '\0') { iVar7 = *(int *)(in_ECX + 0x2c); } else { iVar7 = *(int *)(in_ECX + 0x30); }
if (*(int *)(in_ECX + 0xa0) == 0) { ... UNIT_STATS_LAND_EXPERIENCE_BONUS_RECORD ... row+0x24 / row+0x28 }
else                              { ... UNIT_STATS_NAVAL_EXPERIENCE_BONUS_RECORD ... row+0x1C / row+0x20 }
return row.flat + (int)ROUND((float)base * row.mult);      // no row -> the caller's own else branch
```
The rank is turned into a key string (`FUN_00453d40`) and looked up by `record_index`, so it is a **key**
lookup, not the row position the fatigue path uses.

Ported into the simulation (no Ghidra needed):
- `campaign::rules::XpCostRow { flat, mult }` with `adjust(base)` and `XpCostTables { land, naval }` with
  `adjust_cost(naval, rank, base)` — the naval branch on `is_naval`, the exe's `+0xA0` flag; a rank with no
  row returns `base` unchanged, exactly as the exe's else branch.
- `CampaignRules::xp_cost` + `CampaignRules::xp_adjusted_cost(naval, rank, base)`. Default = **empty**, so
  an unfilled rules set is bit-for-bit the old behaviour (test `xp_costs_are_off_without_the_tables`).
- `economy::recruit_cost(rules, unit, experience)` — called by `CampaignModel::recruit`, so the treasury
  payment, the `RecruitmentItem::cost` (and therefore the cancellation refund) and the
  `InsufficientFunds { needed }` figure all carry the adjusted cost.
- `economy::unit_upkeep_with_experience(rules, fx, faction, unit, experience)` — `unit_upkeep` then the same
  call on the upkeep base; called by `faction_upkeep_with`. INFERRED that upkeep is charged the adjusted cost
  (§53 (7) item 5 is still open: only the panel and the auto-build were found calling `0x00ED49A0`).
- `GameDatabase::experience_cost_rows()` / `naval_experience_cost_rows()` return `(rank, flat, mult)`
  triples — the seam for the loader (§54 (2)).
- Tests: `veteran_units_cost_more_than_recruits` (rank 9 land 1310 vs rank 0 500 on the 500-cost test unit;
  naval 980; every missing rank unchanged; upkeep 379 vs 10; `ROUND` half away from zero on ±10 × 1.45),
  `recruiting_charges_the_xp_adjusted_cost` (treasury, queue item, refusal figure, refund),
  `faction_upkeep_is_the_sum_over_its_units`, `xp_costs_are_off_without_the_tables`.

**Two seams are deliberately left for the file owners (each one line, no behaviour change until then):**
1. `ntw_campaign::rules_from_db` should copy `db.experience_cost_rows()` / `db.naval_experience_cost_rows()`
   into `r.xp_cost.land` / `r.xp_cost.naval` (it is 0-B's file; `CampaignRules` is built there with
   `..Default::default()`, so the field is additive).
2. The campaign model has **no per-unit experience**: `CampaignUnit` has no chevron field and the ESF `UNIT`
   record's index for it is UNKNOWN. `economy::unit_experience(_)` therefore returns 0 for every unit
   (PROVISIONAL), and `recruit` passes rank 0. The battle side's `LandUnit::experience` is unrelated.
Also for whoever owns the recruitment panel: `ntw_script/src/ui/campaign.rs:545` still reads `u.cost` /
`u.upkeep` for the displayed figures, where the exe shows "XpAdjustedCost" beside "RecruitCost"
(`0x005CD340`) — it should show `economy::recruit_cost(&m.rules, u, rank)` once a rank exists.

### (2) `gfx_unit_scale`: the four steps and the exe's default, tested
CONFIRMED and now a named constant: `unit_scale::PREFERENCE_DEFAULT = 2` (the preference's registered
default at `0x00404230`, storage `0x0149D880`, help `"Set unit scale. 0 - lowest, 3 - ultra"`; the player's
`preferences.script.txt` ships `gfx_unit_scale 2`), i.e. the out-of-the-box battle thins units to
`STEPS[2] = 0.75`. Tests `the_four_settings_scale_the_men` (0/1/2/3 → 40/80/120/160 men of 160) and
`default_setting_is_three_quarters`, which also make the `0x004A67EB` truncation explicit: **7 × 0.75 = 5**
(5.25 truncated, never 6) and 158 × 0.75 = 118. The exe's per-mode minimum for battle modes 2 and 4 (the
smallest per-unit size class byte over the army's units) is **still UNKNOWN**, as is what fills battle-settings
key `0x0B`; only the four-step table and the preference default are tested.

### (3) Garrison cap `+0x6C`: the object chain of §53 (4) was wrong (CONFIRMED correction, still no value)
The disassembly of `0x00688DD0` (evidence `0a_garr2_out.txt`) separates the three objects the earlier note ran
together — EBX is the **battle building instance**:
```
00688e0e  MOV EDI,[EAX + 0xdc]          ; EDI = [[instance+0x48]+8]+0xDC = the building TYPE object
00688e16  MOV [EBX],0x1321294           ; base vtable, then 0x1338cd0 at 0x00688e4a (derived)
00688e1c  MOV EAX,[EDI + 0x14]
00688e1f  MOV [EBX + 0x4],EAX           ; instance+4 = type+0x14
00688e26  LEA ESI,[EDI + 0x14]
00688e2b  CALL 0x005784c0               ; get-or-create on that slot block
00688e30  INC [ESI]                     ; ... and bump its counter
00688e37  MOV [EBX + 0x8],EDI           ; instance+8 = the type object
00688e5b  MOV [EBX + 0x54],EDI          ; instance+0x54 = param_5, the battlefield_buildings row
```
So `type+0x14` is **not** a plain int field: `0x005784C0` is a hash-map insert called with the map in
`in_ECX = &type[0x14]` (its body walks `*in_ECX` as an 8-bytes-per-entry bucket array, `in_ECX[1]` as the
bucket count, `in_ECX[2]`/`in_ECX[3]` as sentinels), so the building type carries an inline map at `+0x14` and
`[EBX+4]` is that map's first dword. The cap chain of `0x008554F0` is `T = [instance+4]`, `P = [T+0x54]`,
`cap = [P+0x6C]`, and **`[T+0x54]` is a different field on a different object from the `[EBX+0x54]` store
above** (that one is the building's `battlefield_buildings` row pointer and is not on the cap's chain).
That is the concrete reason three rounds of "find the column" failed: two unrelated `+0x54` fields were read
as one. INFERRED, from the 8-byte bucket stride: `T` is the map's bucket array, `T+0x1E8` is entry 61's key
(the "is this type set up" test) and `P = T+0x54` is entry 10's *value* pointer, whose `+0x6C` is the cap.
Two dead ends closed this round, both recorded so nobody repeats them:
- `0x00E69FB0` (an "inherit if unset" merge: `if (*(char *)(in_ECX + 0x70) == 0) { [in_ECX+0x6C] = [param_1+0x6C]; ... }`,
  with the same flag/value pairs at +0x64/+0x68, +0x74/+0x78, +0x7C..+0x88, +0x8C..+0x98, +0x9C..+0xA8) looks
  exactly like an optional `+0x6C` column, but its caller `0x00E4F4D0` builds the
  FIRE_POSITION / FUSE_POSITION / IMPACT_POSITION / DISTANCE block of a **projectile** record. Not it.
- The only `+0x6C` writes in the building-type builder `0x00E6A030` (called by `0x00E55EB0`, which
  `0x00688DD0` calls with the row's `+0x5C`) are a 16-byte struct copy (`MOVUPS [ESI+0x40]` → `[EBX+0x6C]`)
  and a zeroing initialiser (`MOV [EBX+0x6C],0`). Not a scalar cap either.
- `0x00E691E0` (the `battlefield_buildings` row callback) is a 14-byte forward to the row reader
  `0x00E54010`, which fills only the 8 file columns (last one at builder `+0x58`) — so the cap is **not** a
  column of that table, confirming §53 (4)'s conclusion by a different route.
Next lead: the object the map entry 10 of `[type+0x14]` points at — i.e. who inserts into a building type's
`+0x14` map and what that object is; a read watchpoint on its `+0x6C` at battle setup would settle it in one
run. No port: `garrison::BattleBuilding` keeps the model-derived slot count only.

### (4) Formation `+0x670`: UNKNOWN, unchanged
No debugger in this sandbox. `FORMATION_RADIUS = 0` stays; **no value was invented**.

### (5) Checks
- `cargo test -p ntw_data -p ntw_sim -p ntw_ai` → **376 passed, 0 failed**, 18 ignored (ntw_sim 294,
  ntw_ai 63, ntw_data 18 + 1 doc-test).
- `cargo test -p ntw_data -- --ignored` → **14 passed** against the real install (read-only), including the
  rank sweep 0..9 over both experience tables (`experience_cost_rows()` has 10 rows, ranks in order, cost
  rising with rank and untouched outside 0..9).
- `cargo clippy -p ntw_sim -p ntw_data --all-targets` adds no warning in the files this round touched (the
  one hit in `campaign/tests.rs` is the pre-existing `CaRng` clone at line 2343).
- `crates/napoleon` was **not** built (the sandbox forbids `-p napoleon`), so `SetupData::unit_scale`,
  `unit_scale_setting()` and the `make_unit` change from round N+1 remain unverified for compilation.
  Nothing this round touched `crates/napoleon`.

## 55. Sandbox 0-A round N+3: the XP seams closed, the UI price wired, the garrison chain re-derived
A wiring-and-correction round. The two seams §54 (1) left are gone (`rules_from_db` now copies both
experience tables; `CampaignUnit` has an `experience` field and a setter), the recruitment card's
price goes through `economy::recruit_cost`, and §54 (3)'s garrison reading is **wrong** — the chain
re-reads cleanly off the disassembly this time. Addresses worked (Ghidra read-only runs against
this worker's own project copy `%USERPROFILE%\Documents\NR-sb-0a-ghidra`, evidence
`analysis/fidelity/ghidra_evidence/0a/sb0a_g{6,7,8,9,a,b}_*`, scratch in `target/tmp/sb0a_g*`):
`0x008554F0` `0x008554E0` `0x00855560` `0x00855370` `0x00688DD0` `0x005784C0` `0x0052F3A0`
`0x00E11360` `0x00E4E8E0` `0x00E691E0` `0x00E54010` `0x0069AD80` `0x0069D050` and the vtables
`0x01338CD0` / `0x01321294`.

### (1) The XP seams are closed — the feature is reachable (ported)
Both one-line seams of §54 (1) are landed, and the tests prove the chain end to end.
- **`ntw_campaign::rules_from_db`** now fills `r.xp_cost` from `db.experience_cost_rows()` /
  `db.naval_experience_cost_rows()` (the `+0x24`/`+0x28` and `+0x1C`/`+0x20` pairs). It is the only
  producer of `CampaignRules` for a loaded campaign — `read_esf` calls it (`0x00E11360`-style table
  copy at `ntw_campaign/src/lib.rs:369`) — so **every campaign and save now has live XP costs**,
  where before the tables were empty and no number moved.
- **`CampaignUnit::experience`** (u8, the exe's `unit+0xD48`) plus `World::unit_experience` /
  `World::set_unit_experience` (clamped to 0..=9, the range the rank tables cover). `economy::
  unit_experience` is now a real read of that field instead of a hard-coded 0 — **the upkeep path
  picks up a rank the moment anything sets one.** Every constructor passes `experience: 0`
  (`pool.rs`, `commands.rs`, and the `read_unit` ESF reader).
- **The ESF index was NOT invented.** Which `UNIT` v3 index carries the chevron count is still
  UNKNOWN, so `ntw_campaign::world::read_unit` leaves it 0 with the reason in a comment, and there
  is no XP gain modelled anywhere. So **no loaded unit is above rank 0 yet**: the field, the setter
  and the loader seam are live, the *data* is not. That is the honest state, and it is why the new
  tests drive the rank through `economy::recruit_cost` rather than through a loaded save.
- New tests (`ntw_campaign`, 2): `the_loader_puts_the_experience_tables_in_the_recruitment_cost`
  (fixture DB + an injected rank 5, since the fixture ships only ranks 0 and 9 → both tables in
  `rules.xp_cost` keyed by rank, and rank 5 = 180 + ROUND(111 × 1.5) = 347 > rank 0's 111) and
  `recruiting_from_a_loaded_campaign_charges_the_loaded_experience_cost` (the same DB through
  `read()` on the tiny start position, so `CampaignModel::recruit` really charges the loaded
  tables' figure out of the treasury and into the queue item; a non-trivial rank-0 row is then
  charged too, 100 + 2 × 111 = 322).
- `ntw_sim::campaign::world::unit_experience_is_read_and_set_by_id` covers the getter/setter.

### (2) The recruitment card's price: wired (`ntw_script`, note for 0-E)
`ntw_script/src/ui/campaign.rs`, `recruitment_info`, the `rules` map that used to read `u.cost`:
```
.map(|k| m.rules.units.get(k).map_or((0, 0, 1), |u| (economy::recruit_cost(&m.rules, u, 0), u.upkeep, u.turns)))
```
So the displayed `cost` is the value the treasury is charged (§19a: the exe shows "XpAdjustedCost"
next to "RecruitCost", `0x005CD340`) rather than the bare `units` #4 cost, and the two can never
drift. At rank 0 (a unit being raised afresh) it is the same number as before — the change is that
the *path* is now the real one. `upkeep` deliberately stays the unit type's own `UpkeepCost`
(`card+0x3C`), which is what the panel prints. Also `unit_entry` now sets `Experience` from
`u.experience` instead of a hard 0 (still 0 for every loaded unit, per (1)).
**Kept surgical for 0-E, who is live in `ntw_script` this round: the diff is those two expressions
plus the `economy` import and two comment lines. `cargo build -p ntw_script --all-targets` is clean.**
Note the file's doc comment at the `recruitment_info` heading was corrected from "PROVISIONAL:
experience 0" to say the entries are fresh recruits.

### (3) Garrison cap `+0x6C`: §54 (3)'s chain was wrong; the real one is a record field
`0x008554F0` re-read (CONFIRMED, three sources agree): `cap = min(0x006F22C0(garrison), [ [ [garrison+4] +0x54] +0x6C ])`, 0 unless `[garrison+4]+0x1E8 != 0`.

**Correction to §54 (3) (which had followed Ghidra's `in_ECX` reading of `0x005784C0`):** that
function takes the map as its **first stack parameter**, not in ECX — `FUN_005784c0(piVar23, &param_1)`
with `piVar23 = (int *)(iVar22 + 0x14)`. So `&type[0x14]` is the map **object** (its own header,
`[+0]` buckets, `[+4]` bucket count, `[+8]`/`[+C]` sentinels), and `instance+4 = [type+0x14]` is a
pointer to that header — **not** a bucket array, and not on the cap's chain at all. Concretely:
- `garrison+4` = **the battle building instance** (`0x254` bytes, allocated by `0x0052F3A0`). CONFIRMED
  by the garrison's own constructor `0x00855370` (`in_ECX[1] = param_1`, called as
  `FUN_00855370(in_ECX)` from `0x00688DD0`), and cross-checked because `garrison+0x3C` is the
  per-slot counter array `0x00855370` allocates and `0x008554E0` indexes.
- `[instance+0x54]` = the **`BATTLEFIELD_BUILDING_RECORD`** row — `MOV [EBX+0x54],EDI` at
  `0x00688e5b` from `0x00688DD0`'s 5th argument, which `0x0052F3A0` resolves through the table whose
  own string is `"battlefield_buildings_table"` (`0x00E11360`). CONFIRMED, and it is a *different*
  field from the instance's `+0x1E8`.
- `[instance+0x1E8]` = a container of **0x58-stride entries** (`{count at +8, array at +0xC}`,
  freed in this class's destructor `0x0069AD80`) — the "this building has slots" object. The
  `!= 0` guard is its presence test.
- ⇒ **`cap = [ battlefield_buildings_record + 0x6C ]`.** The row reader `0x00E54010` fills only the
  8 file columns (last at `+0x58`), and `0x00688DD0` already reads the record's runtime tail
  `+0x5C` (the sub-record it hands to `0x00E55EB0`) and `+0x64` (a string pointer). So `+0x6C` sits
  in that same runtime cluster and **is a field of the record**, three rounds after "find the
  column" failed — because it is not a column. (One intermediate reading is retracted here: the
  `FUN_010d3140(0x5c)` in `0x00E11360` sizes a helper object, not the record; the records come from
  the generic builder `FUN_00E4E8E0` → `FUN_00E3A9E0`, whose size argument I did not read.)
- Next lead, now sharp: the writer of `record+0x6C` after load. The `+0x6C` **store** sweep over the
  battle/building region (0x00500000..0x00900000) is dominated by `[ESP+0x6C]` stack slots; the
  few register-based ones are in unrelated classes. `FUN_00E4EBE0` (writes `[EBX+0x6C] = 1` then
  `= ECX`, 1482 bytes, one caller `0x00E67F30`) is a record post-processor of some kind but is a
  different record (it builds with `FUN_00E55EB0` and fields up to `+0x70`) — not yet identified as
  the building one. No port: `garrison::BattleBuilding` keeps the model-derived slot count only.

### (4) The vtable-dispatch technique, applied (0-C's trick): worked as a *technique*, found nothing
`[instance]` is a vtable pointer: `0x01321294` at `0x00688e16`, replaced by **`0x01338CD0`** at
`0x00688e4a`, with `instance+0xC = 0x01338CD8` (= vtable slot 2, the derived-class trick).
- `0x01338CD0` has **28 slots, of which 19 are the 3-byte `return 0` stub `0x00461180`**. The five
  with a real body are `0x0069D050` (→ `0x0069AD80`, this class's destructor), `0x006DCAE0`,
  `0x006DCA10`, `0x00550340`, `0x004ABFE0`. **All five have 0 direct callers**, exactly the
  vtable-only signature 0-C found — so the census of the *vtable* does reach what a census of the
  functions cannot. **None of them touches the cap** (they are the destructor and small
  create/destroy hooks).
- Dead end recorded: `0x01321294` is a **shared base-class vtable** (62 references from ~60
  unrelated constructors, e.g. `0x0098F880`, `0x00AEC210`, `0x00504F00`), not a building-specific
  one. Following it would have been a whole-round mistake.
- Net: the technique is confirmed reusable and is now the right first move for "this function has no
  callers" — but this particular vtable is not the garrison cap's route.

### (5) Per-mode unit minimum (battle modes 2/4): still UNKNOWN, untouched
Not attempted; it stays below items 1–3 in priority and the round's budget went to (1)–(3). Still
UNKNOWN: what fills battle-settings map key `0x0B`, and what sets the per-unit size class byte
(army unit entry `+0x00`) that modes 2/4 take the minimum of (§18a). Only the four-step table and
the preference default (`unit_scale::PREFERENCE_DEFAULT = 2` = 0.75) are tested.

### (6) Formation `+0x670`: UNKNOWN, unchanged
No debugger in this sandbox and this round's RE budget went to (3). `FORMATION_RADIUS = 0` stays;
**no value was invented**.

### (7) Still UNKNOWN after this round
1. Who writes `battlefield_buildings_record+0x6C`, and therefore the cap's value (§55 (3)).
2. The ESF `UNIT` v3 index for the chevron count, i.e. why every loaded unit is rank 0 (§55 (1)).
   Nothing else blocks the XP costs from moving a real number.
3. Whether the XP-adjusted cost is charged anywhere else (only the panel and the auto-build found).
4. Key `0x0B` and the per-mode minimum for battle modes 2/4 (§55 (5)).
5. Formation `+0x670` (needs the debugger).

### (8) Checks
- `cargo test -p ntw_data -p ntw_sim -p ntw_ai` → **377 passed, 0 failed**, 18 ignored (12 suites).
  Up one test from §54's 376: `world::unit_experience_is_read_and_set_by_id`.
- `cargo test -p ntw_campaign` → **96 passed, 0 failed**, 0 ignored (17 suites) — includes the two new
  loader-seam tests.
- `cargo test -p ntw_data -p ntw_campaign -p ntw_script -- --ignored` → **15 passed** against the
  real install (read-only).
- `cargo build -p ntw_script --all-targets` → clean (§55 (2)).
- `cargo clippy -p ntw_sim -p ntw_data -p ntw_campaign -p ntw_script --all-targets` adds **no warning
  in any file this round touched** (the one hit in `ntw_sim/campaign/tests.rs` is the pre-existing
  `CaRng` clone at line 2343, and `ntw_campaign/tests.rs:471-473` is the pre-existing `bad_field_type_
  is_an_error` `if let` chain).
- **`crates/napoleon` was not built** (the sandbox forbids `-p napoleon`, Bevy crash), so its state
  is unchanged and still unverified-for-compilation from round N+1. **Nothing this round touched
  `crates/napoleon`.**

## 56. Sandbox 0-A round N+4: the XP writer sweep (clean negative), `unit_scale` end to end, garrison chain closed
Base merged first: `git merge --no-edit sandbox/main` into `2173038` was a **clean fast-forward** to
`9b954e6` (710 tests) — no conflicts, nothing to resolve, `tools/resolve_additive_conflicts.ps1` not
needed and not read. A wiring/tests round plus two time-boxed RE hunts.

### (1) Chevrons are never gained — and now we know *why* we cannot find out statically (CONFIRMED negative)
`scal:0xD48:0x00400000:0x01000000` (evidence `0a/sb0a_xp1_*`) is the whole-exe sweep for the
experience byte `unit+0xD48`, and it returns **12 hits, every one of them a `MOVZX … byte ptr
[reg + 0xd48]` read**. There is **not one store to `+0xD48` anywhere in `0x00400000..0x01000000`.**
That is the headline: the exe's battle-unit experience byte is never written by a direct store, so
neither the ESF index (§54 (1), still UNKNOWN, **still not invented**) nor a post-battle chevron gain
can be found by looking for `mov byte ptr [eax+0xD48], …`.
- The eleven readers are all the CONFIRMED set plus four new names: `0x0053A720` and `0x0053E4D0`
  (the morale timers, §52 (4)), `0x0057F070` (the unit-averages tick, BATTLE_FIDELITY.md §307), and
  `0x005ABF40` / `0x005CD340` (the "Experience" labels). New: `0x005BDCE0` (97 bytes, **0 callers**
  and 0 callees — a vtable-only function, 0-C's signature), `0x00760A20`, `0x008590B0` (already
  noted at §54) and two instructions in no analysed function at all (`0x006C47D3`, `0x006C47FD`).
- **The twelfth hit is a dead end, now identified: `0x008762CB  LEA EAX,[ECX + 0xd48]` is in
  `0x008751E0`, which has ZERO callees** — a pure constructor, called only by `0x00872550` and
  `0x008742C0` (campaign setup / campaign load). The surrounding listing is four embedded empty
  containers at stride `0x18` (`&+0xD30`, `&+0xD48`, `&+0xD60`, `&+0xD78`, each with its
  begin/end pair and a zeroed head) followed by four vtable stores at `+0x0/0x18/0x30/0x48`. So
  `+0xD48` **there is a container's sentinel node, not a unit's byte** — and CAMPAIGN_FIDELITY.md
  §891 already names `0x008751E0` as a **campaign-model** constructor (`+0x50C`). Two classes, one
  offset; the scalar sweep cannot tell them apart. Worth remembering before any other `scal:` sweep
  is trusted on its own.
- The one candidate *setter* is also closed: `str:Experience` @`0x01314E84` has three referents —
  `0x005ABBD0`, `0x005ABF40` (both battle display) and **`0x009AA5E0`**, the only campaign-region
  one. Its listing (`0a/sb0a_xp4_out.txt`) shows it is a **script getter registration table**, not a
  setter: `PUSH "Name"`, `"RegimentName"`, `"CommandersName"`, `"Men"`, `"MenAsPercent"`, `"Guns"`,
  **`"Experience"`**, `"Firepower"`, … `"RecruitCost"`, `"UpkeepCost"`, `"ShowAsCharacter"`,
  `"InTransit"`, … `"EstimatedMenAsUnary"`. It is the unit-info script interface — the reader
  `0x005CD340` already gives us. It never sets anything.
- `str:army_experience` / `str:navy_experience`: **no functions found**. The mission-reward chevron
  field the data already carries (`CampaignMission::reward_experience`, `details.rs` field
  `reward_experience`, from `m.rewards.army_experience` / `navy_experience`) is consumed by the
  **generic data-driven reward system**, not by any name comparison — so it cannot be traced by
  string either, and it is NOT applied anywhere in the model.
- **Verdict, unchanged and now bounded: no XP gain path is modelled, and none can be ported without
  inventing data.** The two things that would settle it: (a) the ESF `UNIT` v3 index — needs a
  debugger write-watchpoint on `unit+0xD48` during a save-game load, or a save file with a unit
  above rank 0 to diff against a rank-0 one; (b) the post-battle gain — needs a debugger
  watchpoint across a campaign battle's result handler. Statically both are now closed out: there is
  no store instruction to find. **Nothing invented; `CampaignUnit::experience` stays 0 from loaders.**

### (2) The integration test item 2 asked for was already in place; verified, not duplicated
`ntw_campaign/src/tests.rs::{the_loader_puts_the_experience_tables_in_the_recruitment_cost,
recruiting_from_a_loaded_campaign_charges_the_loaded_experience_cost}` (round N+3, `0b`-owned file,
**not touched this round**) already do exactly what was asked: a real start position through `read()`,
the loaded tables in `rules.xp_cost`, and **`CampaignModel::recruit` moving the treasury** —
`assert_eq!(m.world.factions[&FactionId(1000)].treasury, 10_000 - 111 - 322)` — plus the queue item's
`cost` and an `InsufficientFunds` refusal. They pass unchanged. One honest gap, recorded rather than
papered over: **`recruit` always raises a fresh rank-0 unit** (`commands.rs:1060`,
`recruit_cost(&self.rules, &unit, 0)`), which is what the exe does — the rank-aware buyer is the army
auto-build `0x0045CB50`, and the model has no auto-build command. So "a unit at rank N costs more
than rank 0 through `CampaignModel::recruit`" is not expressible today, and adding a rank-N recruit
variant would mean inventing a command the campaign does not expose. Left alone.

### (3) `unit_scale` end to end, in a crate that builds (item 3, the real deliverable)
`crates/napoleon` still cannot be compiled here, so the coverage moved **into `ntw_sim`** — the
preference→scale→men decision is now a tested function in the crate that owns the model:
- `battle::unit_scale::scale_for_setting(Option<i32>) -> f32` and `men_for_setting(i32,
  Option<i32>) -> u32`: the whole preference-index → step-table → clamp → truncation chain in one
  call. `None` = "no preferences file" = the model's 1.0 (deliberate, §18a).
- `Battle::set_unit_scale_setting(Option<i32>) -> f32` beside `set_unit_scale_step`.
- **Tests** (`battle::unit_scale::tests`): `the_preference_index_becomes_the_scale_and_then_the_men`
  and `a_battle_at_the_preference_default_really_is_thinned` — the second builds **real `Battle`s**
  from unit cards and asserts the 0.75 factor (`gfx_unit_scale 2`), the **7 → 5 truncation**, the
  whole four-step table (40/80/120/160) and the `None` case, and then that the thinned men reach
  something the battle *acts on*: `side_strengths` (`0x00539E80`'s alliance figure, built from the
  men) is strictly lower at 0.75 than at 1.0, per side. So the option moves a number the battle
  computes, not just a field nobody reads. This is the coverage the uncompilable crate could not give.
- `crates/napoleon` touched in exactly two places, both made **thinner**: `unit_scale_setting()` now
  calls `scale_for_setting` (the step table, clamp and out-of-range rule leave this crate), and
  `make_unit`'s fallback moved from `STEPS[3]` to the named `MAX`. Net −2 lines of logic, +2 of
  comment. **Still unverified for compilation** — as every `napoleon` change since round N+1.

### (4) Garrison cap `+0x6C`: both fresh ideas exhausted, clean bounded negative
Time-boxed and closed. Idea (b) is now fully answered and is a useful negative in itself:
`0x0052F3A0` (259 bytes, the **only** caller of `0x00688DD0`) resolves its 5th argument through
`DATABASE_TABLE<EMPIREUTILITY::BATTLEFIELD_BUILDING_RECORD, …>::record_index` — the **table's own
row pointer**, not a copy, and it passes it straight through (its own error string even names the
type and `record_index`). So nothing in the construction path copies a cap into the record.
Idea (a) is negative too: a listing of `0x00688DD0`'s whole body (`0x00688DD0..0x00689610`, 2161
bytes, `0a/sb0a_xp5_out.txt`) contains **zero instructions touching `+0x6C`** — consistent with §55
(3)'s finding that it reads only `+0x5C` and `+0x64` of the record's runtime tail. The battle-file
load path is now identified end to end as `0x00506C00 → 0x00515340 (2394 bytes, 31 callees) →
0x00546A10 → 0x0052F3A0 → 0x00688DD0`, and a `scal:0x6c` sweep of `0x00510000..0x00560000` over it
yields only `[ESP+0x6c]` stack slots and `LEA`s into other classes' layouts — **no `BATTLEFIELD_BUILDING_RECORD`
cap write**. So `+0x6C` is written, if at all, by the **generic row builder** `0x00E4E8E0` →
`0x00E3A9E0` (§55 (3) flagged it as unread), which is the only lead left and needs its **size
argument** read to tell whether the record is even big enough to have a `+0x6C`. `garrison::
BattleBuilding` keeps the model-derived slot count; no value invented.

### (5) Formation `+0x670`: UNKNOWN, untouched
No debugger here and this round's RE budget went to (1) and (4). `FORMATION_RADIUS = 0` stands and
**no value was invented**.

### (6) What is live vs open
Live and tested: the XP-adjusted recruitment and upkeep costs (loader seam, `recruit_cost`,
`unit_upkeep_with_experience`, the card's price in `ntw_script`), the `gfx_unit_scale` option
end to end in `ntw_sim`, `CampaignUnit::experience` as a settable field.
Open, all UNKNOWN, none invented:
1. The ESF `UNIT` v3 index for the chevron count — and, new in (1), **the whole write path**: no
   store instruction exists, so this needs a debugger, not more static search.
2. The post-battle chevron gain rule (`battle result → campaign unit`). Same reason.
3. `CampaignMission::reward_experience` (army / navy chevrons) is **parsed but never applied**; its
   consumer is the generic reward system and could not be traced by name.
4. Who writes `battlefield_buildings_record + 0x6C` → only `0x00E4E8E0`/`0x00E3A9E0` remain.
5. Whether the XP-adjusted cost is charged anywhere else (only the panel and the auto-build found).
6. Key `0x0B` and the per-mode minimum for battle modes 2/4 (§55 (5)).
7. Formation `+0x670` (needs the debugger).

### (7) Checks
- `cargo test -p ntw_data -p ntw_sim -p ntw_campaign -p ntw_ai` → **485 passed, 0 failed**, exit 0
  (up two from §55's 377-in-those-crates count / 483 on this branch before the round: the two new
  `unit_scale` tests).
- `cargo test -p ntw_data -p ntw_campaign -p ntw_script -- --ignored` → **15 passed**, 0 failed
  against the real install (read-only).
- `cargo clippy -p ntw_sim -p ntw_data --all-targets` → **no warning in any file this round
  touched** (`battle/unit_scale.rs`, `battle/model.rs`).
- **`crates/napoleon` was not built** (forbidden; Bevy crash). Its two edits above are therefore
  still unverified-for-compilation. **NO PUSH.**

## 57. Manager merge: the melee chain has NO experience term (0-A round 18, re-landed)

The 0-A worker's branch (`work/fidelity-battle-w2`, commits `85d91c9` / `aee8f58`) was branched at
round 17 and reached this file 246 commits late, so its own section numbers collided with §§53-56
above and a blind merge produced a ~440-line conflict. The manager re-landed only the part that is
new: three test/doc changes touching no file that §§53-56 touched, so nothing from those sections
was reverted (verified: `crates/ntw_sim/src/battle/melee.rs`, `battle/rules.rs` and
`crates/ntw_data/src/kv.rs` were untouched on `main` since the branch point).

### (1) The finding: CONFIRMED, melee has no experience term
The whole blow-resolution chain was read out of the exe:
`0x00DAA290` -> `0x00DAB5F0` (hit number) -> `0x00DADA40` (kill chance) -> `0x00DAC2A0` (clamp) ->
`0x00DADB20` (roll), plus every helper.

- `0x00DAB5F0` logs all **eleven** of its factors by name, and **none of them is experience**.
- The rules slots those functions read are `4`, `0x10`, `0x14`, `0x18`, `0x5C`, `0x60`, `0x64`,
  `0x68` — **never slot 0**.
- A whole-binary sweep for defined strings containing `experience` points at exactly one function,
  the battle-file parser `0x0050CAE0` (`unit_experience` / `ship_experience`); the card builder
  `0x00513440` is a plain struct copy with no experience-to-stat arithmetic.

**Consequence:** `relative_melee_experience_multiplier` is `kv_rules` key 0, is loaded by
`0x00F42950`, and is **never read**. The model leaving experience out of melee is therefore
**faithful, not a gap**. Test `no_experience_term_in_the_melee_chain` pins this, with the sample's
hit number 234 asserted so the test cannot pass vacuously.

### (2) The rules-adapter slot numbering: CONFIRMED
The exe reads key `i` of `kv_rules_table` through a rules adapter whose vtable method at `+4*i`
returns key `i`, so **the list position in `KV_RULES_KEYS` is the slot index**. Anchored by three
independent decompiled readers: fatigue at `+4` (`0x00DAD990`), the four attack-direction factors at
`+0x5C..+0x68` = keys 23..26 (`0x00DAC7A0`), and the height min / max / divisor at
`+0x10 / +0x14 / +0x18` = keys 4 / 5 / 6 (`0x00DAD9C0`). Test
`kv_rules_list_order_is_the_exe_slot_index` locks all nine anchored positions.

### (3) Re-verified, no change needed
The rest of the melee port was re-checked term by term against a fresh decompile: order, signs, the
charge halving and reflect tests, the kill-chance bands, the extra-attacker bonus, the xholds tiers
and the single roll all match what is already ported. Kill chance is clamped `1 .. 0x3DE` (990) and
applied twice — now pinned by `kill_chance_clamp_is_1_to_990`.

### (4) Known-failing AI test: now passes
`ntw_ai` `real_battle` `real_ai_against_the_models_default_behaviour` no longer fails. Over 8 seeds
on the real install: AI Austria **2/8** against the default's 1/8, AI France **8/8** against the
default's 7/8 (default v default: Austria 1/8, France 7/8), so `2 >= 1`, `8 >= 7`, `10 > 8` and
`10 >= 9` all hold. The Austrian numbers moved off the 3/8 in the worker table because round 17's
per-drill firing took effect; §6.2's explanation stands.

**Read that precisely:** the assertions hold, not that the AI is now strong. Austria at 2/8 beats the
default but is thin, and whether that charge plays as well as the original depends on AI parts that
are still PROVISIONAL.

### (5) Still UNKNOWN, narrowed
Whether the battle-file `unit_experience` level reaches anything at all. Experience acts in
recruitment cost via `XpAdjustedCost` (§54/§55), and `0x00513440` does no experience-to-stat
arithmetic. Next lead: whoever reads the level after the parser stores it.
> **CLOSED by §58 (1)–(3):** the readers were already found (§52 (4), §19a) and are ported; the
> field is CONFIRMED live data; what is still UNKNOWN is the *writer* of `unit+0xD48`, which needs a
> debugger write watchpoint.

### (6) Checks
- `cargo test -p ntw_sim battle::melee` — 14 passed, 0 failed (2 new)
- `cargo test -p ntw_data kv` — 9 passed, 0 failed (1 new)
- `cargo test -p ntw_ai --test real_battle` — 3 passed, 0 failed, 2 ignored
- `cargo build --workspace` exit 0; `cargo clippy --workspace` no new warnings
- Play-check: `--battle-key NHB_Waterloo` runs, 52-63 FPS with 4,864 men. The §52(5)
  closed-channel failure does not reproduce on this build.
- The one workspace failure, `ntw_campaign save_compat::user_saves_pass_the_checks`, reproduces
  identically on the unmodified tree (the worker stashed and re-ran): environmental, reading the
  user's own `auto_save.save`. Already recorded in §52(5).

## 58. Round 19: §57 (5) settled, and the Austerlitz window's recorded root cause REFUTED

Two items, both doable without the debugger. No new Ghidra was run this round (Worker 1 had the
shared project); every exe fact below comes from the committed evidence in
`analysis/fidelity/ghidra_evidence/0a/` (`exp15_out.txt`, `exp_out.txt`, `sb0a_xp1_out.txt`), and
every data fact from the real install.

### (1) §57 (5) reframed — the *consumers* are already found and ported; the *writer* is what is missing
§57 (5) asked "whoever reads the battle-file `unit_experience` level after the parser stores it".
Re-reading the twelve `+0xD48` readers of §56 (1) against the committed decompilations shows the
consumers were never missing — they were found in §52 (4) and §19a and are ported:

| reader | what it is | in the model |
|---|---|---|
| `0x00670F40` | the `unit_stats_land_experience_bonuses` column 6 fatigue term | `fatigue::experience_bonuses` |
| `0x0053E4D0`, `0x0053A720` | the waver and rout timers | `morale::waver_timeout`, `morale::rout_timeout` |
| `0x005ABF40`, `0x005CD340` | the unit-card panel's `"Experience"` entry (`0x005ABF40` pushes the byte under that name) | exposed to the original UI Lua as `Experience` (`battle_prelude.lua`, `ui/battle.rs`) |
| `0x009AA5E0` | the campaign unit-info **script getter** table (§56 (1)) | `ntw_script::ui::campaign` sets `Experience` |
| `0x008590B0`, `0x005BDCE0` (97 bytes, 0 callers, vtable-only), `0x006C47D3/FD` | unattributed getters | not ported, no formula found |
| `0x0057F070` | the per-unit averages tick: `+0xC70` = mean of the soldiers' vfunc `+0xE0`, `+0xC74` = mean fatigue (§4.1) | display aggregation only (INFERRED) |

**CONFIRMED (new, from the committed evidence):** the parser does reach the card builder, and the
builder is a plain copy with no experience arithmetic — so §57 (1)'s "no experience in the card
path" is now a statement about the *code*, not an inference:
- `0a/exp15_out.txt`: `ins:0x0050CAE0:0x00513000:CALL.*00513440` → exactly one hit, `0050eb77 CALL
  0x00513440 in FUN_0050cae0`, immediately after `0x00513A10` builds the card's inputs
  (`0a/exp16_out.txt`, `0a/exp17_out.txt`).
- `0a/exp_out.txt`, `FUN_00513440` (279 bytes, **one caller**, `0x0050CAE0`): `*in_ECX = param_1`, then a
  29-dword loop copying `param_2`, then `in_ECX[0x1e]`, three sub-object constructors, three copies
  from `param_6`, `in_ECX[0x32] = in_ECX[0x33] = param_4` (the men, §18a/§39), `in_ECX[0x35..0x3a]`,
  two sub-object constructors, and **one single byte store, `card+0x110 = param_12`**. That is the
  only byte it writes outside the copied dwords.
- **And it is not the experience byte:** §18a calls this a *0x114-byte* card and §6 reads the men as
  `(unit+0x1C)+0xC8`, so the card is at `unit+0x1C` and `card+0x110` is `unit+0x12C`. The experience
  byte is at `unit+0xD48`, far outside the card.

So the writer of `unit+0xD48` is **not** the card builder, **not** the card constructor `0x00513320`,
and **not** any scalar store in `0x00400000..0x01000000` (§56 (1)). **UNKNOWN**, narrowed to two
mechanisms a scalar-operand sweep cannot see: (a) an aggregate/bulk copy of the unit (a `memcpy`
shape or `rep movsd`), or (b) a store through a computed address (`lea`/`add` then `mov [eax],cl`)
— note the sweep did find the *one* `LEA [ECX+0xD48]` in the whole exe and it belongs to the
campaign model's container, so (b) has no `+0xD48` operand to find either, which points at (a).
Settling it needs a **debugger write watchpoint on `unit+0xD48`** during a battle-file load — the
same batched session as formation `+0x670` / garrison `+0x6C`
(`analysis/fidelity/debugger/f0a_battle_probe.cdb.txt`, which is `.gitignore`d, so it lives only in
the 0-A worktrees). Not attempted (no user at a keyboard). For whoever runs it, the twelve reader
instruction addresses are in `0a/sb0a_xp1_out.txt` lines 4-27 (`0x0053a739`, `0x0053e4e5`,
`0x0057f0e8`, `0x005ac243`, `0x005bdcec`, `0x005cd422`, `0x005cd5be`, `0x00671152`, `0x006c47d3`,
`0x006c47fd`, `0x00760d39`, `0x008591df`; the only non-read is `0x008762cb`, the campaign model's
container). **Nothing invented; no value substituted for the writer.**

**Consequence for the model: none.** The model reads the level from the battle file, which is the
only source a historical battle has, and the exe has no other writer we can find. So the current
wiring is the faithful end of the chain as far as static analysis reaches.

### (2) The field is live data, not a dead one — CONFIRMED, measured off the install
This is the part §57 could not say: whether the level ever *matters*. It does, a lot.
- New test `ntw_formats` `battle_file_experience_coverage` (real install, `#[ignore]`d): over the
  **32** installed `*_battle.xml` files, **25 carry `<unit_experience level=...>`**, covering
  **1267 of 1350** units (including reinforcement armies). Levels seen: `0:403, 1:404, 2:156,
  3:147, 4:49, 5:53, 6:35, 7:12, 8:3, 9:5`.
- **Every level is 0..9** — exactly the ten rows of `unit_stats_land_experience_bonuses` and
  `unit_stats_naval_experience_bonuses` (§19a), which is strong independent evidence that this field
  *is* the chevron level `unit+0xD48` and not some other byte. The test asserts the range.
- A general's own `<experience>` element is a different thing: 9 units have one, it is the display
  value next to the name, and the exe does not read it into the unit (`star_rating level` is what it
  takes for the rank — CONFIRMED, `apply_spec_unit`).
- Napoleon Austerlitz itself: 40 units, **every one of them with a level**, spanning 1..6. So the
  file the "Austerlitz length" question is about is a battle where no unit is a raw recruit.

### (3) The level reaches the model exactly, and it moves real numbers — CONFIRMED, tested
- `napoleon` `battle_file_experience_reaches_every_unit`: builds **all 25** installed land battles
  from the `battles` table and asserts `LandUnit::experience` equals the file's `unit_experience
  level` for **every** unit, in the same order `historical_armies` builds them. The widest spread is
  **7 distinct levels** (`NHMPB_Waterloo_2v2` and `NHB_Waterloo_Well`), and level 9 does occur, so
  the whole range is covered and a rank-0 stand-in cannot pass.
  (Writing this test found a real ordering trap: `historical_armies` walks **all** alliances'
  armies first and only then **all** alliances' reinforcement armies, not army+reinforcements per
  alliance. `historical_battles_build_with_all_units` flattens it per alliance and still passes
  because it only counts. The order is now commented at the second loop.)
- `napoleon` `the_experience_level_moves_fatigue_and_the_morale_timers`, against the real install:
  fatigue rows `[0, 0, 0, 0, 0, -1, -1, -2, -2, -3]` (level 9 gets **−3** fatigue per tick against
  level 0's **0**), waver timeout **40 → 85** light-update ticks (40 + 5·xp) and rout timeout
  **600 → 420** (600 − 20·xp) from level 0 to level 9.
  So in `NHMPB_Waterloo_1v2` (levels 0, 1, 4, 5, 7, 9) the file's own numbers hand the British
  veterans 180 fewer rout ticks of resistance than the green units, and a level-9 unit's soldiers
  wear down three fatigue points per tick slower. The field moves the battle, not a label — and it
  is exactly the term §57 (1) proved the **melee** chain does not use.

### (4) Austerlitz: the recorded root cause is REFUTED, and the symptom does not reproduce
`analysis/fidelity/AUSTERLITZ_WINDOW_CLOSE_ANALYSIS.md` (sandbox 0-A, 2026-10-04) concluded
**INFERRED** "the battle file's `duration` field set to 60 seconds, which triggers a timeout during
the 64-second intro cutscene", and listed "verification needed: read the actual `duration`". Done:
- **REFUTED.** `napoleon_historical_battles\austerlitz\austerlitz_battle.xml` has
  **`<duration>2400</duration>`** (timeout winner alliance 0). Measured over the 29 battle files
  `every_battle_file_parses` walks, the limits are 1800 s (Napoleon Lodi), 2100 s (Napoleon Arcole),
  2400 s (the other 25, Austerlitz among them) and 3000 s (MP Arcole 1v2 and MP Lodi 2v2). Nothing
  is near 60 s, so the timeout cannot fire at 60 s and the analysis file's **options B and C
  ("pause battle time during cutscenes", "ignore the timeout while a cutscene is active") are
  unfounded** — they would be a fidelity regression, since §9's rule (`0x00582FB0`: the clock adds
  0.1 s per tick only in states 2..5, the limit test is strict `duration < clock`, and it runs after
  the alliance test) is already ported exactly.
- **The window does not close any more.** Three runs on this build (debug, the real install):
  1. `--battle-key NHB_Austerlitz`, no harness, 150 s wall clock → the window stayed open, and
     nothing was logged: the battle never left **Deployment**.
  2. `--battle-key NHB_Austerlitz --battle-ui-click wait,button_battle_start`, `NAPOLEON_AI_SPEED=8`,
     260 s wall clock → deployment finished 11 s in, the battle ran and **ended on its own at
     712.1 s: `Won { side: 0 }`** (France), and the window stayed open after that (the process was
     still running when the harness timeout stopped it).
  3. `--battle --screenshot <png>` → **exit 0 with the picture written** (1.88 MB), which is the
     second half of the old report ("exited 0 without saving its picture").
- **What the old runs probably were (INFERRED, and it is a harness fact not a model bug):** a plain
  `--battle-key` run never ends deployment by itself — `hud::harness_clicks` only clicks Start Battle
  for a `NAPOLEON_AI_SHOT` run or when `--battle-ui-click` is given, and `hud.rs` already records
  that "nothing ends deployment by itself (CONFIRMED by repeated runs)". So battle time never
  advanced, the cutscene never mattered, and any exit those runs saw came from a harness (the
  front-end `AutoScreenshot` at 4 s + exit at 5 s, which is **not** gated on `GameMode`; or
  `fps_log`'s `NAPOLEON_FPS_LOG`; or `AiShot`'s auto-click). **Recipe for a headless Austerlitz run:
  `--battle-key NHB_Austerlitz --battle-ui-click wait,button_battle_start`** (or
  `NAPOLEON_AI_SHOT=<png>@<s>`), never a bare `--battle-key`.
- **Verdict: nothing to change in the model.** The documented symptom is closed as *not reproducible
  on current main*, the one recorded hypothesis is refuted with data, and
  `AUSTERLITZ_WINDOW_CLOSE_ANALYSIS.md` should be treated as superseded by this section (left in
  place for the audit trail).

### (5) Resolved / open
| item | verdict | evidence |
|---|---|---|
| Who reads the battle-file `unit_experience` | **SETTLED — the three formula readers were already found (§52 (4), §19a) and are ported**; the rest are display/script getters | §58 (1) |
| Does the level reach anything at all | **CONFIRMED yes**: the fatigue term and both morale timers, with real numbers | §58 (3) |
| Is the field load-bearing in the shipped data | **CONFIRMED**: 25/32 files, 1267/1350 units, levels exactly 0..9 | §58 (2) |
| Does the level reach the model | **CONFIRMED**, unit for unit, over all 25 land battles (tested) | §58 (3) |
| Who *writes* `unit+0xD48` | **UNKNOWN** — not the card builder (`card+0x110` = `unit+0x12C`), not any scalar store in the exe; a debugger write watchpoint is the only way | §58 (1) |
| Melee and experience | **CONFIRMED negative**, unchanged (§57 (1)) | §57 |
| Austerlitz `duration = 60` | **REFUTED**: 2400 s | §58 (4) |
| Austerlitz window closes by itself | **NOT REPRODUCIBLE** on current main; three runs, one full 712.1 s battle | §58 (4) |

### (6) Checks
- `cargo build --workspace` — exit 0.
- `cargo test --workspace` — **776 passed, 0 failed, 73 ignored** across 51 suites, exit 0. (The
  §52(5) `save_compat::user_saves_pass_the_checks` failure did **not** reproduce this round: the
  user's save folder is empty, so that test skips.)
- `cargo test -p ntw_formats --test battle_spec_install -- --ignored` — **3 passed, 0 failed**
  (one of them the new `battle_file_experience_coverage`, against the real install).
- `cargo test -p napoleon --bin napoleon experience` — **2 passed, 0 failed** (the two new tests).
- `cargo clippy --workspace --all-targets` — exit 0, **145 warnings workspace-wide and none of them
  in `crates/ntw_formats/tests/battle_spec_install.rs` or `crates/napoleon/src/battle/setup.rs`**
  (no new warning in a file this round touched).
- Play-checks (real install, debug build, logs in `target/tmp/w2_aust_run*.txt`, `w2_battle_shot.txt`):
  the three Austerlitz / `--battle --screenshot` runs of §58 (4).

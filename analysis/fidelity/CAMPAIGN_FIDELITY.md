# Campaign and front-end fidelity pass (backlog §0, pass 0-B)

Worker: fidelity-campaign (branch `work/fidelity-campaign`, worktree `%USERPROFILE%\Documents\NR-fidelity-campaign`).
Tags: CONFIRMED / INFERRED / UNKNOWN; stand-ins stay PROVISIONAL / PLACEHOLDER.
Ghidra copy: `%USERPROFILE%\Documents\NR-fc-ghidra` (copy of NR-s1-ghidra). Scripts: `analysis/fidelity/campaign_ghidra/`
(`CampDecomp.java` = the AI worker's AiDecomp plus `strx:`, `mem:`, `vars:`, `vdisp:`, `callsites:`, `ptrs:` modes;
`run_ghidra.ps1 <targets> <out> [maxLines]`). Ghidra output and temp files stay out of git (`target\tmp`).
Research tool: `cargo run -p ntw_campaign --release --example economy_check -- <data dir> <save>...` (model taxes next to
the taxes the original stored in `FACTION_ECONOMICS`); `... economy_check -- db <data dir> <table> <codes> [filter]`.

## Where I am / what's next
**State on main (ported from the sandbox, reviewed at `sandbox/main` 90bfc1c, branch
`work/sandbox-port-campaign`).** Main now holds 0-B rounds 11-15: rounds 12-13 (government drift
`0x00B1B5A0`) were already on main; round 14 (the governing faction and the tax-exempt guard in the
region recompute, the pair-keyed `diplomatic_relations_government_type` documented) and round 15
(`world::recruited_unit_size`, the property sweep, the `economy_check` modes) were ported, and
`spawn_recruited_unit` now **calls** `world::recruited_unit_size` (it was left as a "note to 0-E" in
the sandbox). Also ported from other sandbox workers into the campaign model: 0-G's
`DemolishBuilding` / `can_demolish`, 0-E's fort options (`fort_levels` / `fort_options` /
`can_build_fort`), and the AI's `RESEARCH_TECHNOLOGY` (`analysis/ai/AI_RESEARCH.md`).
**Not on main:** 0-E's `TransferRegion` (deal region step `0x00B449F0`) — its effect is INFERRED
(it reuses the capture's `occupy`), which leaves the old owner's garrison army standing in the
handed-over settlement with a stale `garrisoned_in` (the capture path destroys the garrison before
`occupy`; what the deal does with it is UNKNOWN), and its turn gate on the region's owner refuses
the demand-side deals the sandbox UI issues; it needs `0x00B449F0`'s flags decoded first. The
sandbox's experience-adjusted campaign cost (`0x00ED49A0`, `XpCostTables`) was wrong and is gone: the
campaign never calls `0x00ED49A0` (§Recruitment cost and money); the UI side of demolish /
forts / deals (`ntw_script::ui::campaign`) belongs to the UI worker. The round 14/15 Ghidra listings
cited below are in `analysis/fidelity/ghidra_evidence/0b/gh__r15_*` (ported to main by the evidence worker).

**Round 16 (worker campaign-0b, 2026-10-08).**
- **Importer limit — CLOSED: there is none; the split is `0x00BC26D0` (CONFIRMED, ported).** `0x00BB5730` caps a
  builder source only by its path's first stop (300 per settlement by land, the port's `commodity_export_vol`); the
  importers' shares come from `0x00BC26D0` (run by `0x00BBD870` after the builder `0x00BC0960`): every international
  route cleared (`0x00B1C1A0`); an exporter without a capital (faction `+0x72C`) exports nothing; a partner's demand is
  its **net** demand = Σ region `+0x154` over its region list (`0x008F4E50`, faction `+0x778`) minus its own supply
  when it has a capital, kept only if > 0 for some commodity; partners in the world's faction list order
  (`FACTION_ARRAY` file order: loader `0x0090BEB0` appends to world `+0x2C/+0x30`, the list `0x00BC26D0` walks as
  `+0x20`) with a trade agreement (relationship `+0x788`) and a route; per commodity the (partner, demand) pairs are
  sorted largest first with the exe's `std::sort` (`0x00B7F790`: VC++ 2008 introsort, insertion sort up to 32,
  ported as `ntw_sim::msvc_sort`) and handed out from the end: total = Σ max(demand, 1); while `k` partners are left
  and `k ≤ rest`, share = max(1, (demand × rest as i32) as u32 / total); total −= demand; the share goes onto every
  route of the pair (`0x00B08880`). Result: `ECON_SPLIT` **352/352** route volumes on the 9 vanilla saves (was
  350/352; the two `orig_over_nr4_0252` misses are gone), `ECON_TRADE` 230/230 incomes. Tests
  `supply_is_split_over_the_partners_by_their_net_demand`, `partners_without_net_demand_take_nothing_and_the_rest_may_stay_home`,
  three `msvc_sort` tests.
- **The 4 desertion-exempt classes — CLOSED (CONFIRMED, ported).** `UNIT_RECORD` ctor `0x00E91320` sets `+0x20` =
  `0x00EED3E0(units #3 class key)` (a fixed key list, any other key 0); codes 4 / 0xB / 0xC / 0xE are
  `cavalry_heavy`, `elephants`, `general`, `infantry_elite`. `0x008BA1E0` skips those units before the RNG draw.
  Code `economy::DESERTION_EXEMPT_CLASSES`; test `exempt_unit_classes_do_not_desert`.
- **Recruitment details — CLOSED (CONFIRMED, ported) except the recruitable population.** The campaign cost is
  `units` #7 with the region's cost effects (not #4, not the experience-adjusted `0x00ED49A0`, which is the battle
  army-setup price), charged and refunded unchanged; a faction in debt may recruit; `units` #15 caps a unit type per
  faction. See §Recruitment cost and money. Left: the recruitable population gate and charge (vars 36 / 37, 0 in the
  shipped data; PROVISIONAL in `recruitable_entry_flags`).
**Round 15 (data first, Ghidra last), complete in the sandbox.** `ECON_RECOMP` is untouched and still
exact (sandbox count: 713 tests).
- **Item 1 — the two round-14 fixes turned into properties (§GDP, "Round 15").** The exposure was
  measured first (`ECON_TAXEX=1`) and it is thin: across the six start positions and the nine vanilla
  saves there are **2** tax-exempt regions and **1** region governed by another faction, and four of
  the five eur saves and all four spa saves have neither. Two new tests therefore re-shape the model
  instead of adding one-off regions: `tax_exemption_and_governorship_hold_for_every_region_and_faction_pair`
  (**25260** `(region, governor, exempt)` shapes over 436 owned regions in 9 files; 12630 exemption
  flips changed the growth, 18728 foreign governors changed the answer, 12194 composed shapes) and
  `tax_exemption_and_governorship_hold_without_the_faction_wide_effects` (1518 shapes). Both are
  **mutation-checked**: reintroducing either round-14 bug fails them. **LIVE.** (On main only the
  first is kept: the second passes the same computed faction effects over the same start positions,
  so it is a subset of the first, not the effect-free state its doc claims.)
- **Item 2 — the recruited unit's size is CONFIRMED (§Recruitment, "Round 15").** `0x00B71FB0` creates
  no units (this round's decompile), so placement/garrison-join really was 0-E's and is already ported.
  The one sub-item left, `num_men`, is decidable from data: **every one of the 3174 units** in the nine
  vanilla saves has `max_men` = `unit_stats_land.num_men` for land and = **the sum of the
  `unit_stats_naval` crew triple** for ships. Added `world::recruited_unit_size` and the test
  `recruited_unit_size_is_num_men_for_land_and_the_crew_sum_for_ships`. **The naval stand-in in
  `spawn_recruited_unit` is replaced on main**: it calls `world::recruited_unit_size` (unit test
  `recruited_units_are_sized_by_num_men_or_the_crew_sum`). **LIVE.**
- **Item 3 — the 4 desertion-exempt classes: still UNKNOWN, but round 14's recorded next step was
  based on a wrong address (§Bankruptcy).** The test reads `*(unit+0x48) + 0x20`, **not**
  `LAND_UNIT + 0x20` — so naming "campaign LAND_UNIT +0x20" (the round 14 next step) could not have
  worked. `*(unit+0x48)` is the unit's **stats record** (CONFIRMED: `0x008F9B10` reads
  `+0x24` category key, `+0x28` class key, `+0x3C` upkeep off the same pointer, exactly as §Economy
  documents for that function), so `+0x20` is an int sitting immediately before the category key.
  It has exactly **one** other reader in the binary, vtable slot 0 of the record's table
  (`0x008F6810`: `return record->[0x20] == arg`, reached only through 38 vtables at `0x013554B0`).
  Ruled out: it is **not** `num_men` — that column takes only {24, 32, 48, 60, 80, 120, 160}, so none
  of the four codes can occur in it. Not ported. Sharper blocker, same PROVISIONAL standing.
- **Item 4 — `0x00B1B190`: the contradiction is RESOLVED from the bytes, and there was never a
  contradiction (§Diplomacy rules, "Round 15").** `0x00B1B190` calls **`0x00B69A70`**, not `0x00B69640`.
  `0x00B69A70(arg1, arg2)` writes `factor+0x18 = arg1` (drift) and `factor+0x20 = arg2` (limit) and
  **never touches `factor+0x1C` (the value)**, where `0x00B69640(arg1..arg3)` writes
  `+0x1C = value`, `+0x18 = drift`, `+0x20 = limit`. Two different signatures, so the identical push
  sequence means different things and round 13's `drift = #3, limit = ±2` is **correct as read**, not an
  argument-order artifact. The `±2` is also not a literal: it is `[campaign_model+0xFAC+0x94]`, the same
  `government_type` event drift field its twin reads. Still **not ported**, now for a different and
  sharper reason: the **sign** test reads `record+0x28C` (= factor 20's `+0x04`, factors being 0x28
  apart from `record+0x8`), which is still unnamed, and no vanilla save reaches the path. A side
  finding: the setup writer `0x00B45A40` initialises **`record+0x2B0` = factor 21**, not the factor 20
  the change path writes — worth a look before anything is ported.
- Ghidra output archived under `analysis/fidelity/ghidra_evidence/0b/gh__r15_*` (22 files).
**Round 11 (close every 0-B residual), in progress.** Status for a resume:
- Done and committed:
  - France leader factor CONFIRMED (`effects::minister_level` uses the holder's raw main-attribute level;
    `ECON_CFACT` 506/506).
  - Debugger session (user, 2026-10-04). Script `target/tmp/probes/0b_session.cdb.txt`; log
    `target/tmp/probes/0b_session_6968_2026-10-04_11-02-42-588.log`; Napoleon.exe base 0x00E50000 in that run
    (static = addr − 0xE50000 + 0x400000). New vanilla saves copied to `target/tmp/van/`: `nr16_spa_t0` (turn 1)
    and `nr16_spa_t2` (turn 2, one round later despite the name), and `nr16_naval`..`nr20_naval` (eur Coalition,
    Britain).
- Findings from the log:
  - **Desertion gate CONFIRMED.** Faction +0x50C is economics (faction +0xAC) +0x460, the bankrupt-turn counter.
    A write-watch caught `MOV [EBP+0x460],0` at 0x00BAC0B9 in 0x00BABE30. The model's gate (bankrupt turns > 1)
    is the original's. Comments in `economy.rs` are updated.
  - **Naval inputs CONFIRMED** and ported in `naval.rs` / `ntw_campaign/src/rules.rs`. The battle record O is the
    `unit_stats_naval` row:
    - O+0x1D8 = #20, O+0x1DC = #21, O+0x1C8 = #1 (morale);
    - model+0x7C = the type's gun count (= saved full guns; one count per model, `rules::fill_ship_guns`);
    - the potential matched all 71 logged ships;
    - ×1.5 for classes 0x18 / 0x26 = bomb ketch / rocket ship (unit class enum `0x00EED3E0`);
    - range class = table `0x0070D370` by `units` category: line 3, frigate and galley 2, others 1, artillery 2;
      the range level = class difference (188/188);
    - max morale = the highest #1 of all ship types (14);
    - merchant (category 11) winner loss × 0.95.
  - **Ship captures ported**: `naval::captives` / `capture_ship` (0x00792CD0, 0x00793D60, 0x0075C090,
    0x007CD570); captured ships move to the captor's navy (`battles.rs`). The log has 5 captures in 5 battles.
    PROVISIONAL: the force-weight doubling field (+0xE8 of the setup record); the capture crews base (card +0x70).
  - **Spa drift CONFIRMED (fixed).** The changes come from the conversion step at the round end with the
    missionaries where they stand then (the British missionary moved from Badajoz to Caceres during the round).
    The missionary rank adds the theatre zeal bonus `zeal_europe` (`0x00A198D0`, traits `zeal_spain`):
    `religion::missionary_rank`. Check: `ECON_MAP=1 ECON_RELIGION_CHARS=1 ECON_RELIGION=nr16_spa_t2 nr16_spa_t0`
    gives all 341 shares within 1e-3. The 3-turn `orig_fr_t1` pair cannot be checked: the missionaries'
    intermediate positions are not in any file.
  - Theatres CONFIRMED a no-op (`economy_check theatres`: one theatre per shipped map, all regions in it).
  - Research army/navy mods CONFIRMED never to apply. The tech's building +0x5C+0xC is compared with
    "army-admin" / "navy-admin", and no DB string carries those keys (only the trigger script text).
  - **Region recompute omissions settled.**
    - The building chain modifiers: `effect_bonus_value_building_chain_junctions` is loaded (`EffectBonusChain`)
      and mapped as bonus type 2 (mod_cost 0 / mod_gdp 1 / mod_tw_growth 2 / mod_commodity_production 3,
      qualifier = chain), read per building by `0x00A6AFC0` → `0x00E1EF10(chain, 1/2)`.
    - The home-theatre `tw_growth_home_region` (effect 0x36, `0x00A8B5C0`) added once from the faction part.
    - `ECON_RECOMP` on the vanilla saves: GDP and growth **exact on all 9** — 72/72 on the five eur saves,
      31/31 on the four spa (round 14; before: 71–72 / 69–72, the misses being eur_wallachia,
      eur_moravia and eur_bavaria — see §GDP for the two causes). Spa was already 31/31. Before both: GDP
      67–69, growth 28/72.
    - Still not modelled, 0 in the shipped data: `slots_gdp_values` (1.0) and the commodity terms.
  - **Construction cost**: researched technologies' chain cost entries are counted (same mapping).
    `ECON_BUILDCOST`: 328/330 queued items on 13 vanilla saves; the 2 are the known Bavarian items.
- Left, with the next step:
  - Recruitment details (open item 7): full refund, the new army waiting outside, unit size from `num_men`, an
    army joining a garrison. Next: decompile `0x00AECEE0` / `0x00AED220`.
    - Round 12 step 1 (traced, output in `target/tmp/0b_round12/`, git-ignored): `0x00AECEE0` /
      `0x00AED220` are the land / naval recruitment-item constructors, each called only from the
      queue-unit function `0x00B58DD0`; turns come from `UNIT_RECORD` +0x34 (already CONFIRMED).
      `0x00B58DD0` fails on queue-full (`0x00B62040`), on recruitable-lookup (`0x00B45690`) miss,
      or when the entry's flag [5] != 0 (INFERRED blocked-entry gate, not modelled). Cancel
      `0x00B1A820` calls the item slot +0x1c (`0x00B5C060`) with 1, which credits the faction
      economics via `0x00BB3810(item+0x20, kind 3)` (bucket + total), and adds campaign variable 37
      (`recruitment_population_cost`, name from the ordered registration dump) to the queue
      manager's +0x54. So a money credit on cancel is CONFIRMED in structure; the FULL amount is
      still INFERRED (kind-3 evaluator and item+0x20 contents not traced), and population is
      unmodelled. Still open: new-army placement, `num_men` size, garrison join (spawn path).
      - Round 12 step 2 (traced, output in `target/tmp/0b_round12/spawn.txt` + `spawn2.txt`,
        git-ignored): item slot 8 `0x00B5C1B0` calls `0x00B5C2D0`, then while item+0x18 != 0
        tries two context paths (`0x009D3A40`, i.e. context slot +0x3c; `0x006649C0` slot +0x38
        gating item slot 5) and otherwise resets item+0x18 to 0 (CONFIRMED structure, UNKNOWN
        meaning). The `0x00B71FB0` tail builds its pair list via the queue's slot +0x20, adds
        entries via `0x00B0A270` (a 0x54-byte record from `0x00AE8B10` holding the item+0x18
        value), and drops queued items passing `0x00B5C110` (which reads item+0x24 when
        item+0x1c != 0). No `num_men` reader and no position / garrison-branch reader found on
        this path, so all three spawn sub-items stay UNKNOWN and the model is unchanged
        (PROVISIONAL stand-ins stand). Next: decompile the two context slots and the item
        slot 5 target to name the placement branches.
      - Round 12 step 3 (traced, output in `target/tmp/0b_round12/step3*.txt`,
        git-ignored; Ghidra copy `%USERPROFILE%\Documents\NR-fc-ghidra`, read-only):
        - Item slot 5 is `0x00B1B480` (land) / `0x00B1B4B0` (naval), from the `vtat:`
          dumps of `0x0137CB1C` / `0x0137CB64` (CONFIRMED slot identity): if item+0x18
          != 0 unregister `0x008B0150(item+0x5c)`, then `0x009D3A40` / `0x009D3A80`
          on the passed-in object `y`, then `0x008A9110` / `0x008A9120`, then
          item+0x18 = `y` (CONFIRMED structure from `lst:0x00B1B480`).
        - `0x008A9110` / `0x008A9120` are thunks (`lst:0x008A9100`: ECX += 0xA4 /
          0x94, then `0x008A9100`: `[ESP+4]` += 0x5C, ECX += 0x30, JMP `0x008CF810`;
          CONFIRMED). `0x008CF810` is a vector push-back with doubling (CONFIRMED
          structure): fails when `param+4 != 0` or ECX == 0, else appends `param`
          to the ECX+8/+0xC/+0x10 list and sets `param+4` = ECX. So land registers
          item+0x5C into context+0xD4, naval into context+0xC4; `0x008B0150` removes
          an entry from such a list (CONFIRMED first half; INFERRED purpose).
        - Context accessors (CONFIRMED listings): `0x009D3A40` = ECX+0x2A8 holder
          deref (`0x006649C0`) then virtual +0x3C; `0x009D3A60` = same holder then
          +0x38; `0x009D3A80` = same holder then +0x44 (naval). `0x00898E80` is a
          `0x006649C0` duplicate (CONFIRMED identical body). Slot 8 path B
          (`lst:0x00B5C1B0`): ESI = `[item+0x18]`+0x2B0, objB = `**(ESI+4)`, gate
          objB+0x38, `tmp` = objB+0x34, `y` = `**(tmp+0x74+4)`, then item slot 5
          (`y`); path A: objA = `**([item+0x18]+0x2A8+4)`, `r` = objA+0x3C, if `r`
          then register `r` via `0x008A9110`.
        - Outcome: this whole path is list bookkeeping (register / unregister
          item+0x5C), NOT unit creation: no `num_men` reader and no
          garrison-join vs new-army branch on it. Branch names stay UNKNOWN, the
          model is unchanged (PROVISIONAL stand-ins stand). No code change.
        - Next lead: (a) recover the `P = [item+0x18]` argument at the
          `0x00AECEE0` / `0x00AED220` call in `0x00B58DD0` (`lst:` of the call
          region; decomp shows stack noise) to type the context; (b) find the
          consumer of the context+0xD4 (land) / +0xC4 (naval) lists — the walker
          that creates the force/unit is where the garrison-join vs new-army
          branch and the `num_men` size reader live (candidates: walkers of those
          offsets near the recruitment code, or the `0x00B0A270` 0x54-record
          consumer in the `0x00B71FB0` tail).
    - Round 12 step 5 (**ported**): the government drift on a government change, `0x00B1B5A0` (decoded
      round 10) — see §Diplomacy rules. `CampaignCommand::ChangeGovernment` /
      `CampaignModel::change_government` (code `treaties.rs`), `GovernmentType::from_db_key`
      (`world.rs`), event `GovernmentChanged`, two tests in `campaign/tests.rs`. The round 10 note said
      the factor is "set drifting toward #2"; the drift a turn is INFERRED: it is the
      `government_type` attitude event's 2 (`0x0042F110`) with the sign taken towards the limit, which
      is what the drift step (`0x00B290D0`) needs (it clamps with `min(limit)` when the drift is
      positive, `max(limit)` when it is negative). Every shipped `diplomatic_relations_government_type`
      row has #3 above #2 (absolute monarchy towards republic −30 → −100, DB_CAMPAIGN_TABLES §9), so
      in the shipped data the factor falls 2 a turn to the limit — the sign rule is written so a row
      the other way round also holds. UNKNOWN: the callers of `0x00B1B5A0` in the exe (which UI /
      script path changes a government) and whether it re-reads the `religion` factor as well. The
      round 10 save check for this factor (506 / 506) still stands and was not re-run: it compares the
      setup's fixed #3 values, which only a government change rewrites, and no vanilla save has one.
    - Round 13 step 1 (**fully decoded; the round 12 port CORRECTED**): `0x00B1B5A0` read from the bytes
      (`0x00B1B5A0` .. `0x00B1B6A4`, output in `target/tmp/0b_round13/o{1,3,4,5,6}.txt`, git-ignored).
      Three things the round 12 port had wrong, all now fixed in `treaties.rs::change_government`:
      1. **value and limit were the wrong way round.** The exe pushes `[row + 0xC]` and `[row + 8]` as
         the last two arguments of `0x00B69640`, whose listing is `(value, drift, limit)`
         (`0x00B69640`: `*ptr(ECX+0x1C) = arg1`, `*ptr(ECX+0x18) = arg2`, `*ptr(ECX+0x20) = arg3`,
         `*ptr(ECX+0x24) = 1`, `RET 0xC`), so **value = `diplomatic_relations_government_type` #2 and
         limit = #3**. `0x00B45A40` (the setup writer) pushes `[row + 0xC]` into `0x00B69620`, so the
         campaign setup's fixed value is #3 - the two go opposite ways, and the port had them the same
         way round. A government change **shocks the factor to #2 and lets it recover to #3**; the port
         did the reverse (started at #3 and drifted to #2).
      2. **The drift sign followed from that**, so it was inverted too: the exe drifts **+2** on every
         shipped row (`0x00B1B665` `CMP EDI,[ESI + 8]` / `JGE` to the positive branch; the branch that
         negates is taken only when the limit is below the value). The port produced -2.
      3. **One direction only.** The single caller writes the record that has the changed faction as its
         **target** (the counterpart's attitude towards it), not both sides of every record.
      Also confirmed and now cited in the code: the drift magnitude is **not** an assumption - it is
      read out of the attitude events array (`[campaign_model + 0xFAC + 0x94]`), which is triple 12 =
      `government_type`, field +4 = drift = 2. The `abs()` is gone (the exe uses the raw field), and the
      in-the-game filter is gone (`0x00B1B5A0` has none). Test count unchanged at 285.
      Ledger and callers: see §Diplomacy rules, "Government change `0x00B1B5A0` (round 13 full decode)".
   - **Round 14 (data round; no Ghidra opened).** Status:
      - **The 3 remaining region-growth misses are FIXED** (Wallachia, Moravia, Bavaria) and `ECON_RECOMP` is
        exact on all 9 vanilla saves (72/72 and 31/31, GDP and growth; before: growth 69-72/72). Two wrong
        sources in `economy::recompute_region_with`, both CONFIRMED from the stored values:
        (a) a **tax-exempt** region (`REGION` #19) takes no `taxes_effects_jct` bundle - `eur_moravia` and
        `eur_wallachia` are both exempt, so their growth is the raw sum (5 and 0, the model gave -2 and -5);
        (b) the government and the faction-wide part are the **governing** faction's, not the owner's -
        `eur_bavaria` is Austria's but Bavaria's governorship, so it reads Bavaria's (no techs,
        `gdp_mod_all` 0), giving the stored 1275 / -3 where Austria's gave 1277 / 0. Both are the same set
        `0x00A67530` builds that `region_effect_set` already used for the public order. See §GDP.
        Tests: `gdp_and_growth_match_the_vanilla_saves` (484 region records) + 3 unit tests, one per region.
        `ntw_sim` 288 tests (was 285).
      - **The `diplomatic_relations_government_type` row key is the PAIR - our map was right and round 13's
        "single key" reading was wrong.** `record_index` takes one string, but it is the composite
        `govA + ";" + govB` that `0x00B1B5A0` / `0x00B1B190` / `0x00B45A40` build with the separator global
        `DAT_013305F8` (= ";", DB_BUILDERS.md 2b) - round 13 mistook `0x004F1200`'s three-argument concat for
        a copy ctor and called the code dead. And #2 is **not** a function of one government: within
        `own = gov_absolute_monarchy` alone #2 takes -100/-50/0/+50, within `target = gov_republic` alone
        -100/-50/0/+70, and grouping the save's 506 stored factors by either single column gives 3-4 distinct
        values. See the new §The row key of `diplomatic_relations_government_type`.
      - **The 4 desertion-exempt unit classes are still UNKNOWN** - a bounded negative result, documented in
        §Bankruptcy with the exposure numbers. Do not port a guess.
      - **No regression** after round 13's corrections: religion 506/506, government 506/506, leader 506/506,
        enlightenment 506/506 (`auto_nr4_t4`, `auto_b2b3_0211`, `auto_nr1`), 462/462 on `auto_after_c8` and
        `orig_over_nr4_0252`, 12/12 on all four spa saves; spa religion drift 341/341 (327 equal + 14 within
        1e-3, 0 further off).
      - `0x00B1B190` **not ported** - deferred deliberately: its sign test reads an unnamed factor sub-field
        (`factor + 0x04`), and taken literally its two pushes set `drift = row #3, limit = +-2`, which
        contradicts its twin `0x00B1B5A0` and looks like an argument-order artifact. No vanilla save reaches
        either (only a peace-treaty deal item calls them), so there is nothing to check a guess against.
      - **`ntw_script` did not compile at bf2519b** (found by running the whole workspace, not just
        `ntw_sim`): `ntw_script::host::context_for` had no arm for `CampaignEvent::GovernmentChanged`, so
        `cargo test -p ntw_script` was a hard `E0004` from the round 12 event. Fixed (one arm, the faction
        key as the context). Worth remembering: the round 12/13 counts only ever covered `-p ntw_sim`.
      - New tool modes: `ECON_RGROWTH=<key filter>` (every term of the region recompute),
        `ECON_GOVPAIR=1` (the 16 rows, the save's pairs, distinct values per column),
        `ECON_DESERT=1` (class ids of units outside settlements), `ECON_UCOLS=1` (the `units` int columns).
      - Next: (a) the `0x00B1B190` read above; (b) `LAND_UNIT` +0x20; (c) the recruitment spawn path's
        remaining three sub-items (open item 7) - still the largest unmodelled gameplay item in 0-B.
    - Peace terms: regions and technologies as deal items, region transfer `0x00B449F0`. Round 13 found that
     `0x00B449F0` is the *terms* applier, not just the region transfer: it is also what runs the
     government-change item (`0x008BEAA0` -> `0x00B1B100` -> `0x00B1B5A0`, see §Diplomacy rules), and it
     calls `0x00B2B810` / `0x00A64AC0` / `0x00A1B6C0` / `0x00AB3DF0` for the rest. Next: decode the
     other deal-item appliers reachable from `0x00B449F0`'s five callers.
  - Importer limit (open item 9). `0x00BC0960` builds the (index, value) pairs, sorts them (`0x00B7F790`) and
    runs `0x00BC0DC0` per route source. The split rule in `trade_split` is INFERRED, 358/362. Next: read
    `0x00BC0DC0`'s amount step (`0x00BB5730`) for an importer-side cap.
  - Naval PROVISIONALs:
    - the captives force weighting (+0xE8 of the 0xFC-stride setup records);
    - the capture crew base (card +0x70);
    - the gun counts of the 8 ship models in no vanilla file (`rules::fill_ship_guns`; the `models_naval`
      gun list is not decoded).
    A probe at the ship ctor `0x0070D600` in a battle with those ship types would settle the gun counts.
  - Desertion: the original also skips unit record classes 4 / 0xB / 0xC / 0xE — **round 14: still unnamed**,
    a bounded negative result in §Bankruptcy (do not port a guess).
  - Out of 0-B: movement path and road cost, and contact distances, belong to the pathfinding worker. The AI's
    choices are §6.
  - `save_compat` `user_saves_pass_the_checks` now fails on the user's new vanilla `auto_save.save`: an "AI
    block … holds 0 where the original never does" rule that the original itself breaks. This is a test for
    the save worker, not 0-B code.
  - The closing summary at the end of this file still shows the round 10 state. Update it from this section.
- **Round 15 (data first, Ghidra last).** `sandbox/main` 9b954e6 merged clean; **713 tests pass** (710 at
  the merge, +3). `ECON_RECOMP` untouched and still exact on all 9 saves. New tool modes: `ECON_TAXEX=1`
  (tax exemption / governing-faction exposure), `ECON_MEN=1` (recruited unit sizes), `ECON_UCOLS2=1`
  (the `num_men` value space). New tests: the two `tax_exemption_and_governorship_*` property tests and
  `recruited_unit_size_is_num_men_for_land_and_the_crew_sum_for_ships`. New code:
  `world::recruited_unit_size`. **Notes to others:** 0-E — `spawn_recruited_unit` should call
  `world::recruited_unit_size(&self.rules, &unit_key)` instead of its `same_key()` stand-in for ships
  (done on main by the port); 0-A — nothing needed, `economy.rs` is unchanged and the two
  round-14 fixes are now locked by property tests that fail if either is reverted. **Open after round 15:**
  the recruitment refund / outside-army / population cost; the record's `+0x20` desertion field; and
  `0x00B1B190`, now blocked only on the unnamed `factor+0x04` sign field and on whether the setup's
  fixed value lives in factor 21 rather than 20.

Round 7. **Evidence rule: vanilla files only** — the shipped start positions and the vanilla saves written by the
original (eur `auto_nr4_t4`, `auto_after_c8`, `auto_b2b3_0211`, `auto_nr1`, `orig_over_nr4_0252`; spa `orig_fr_t1`,
`orig_fr_t1_b`, `orig_fr_may1811`, `auto_orig_spa_0245`; copies in `target/tmp/van/`). `nr1`–`nr3` are saves written by
our port, not evidence. 0-F's effects store is merged here and owned by 0-B while 0-F is paused.

**Round 7 so far:**
- **Research** (§Research): save layout, states (0 researched CONFIRMED), availability rule (all start positions
  and saves reproduced), schools, rate formula (whole steps in every AI case), round-end step, completion, the
  `StartResearch` command, `ResearchCompleted` event, building / unit tech gates.
- **Sweep** (§Open items): recruitment queue limit (10, CONFIRMED) and the capacity rule (only the first
  `recruitment_points` items train, INFERRED); tax levels per governorship; construction side by side; stale notes
  removed; 15 open items ranked with leads.
- **Time-boxed items:**
  - **Bavaria / Württemberg settled.** A route's importer is the governing faction of its last region, and its GDP
    part takes the owners of its end regions. Trade income is now exact for every faction of all 9 original saves.
  - **Supply builder importer limit: not found.** 0x00BC0960 sorts (key, int) pairs in descending order
    (0x00B7F790) before running 0x00BC0DC0, probably the exporters; the demand-split rule stands (INFERRED).
  - **Desertion gate writer: not found** (round 6 search; open item 6).
  - **Per-building-type cost reader: not found.** No function pushes id 0x98 to the effect getters; the saved
    type-2 chain entries are read directly (INFERRED rule, 141 / 143 queued items).

**Round 10 (closing 0-B):**
- (1) **Naval autoresolve ported** (`ntw_sim::campaign::naval`, §Naval autoresolve). The ship damage state is loaded
  from `SHIP_DAMAGE_INFO`; the part hit points and sink weights come from `unit_stats_naval`. Navy-against-navy
  battles are no longer refused. The runtime ship record was not read in the time box, so the ship potential and
  morale are tested PROVISIONAL stand-ins.
- (2) **Diplomacy completed** (§Diplomacy rules, round 10):
  - computed factors: religion and government checked 506 / 506, enlightenment all, faction leader 484 / 506;
  - allies called into a war (PROVISIONAL AI decision);
  - the money of payments and tribute.
- (3) **Fortifications** are built, upgraded and repaired through `FORTIFICATION_SLOT`.
- (4) Time-boxed:
  - The garrison militia rule now uses `unit_stats_land` #70 (0-G's CONFIRMED flag), with the same results.
  - spa looting alignment shift (`0x00AAA5B0`) ported.
  - Desertion gate: faction +0x50C has no writer besides resets, so it is a loaded value (§Bankruptcy note); the
    rule stays PROVISIONAL.
  - Importer limit: no further lead.
  - spa religion drift: no other writer found; open, with evidence.
- **Closing summary**: §0-B closing summary at the end of this file.

**Round 9:** (1) the capture choice — occupy / loot / liberate, preview, apply, the AI default, building repairs
(§Capture, CONFIRMED from the exe unless tagged; UI note in UI_FIDELITY.md); (2) naval autoresolve decoded but not ported (§Naval autoresolve: it needs the runtime ship record
and the campaign ships' damage state); (3) the diplomacy rules ported (§Diplomacy rules: attitude events, the treaty actions, the per-turn update checked
against the vanilla saves); (4) the turn order made CONFIRMED (§Turn order: characters → regions → FactionTurnStart; turn end characters →
regions → units). Then the religion conversion (manager's addition: §Religion conversion) and the fortification slot (manager's addition:
`Region::fortification`, open item 18). LATER (manager): an end-to-end playability pass of the settlement panel (build,
upgrade, recruit, repair, capture screen) through the real UI.

**Round 8:** queues (recruitment capacity rule, dropped items, parallel construction, turn-start countdown after the
round-end flag) and research availability and step place made CONFIRMED from the exe (§Recruitment and construction
queues, §Research). Next: the battle consequences (open item 2).

**Round 6 (done):** public order 1180 / 1184 classes exact; region adjacency and the sea cap; trade supply split
(358 / 362 route volumes); spa node supply effect; End Turn speed (`Effects::compute_for`); construction cost (141 / 143);
desertion trigger search; region production 0 everywhere. Details in the sections below.

## Open items: PROVISIONAL / PLACEHOLDER sweep (round 7)
Every PROVISIONAL / PLACEHOLDER in `crates/ntw_sim/src/campaign` and `crates/ntw_campaign/src` (outside the save writer,
pathing / zoc / embark, characters.rs and names), ranked by gameplay impact.

**Resolved this round:**
- **Recruitment queue.** A queue holds 10 items (`0x00B62040`: full when its count is above 9, CONFIRMED). Only the
  first `recruitment_points` items train; the rest wait (INFERRED from the saves: Cleves with 1 point has 1 started
  and 5 waiting; Gibraltar with 6 points has 6 started of 10). Before, the model refused to queue beyond the points
  and advanced everything.
- **Tax levels.** These are per governorship (`GOVERNORSHIP_TAXES`), loaded from the file, and every faction of
  the shipped campaigns has one governorship. `SetTaxLevel` now also updates the governorship record. The
  PLACEHOLDER notes were stale.
- **Construction:** several items of a region progress side by side (INFERRED from the saves).
- **Stale notes removed:** trade volumes as loaded, effects not modelled, GDP / growth not recomputed, accumulated
  trade starting at 0, routes not built, the garrison flag, the public-order class note.

**Round 8, made CONFIRMED from the exe** (§Recruitment and construction queues, §Research):
- recruitment: only the first `recruitment_points` items train (`0x00B71FB0`); items whose unit the region can no
  longer recruit are dropped;
- construction: every slot in one pass (`0x00A78670`);
- the queues count down at the faction turn start (`0x008F2620` → `0x00AAE820`), after a round end has flagged
  the items (`0x00A78620`);
- research availability (`0x008F91F0`), also run at the faction's turn start.
- capture: the settlement's garrison army destroyed, every queue cleared (`0x00B58560` / `0x00B58890` / `0x00B58C00`).
- naval recruitment capacity: per port, the int `naval_recruitment_points` of the port's own building (`0x00B61EE0`).

**Open, ranked** (impact; lead):

| # | Item (file) | Impact | Lead |
|---|---|---|---|
| 1 | Phase order inside a turn: **CONFIRMED round 9** (§Turn order): turn start `0x008F2620`, turn end `0x008BD0F0`, round-end economy `0x008BC650`. Left: the unmodelled calls listed there (character pools, pending orders, forces in transit, building spawns `0x00A249A0`), the calendar step's place. | low | §Turn order |
| 2 | Battle consequences (battles.rs:13, :171). CONFIRMED round 8: on capture the settlement's garrison army is destroyed and every queue is cleared (0x00B58560 / 0x00B58890 / 0x00B58C00 → 0x00B1A760, 0x00A6CBE0). Left: a real-time battle's casualty spread, the capture / wound of generals. | medium | the post-battle applier after autoresolve (callers of `0x0070D370`) |
| 2b | Capture choice: **done round 9** (§Capture); fortification roll and the spa looting alignment shift added round 10. Left: the AI's choice (`0x00AAABA0`, §6; PROVISIONAL occupy), the liberated faction's new army (`0x00B4F090`), the alive-faction liberation branch (faction +0x824 / +0x72C), which capture variant sets the surrender flag (`0x00B58C00`'s caller), the spa guerrilla branch. | medium | `0x00B4F090` (army), callers of vtable `0x0137CA30` slot 11 |
| 3 | Diplomacy: **rules ported rounds 9–10, government drift rounds 12-13 (corrected), the row key settled round 14** (§Diplomacy rules): treaties, war / peace, gifts, access, per-turn update, computed factors, allies called, treaty money, the drift on a government change. The `diplomatic_relations_government_type` row is **pair-keyed, CONFIRMED round 14** (bytes + shipped rows + the save), so the map is right. Left: France's faction-leader factor (model −9, saves +9: the leader post's ministerial level, item 11), the AI's join / accept decisions (§6), `allied_with_enemies`, peace terms (regions), the government change's own-side drift/limit pass `0x00B1B190` (not ported: its sign test reads an unnamed factor sub-field, and its literal argument order contradicts `0x00B1B5A0`). | low | `0x00B0CE30`; item 11; `0x00B1B190` (name `factor + 0x04`, re-read the two pushes) |
| 4 | Movement path and road cost (commands.rs:34, rules.rs:296). | high | pathing worker (not 0-B) |
| 5 | Contact / attack distances (commands.rs:302, `CONTACT_DISTANCE` at commands.rs:1002). | medium | zone-of-control / area queries (0x00BA6840 region area, 0x009D3A60) |
| 6 | Desertion gate (economy.rs): faction +0x50C. Round 10: every instruction using displacement 0x50C in 0x00800000..0x00C40000 was listed. The faction field is only reset (constructors) and read by the gate `0x008AE710` (> 1) and by `0x0089DFE0` (a script query); the INC at `0x009DA210` is a character's +0x50C. So it is a loaded or script-set value; the model keeps the bankrupt-turn count as the gate (PROVISIONAL, tested). **Round 14 and round 15 both closed the four skipped unit classes as UNKNOWN** (§Bankruptcy: the field is `*(unit+0x48)+0x20` on the unit *stats record*, not `LAND_UNIT+0x20`; it is not `num_men`; it has one other reader, a named-argument property getter). | low | the FACTION record field that loads +0x50C (save-compat); the record's `+0x20` field, whose only other reader is the vtable-slot-0 getter `0x008F6810` |
| 7 | Recruitment details: **round 15 closed the size and confirmed placement/garrison join was already ported** (§Recruitment spawn, round 15) — `num_men` for land and the crew-triple sum for ships, exact on all 3174 saved units; `0x00B71FB0` confirmed to create no units. | medium | full refund (:823), the new army waits outside (:913), the population cost (the switch to `world::recruited_unit_size` is done on main) |
| 8 | Naval autoresolve: **ported round 10** (§Naval autoresolve). Left: the runtime ship record (the real potential `(r+0x1D8 + r+0x1DC) × (model+0x7C >> 1) × 3` and morale +0x1C8; PROVISIONAL stand-ins), ship captures (`0x0074F710` / `0x0075C090` / `0x007CD570`), the 0.95 factor of class 0xB, ships carrying armies, the autoresolve seed rule (autoresolve.rs:21). | medium | the battle ship object (`0x006B1530`'s +0x204), captives `0x0075C090` |
| 9 | Trade: blockade area radius (trade.rs:49), sea cap fallback (:315), route candidate set (:349, world.rs:319), textile demand drivers (:491), partners needing a path (economy.rs:191). | medium | area `0x00BC4860` / `0x00A8BDE0`; candidates 0x00BC0960 |
| 10 | Theatres (economy.rs:114, effects.rs:381): one per campaign in the shipped files. | low | theatre object, faction +0x734 |
| 11 | Effects: army container (effects.rs:392), minister base attribute (:416), difficulty front end (:440), unmapped junction kinds (:463, :20). | low | `0x008AFC00`; `0x008D9D70` |
| 12 | Research: army / navy rate mods (research.rs:100). | low | the string test in `0x008EA9F0` (tech record "army-admin" / "navy-admin") |
| 13 | Region recompute omissions (economy.rs:373): `slots_gdp_values` and the commodity terms (0 in the shipped data); **round 14 closed the two that were real** — the tax-exempt guard and the governing faction (§GDP). Chain modifiers and the home-region bonus are modelled. | low | 0x00A6AFC0 |
| 14 | World bookkeeping: new id allocation (world.rs:387, :396), data loaded for the UI (:347), embark link (:358), id step (:365). | low | id allocator (save-compat) |
| 15 | Others' areas: CAI beliefs (cai_world.rs:20, AI); grid obstacle shapes (grid_obstacle.rs:31, :82, pathing); government type for an unknown key (error.rs:132, world.rs:338); S1 leftovers (details.rs:323). | low | their owners |
| 16 | Region religion: **conversion ported round 9**; spa looting shift round 10. Left: the spa drift with no source in the only vanilla pair (3 turns). No script or other breakdown writer was found: `0x00A4C9B0` is the population constructor and `0x00AA9C10` only renormalises. Population growth is also left. | low | more spa saves over consecutive turns |
| 17 | Repairs (round 9, §Capture): the repair item's per-turn health step and the cancel refund are INFERRED; the model saves a repair as an ordinary construction item of the same level (the original's item type 2, `0x00AE7010`; save-compat). | low | `0x00AE7010` (vtable of the repair item) |
| 18 | Fortifications: loaded (round 9), **built, upgraded and repaired round 10** through `FORTIFICATION_SLOT` (slot type `settlement_fortification`, repair cost from the fort branch of `0x00B66410`: round(cost × (1 − strength))). Left: their effects, the save writer (save-compat). No vanilla file has one. | low | — |

## How the exe holds `campaign_variables` (CONFIRMED)
- 110 (0x6E) keys registered at startup (0x00432DC0..0x00433432: `PUSH "name"; MOV ECX,obj; CALL 0x004F5500`), key
  objects 8 bytes each from 0x0164B550; index = registration order.
- The DB table is copied into a 110-float array (0x00E214B0), the campaign's `campaigns_campaign_variables_junctions`
  overrides are applied (0x009D4090), and the array is copied into the campaign model (constructor 0x008742C0) at
  model+0xDA0. Getters `float model::variable(idx)`: 0x008B25E0 (x87) / 0x008B25F0 (SSE), 113 call sites.
- Index (selected): 0 tax_efficiency_regions_minimum, 1 tax_efficiency_region_modifier, 10–13 road_level_0..3, 14
  baseline_pop_growth, 18–20 policing_*, 43 faction_gdp_other, 44 base_wealth_increase, 45–80 autoresolve_*, 73
  unit_minimum_strength, 81 character_research_points_cap, 82 losing_unit_minimum_strength, 83 tax_efficiency_log_base,
  84 tax_efficiency_modifier, 85 tax_efficiency_total_regions, 86 faction_gdp_other_minor, 87–89 settlement_looting_*,
  90–99 character_recruitment_*, 100 maximum_attrition_pct, 101 passive_spying_region_bonus, 102
  general_admiral_action_point_bonus, 103–105 gentleman_happiness_*, 106 settlement_looting_base_loot, 107
  town_wealth_growth_discontent_reduction, 108–109 settlement_looting_*publicorder_reduction.
- `maximum_town_wealth_level` and `happiness_war_*` are in the DB table but not registered: the exe never reads them
  through this table (INFERRED unused).
- Getter readers by variable: 0 / 83 / 84 / 85 → 0x00BC73C0 (tax efficiency); 43 / 86 → 0x008C3110, 0x00BBC710; 107 →
  0x00AB42F0; 44 → 0x00AB4410 (town wealth levels); 19 → 0x008B17D0; 20 → 0x008E2B00, 0x008EDF70; 102 → 0x008C3170;
  82 → 0x008D2260; 73 → 0x008F68F0; 90–99 → 0x00A17A90 / 0x00A17AF0 / 0x00A1BB20. **None of the autoresolve variables
  (45–80) is read through the getter**: autoresolve reads them some other way (open).

## Effect ids (CONFIRMED)
The engine's effect enum is a pointer table at 0x0145B4A8 (`ptrs:` mode); effect getters 0x00E1EFD0 (float) /
0x00E1F130 (int) take the index. Selected: 4 gdp_farm, 7 gdp_mod_all, 9–18 happiness_*, 22 policing_cost_mod, 32
recruitment_points, 35–38 repression_*, 46 tax_bonus_building, 47 tax_bonus_character, 48 tax_cap, 51–61 tw_growth_*,
62 upkeep_cost_mod_land_all, 63 upkeep_cost_mod_naval_all, 110 tax_bonus_minister, 124 tax_bonus_technology, 125
admin_cost_mod, 128 general_admiral_action_point_bonus, 130 tw_growth_factionwide.
Readers (from the call sites): happiness_* / repression_* → 0x008EDF70; repression_policing_cap → 0x008E2B00;
policing_cost_mod and pop_growth_* → 0x00AA9C10; upkeep_cost_mod_land_all → 0x008F9B10; naval → 0x008B21D0;
recruitment_points(_home_region) → 0x00B61F30; general_admiral_action_point_bonus → 0x008C3170.

## Economy (CONFIRMED unless tagged)
**REGION fields** (writer 0x00A51E30, child index → region offset): #9 +0xB8 base GDP, #10 +0xBC GDP, #11 +0xC0,
#12 +0xCC town wealth, #13 +0xD0 town wealth at the last wealth level, #14 +0xD4 stored town wealth, #15 +0xD8 town
wealth growth (i32), #16 +0xE0 wealth level count, #17 +0xEC, #18 +0xF0 (= −town_wealth_growth_discontent_reduction
while the region has unrest, i32), #19 +0xE4 tax exempt (bool).

**Taxes:**
- Tax efficiency (0x00BC73C0): `n = max(min, regions + admin_cost_mod)` with `min = round(var 0)`;
  `e = sqrt(ln((n − min)/var85 + 1) / ln(var83)) × var84 × 0.1` (`ln` = 0x012710FD, the SSE logf). Regions = the
  faction's region list count (faction +0x76C list). Shipped values: 12 regions → −0.2278.
- Effective class rate (0x00BA4210): 0 if the region is exempt, else
  `max(0, (1 + e) × (rate/100 + 0.5 × charbonus/100 + 0.5 × building/100 + 0.5 × tech/100))`; `rate` = the
  governorship's u8 rate (`GOVERNORSHIP_TAXES` +8 lower, +9 upper); building = `tax_bonus_building` of the region
  (0x00BC7540); tech = faction `tax_bonus_technology` (0x00BC75F0); charbonus = faction `tax_bonus_minister` when the
  region is in the faction's home (faction +0x734 equals the region's 0x008F4100 value; INFERRED "home theatre"), else
  `tax_bonus_character` of the region's governor object (0x00BC7580).
- Region taxes: lower 0x00A8C2E0, upper 0x00AB5560: `round(rate × (GDP + town wealth))` (FISTP, ties to even); 0
  without a governorship (region +0x248).
- Faction taxes = Σ over the faction's region list (0x008D22F0 lower + 0x008F9E50 upper). Faction GDP shown in the UI
  (0x008C3110) = var43 + Σ region GDP.
- Other income = var43, or var86 when faction +0x524 is 0 (0x00BBC710). The startpos shows 2000 (var86) for the
  minor factions and 1700 for France, Austria, Britain, Prussia, Russia, Ottomans, Spain. The flag is the `FACTION`
  bool right before `CHARACTER_ARRAY` (read there by the loader 0x0087A190; set exactly for those seven), CONFIRMED;
  the `factions` category does not match it. The mp_eur startpos stores 1500 for its minors (the DB base value) although
  that campaign overrides `faction_gdp_other_minor` to 1700: stale build-time data (the loader recomputes it).

**FACTION_ECONOMICS** (economics object: 10 history records of 25 i32 at +4, current index +0x3F0, count +0x3EC,
treasury +0x3F4, cannot-pay flag +0x464, cannot-pay turns +0x460). ESF `ECONOMICS_DATA` = the 25 categories in groups
5/3/4/1/5/7: income = categories 5..11 (5 taxes, 6 ?, 7 trade 0x00BB3490, 11 other income), expenses = 18..24 (19 land
upkeep, 20 naval upkeep; set through 0x0088F950 / 0x0088F990 → 0x00BBE930); the other groups are one-off spending and
refunds (0x00BAF500, 0x00BAF660, 0x00BB3810, 0x00BBE9A0).

**Round end** (0x00948CF0, when the turn passes the last faction): for every faction 0x008E1030 (region pre-pass,
upkeep = Σ forces 0x008B2150 → land / naval, then the cannot-pay flag `treasury + income < expenses`, 0x00BBC7D0),
then for every faction 0x008BC650 (0x00BABE30: if it can pay, treasury += income − expenses; the record goes to the
history; then each region 0x00AB42F0: GDP / town wealth recomputed by 0x00A6AFC0 and `tw = max(0, tw + growth)`, town
wealth levels), one more 0x008BC650 (INFERRED the rebels), then 0x00BCB020 (trade). So **income is paid at the round
end for all factions**, not at each faction's turn start.

**Upkeep** (spec, not in code yet): per unit `round(upkeep × (100 + mod) × 0.01)` with mod = faction
`upkeep_cost_mod_land_all` (land, 0x008F9B10) or `upkeep_cost_mod_naval_all` (ships, 0x008B21D0) + two
unit-category effects (0x00E1F0F0 / 0x00E1F110 on unit record +0x24 / +0x28) (+ guerrilla / auxiliary mods in the
Peninsular campaign); 0 if the sum is negative. Recruitment cost: `round(cost × (100 + max(−100,
recruitment_mod_cost_* + category effects)) × 0.01)` (0x008B15A0). Faction effects (techs, difficulty handicaps,
buildings) are not modelled, so upkeep stays the plain `units` column.

## Public order (0x00AA9C10 per region → 0x008EDF70 per social class)
Each social class of the region's `POPULATION` (stride 0x78) gets 13 happiness factors and 6 repression factors;
the class's public order = positive sum + negative sum + repression total (0x008E2AB0). CONFIRMED factor sources:
[1] government-type happiness of the class (effect 9 lower / 11 upper), [2] tax happiness of the class (10 / 12),
[4] regional + factional events (15 + 14), [5] / [6] class-keyed building effects (0x00E1F050 type 2 / 5),
[7] class-keyed type 0 + ministerial position (16) + character traits / ancillaries (13), [9] education
(0x008B1A70), [3] or [13] religion (0x008B1B10), [11] war results (+12 if 0x00AA10F0 and none), [12] gentlemen
(0x008C55D0); repression [16] government type (36), [17] ministers (37), [18] government building (35), [19]
automated policing, [20] garrison (0x008B18F0), [21] set elsewhere. Garrison: `round(clamp(units × f(pop), 0,
policing_garrison_cap))` with f = 1.5 / 1.25 / 1.0 / 0.75 / 0.5 / 0.3 for populations below 9 999 / 49 999 /
249 999 / 1 000 000 / 10 000 000 / above (a unit counts twice when unit record +0x136 is set: militia, INFERRED, see round 6).
Automated policing: if the net is negative, `min(|net|, max(0, policing_automated_cap + repression_policing_cap))`.
Policing cost (faction +0x6E0 set): `round(policing_automated_cost_per_pop × Σ policing × sqrt(population) ×
max(0, 1 + policing_cost_mod/100))` (0x00AA9C10, not modelled).

Other sources read in round 3, all CONFIRMED formulas but not modelled because the model lacks their data:
- **Education** [9] (0x008B1A70): only in the capital, or where effect 39 ≠ 0. The value is the class's base
  (`+0x70`) + `round((1 + education_happy_mod/100) × class-keyed category-1 effect)`.
- **Religion** [3]/[13] (0x008B1B10). Per religion share s of the class, `s × 100 × religion factor`, minus the faction's
  religion effect (0x00E1F030). Then `× (1 + happiness_mod_religious_unrest/100)` (0 if negative), rounded and
  negated. Needs the regions' religion shares (`POPULATION` / `TRAITS`, not loaded).
- **Gentlemen** [12] (0x008C55D0). Each gentleman in the region (in `spa_napoleon` also the three missionary types)
  gives `ceil(x / gentleman_happiness_[103]) + gentleman_happiness_bonus` (effect 129). x = 0x00A198D0, a character
  value: UNKNOWN, not modelled. The total is positive for the class's own side and negative otherwise. The positive
  part is clamped to [0, var 104]; the negative part is floored at var 105.
- **War results** [11]: 0x00AA10F0 is true when a character of another faction stands in the area query of the
  region (0x00BA6840) and passes 0x008CE9B0 (hostile).

The other sources (ministers, character traits and ancillaries, events) come from the effects system (slot 0-F).

**Round 4: the four sources are modelled** (code `economy::public_order`, `religion_factor`, `gentlemen_factor`;
test `religion_gentlemen_war_and_education_in_public_order`).
- Data loaded per region:
  - `POPULATION/REGION_FACTORS/RELIGION_BREAKDOWN`: (religion, share) → `Region::religions`;
  - per population class (`POPULATION CLASSES`, `POPULATION_CLASS` v3): #11 the education base (class +0x70) and #12
    the war-results base (class +0x74) → `Region::class_bases`.
- The class record also holds the original's computed factors: #1 i32[13] happiness factors [1..13], #2 i32[6]
  repression [16..21], #3 positive sum, #4 negative sum, #5 repression total. These are CONFIRMED by 0x008EDF70's
  layout. Round 6 compares the model with them on the vanilla saves (§Public order, round 6).
- **Education** [9] (CONFIRMED): only in the owner's capital or a region with research points. The value is
  `base + round((1 + education_happy_mod/100) × clamour)`. Clamour is the class-keyed `happiness_clamour_for_reform`
  effect (category 1); the model routes the buildings' `happy_clamour_for_reform_*` effects here instead of summing
  them everywhere.
- **Religion** [3] (CONFIRMED on the vanilla eur save, round 6): `−round(max(0, 100 + happiness_mod_religious_unrest)
  / 100 × (Σ share × 100 × value − conversion))`, with `value` = `diplomatic_relations_religion` #3 for (region religion,
  state religion). A missing pair counts 0. The `conversion` effect (type 7) is not modelled.
- **Gentlemen** [12] (CONFIRMED formula; skill INFERRED): each gentleman in the region gives
  `ceil(research attribute / gentleman_happiness_divisor) + gentleman_happiness_bonus`.
  - The sign is positive for the owner's own gentlemen and negative for others'.
  - The positive part is clamped to [0, positive limit]; the negative part is floored at the negative limit.
  - Round 6: the skill 0x00A198D0 is 0 on the vanilla saves; only the bonus (the faction's effect 129) counts.
- **War results** [11] (CONFIRMED): the class's stored value, plus 12 when that value is 0 and a character of a faction
  at war with the owner stands in the region (0x00AA10F0), except in `spa_napoleon`.
- "In the region" = garrisoned there, else the path grid's region at the character's position (round 6), else within
  20 map units of the settlement when no terrain is loaded (PROVISIONAL fallback).

**Round 5: faction-wide public order effects.** The owner's faction sum from the effects store (`Effects::faction_sum`,
which is the same as `Effects::compute(..).faction` but costs about 1/40 of it) adds to each class:
- the class-keyed `happiness_*` entries (traits and ancillaries of ministers, techs, events, scripted bonuses, the
  difficulty handicap), with `happiness_clamour_for_reform` going to education;
- the basic `repression_gov_type`, `repression_ministers` and `repression_gov_building`.

The government set is taken out because the DB government rows are already summed. Which of the factor slots
[4] / [5] / [6] / [7] each entry fills is not tracked (only the sums matter). Test
`faction_wide_effects_reach_public_order`.

**Round 6: the slots against the vanilla saves** (tool `ECON_MAP=1 ECON_POSUM=1 economy_check`, which compares every
class the save holds, all-zero ones included). **1180 of 1184 classes are exact in all 19 slots** over 8 vanilla saves:
eur `auto_nr4_t4`, `auto_after_c8` and `auto_b2b3_0211` 144/144, `auto_nr1` and `orig_over_nr4_0252` 142/144, spa
`orig_fr_t1`, `orig_fr_may1811` and `auto_orig_spa_0245` 62/62. Rules settled on the way:
- **Classes per government** (CONFIRMED, `government_types`, `sbbiss`: key, two UNKNOWN flags, an UNKNOWN int, upper
  class #4, lower class #5). Absolute monarchy governs `upper` / `lower`, constitutional monarchy `upper` / `middle`,
  republic and empire `middle` / `lower`. `0x008CE210` compares the class with the government's lower class
  (government +0xB0 → +0x1C): equal → effects 9 / 10, otherwise 11 / 12. A class that is neither the lower nor the
  upper class gets nothing (`0x008CE230` / `0x008CE2F0`): the saves store zeros there (France's `upper` under the
  empire, Britain's `lower` in spa). `economy::public_order` returns the totals of the government's two classes.
- **Slot holders** (CONFIRMED, `0x00A62380` → `0x00A91FC0`): a building adds its local and faction-wide effects only
  if its health is > 99 and the faction holding its slot (`REGION_SLOT` #0 `GARRISON_RESIDENCE` #0, a faction id) is
  the region's owner. Loaded as `RegionSlot::holder`; `Region::effect_buildings`. Evidence: `eur_bavaria` in
  `auto_after_c8`, owned by Austria, keeps four slots held by Bavaria; its tax office (`repression_gov_building` 1)
  is missing from the stored factors, while the Austrian-held town's gentlemen's club counts. On capture the
  slots the old owner held pass to the new one (INFERRED).
- **Governing faction** (INFERRED, one region): the region's faction part (government, ministers, technologies, tax
  levels) comes from the faction whose governorship lists the region (`GOVERNORSHIP` #2), normally the owner. The
  same `eur_bavaria` is still in the dead Bavaria's governorship and shows Bavaria's government and none of Austria's
  minister effects (`happiness_character` −2 elsewhere in Austria). `World::governing_faction`; on capture the
  region moves to the new owner's governorship.
- **Gentlemen** (INFERRED): the skill `0x00A198D0` contributes 0 in every vanilla save (own gentlemen in their
  capitals add nothing), so each gentleman gives his faction's `gentleman_happiness_bonus` (effect 129).
- **War results**: only armies count (a navy does not: the British fleet off Rotterdam in `auto_nr4_t4` leaves
  Holland at 0), and "in the region" uses the path grid's region lookup when terrain is loaded.
- **Garrison**: only the settlement's garrison army counts (`0x00B14880` → `0x008B1C30`), and a militia unit
  (`infantry_militia`) counts twice. The doubling matters in about 80 garrisons across the saves and holds in all of
  them except Rumelia (below). The `units` table has no column that marks the levies, so +0x136 stays INFERRED as the
  militia class. Tax-exempt regions get no tax factors.
- **The 4 misses are timing.** `auto_nr1` Provence stores garrison 2, but its garrison has left the settlement (none in
  the save). `orig_over_nr4_0252` Rumelia stores 4 for 8 units including two new Ottoman levies; the model gives 5.
  The stored value was most likely computed before the second levy arrived.

**Recruitment points** (0x00B61F30, CONFIRMED; code `CampaignModel::recruitment_points`). Land recruitment points
are built up from:
- the region's int effect `recruitment_points` (32, rounded; the region's effects are its buildings' and the owner's
  faction-wide effects, 0x00A67530);
- plus `recruitment_points_home_region` (33) if the region is the owner's capital (0x00A8B5A0: region == faction
  +0x72C);
- plus 1 for an AI France in `mp_eur_napoleon` (hard-coded).

Faction-wide effects are 0 until 0-F's effects are wired. The naval reader is not found; the sum is PROVISIONAL.
Recruitment cost and upkeep modifiers: spec in §Economy; with the effects at 0 they equal the plain DB values.
Construction: the basic effect enum has no construction cost or time effect. The cost takes the chain-keyed and
local cost effects (round 6, §Construction cost); the `building_levels` turns are used as they are (INFERRED). **Recruitment time** = `units` #6 (CONFIRMED, round 4): a new land recruitment item (`0x00B58DD0` → `0x00AECEE0` →
base `0x00AF3F80`) takes its turns (item +0x1C = `RECRUITMENT_ITEM` #3) from `UNIT_RECORD` +0x34. The record keeps
the builder's ints 4 bytes lower: upkeep is builder +0x40 and is read at record +0x3C by 0x008F9B10. So +0x34 is
builder +0x38 = column #6 (infantry 2, artillery 3, cavalry 4, ships 5–8). The vanilla saves agree: of 43 queued land
items none has more turns left than its unit's #6, and 25 have exactly #6 (just queued).

## GDP and town wealth growth (0x00A6AFC0, CONFIRMED; code `economy::recompute_region`)
Recomputed for every owned region at the round end (then `tw += growth`, 0x00AB4410):
- `GDP = base GDP (#9) + Σ slots Σ_{gdp_farm, gdp_industry, gdp_mine, gdp_port} trunc(round(effect) × factor)`,
  `factor = slot factor (slots_gdp_values, 1.0 shipped) × (1 + gdp_mod_all/100 + chain mod)`. Only buildings at
  health ≥ 100 count. The DB key → engine effect mapping is `effect_bonus_value_basic_junction` (`gdp_port` →
  gdp_port_trade, `tw_growth_taxes_modifier` → tw_growth_tax_modifier, ...).
- `growth = tw_growth_factionwide + #18 + Σ slots Σ_{education, government, home_region, industry, port, roads}
  trunc(effect × (1 + tw_growth_mod_all/100)) + tw_growth_technologies`, then
  `growth = trunc(trunc((m − #17) × growth) + growth + fixed)` with `m` = `tw_growth_taxes_modifier` (only when
  growth ≥ 0) and `fixed` = `tw_growth_taxes_fixed`, **both classes' tax effects summed** (normal: −0.3/−3 upper,
  −0.25/−2 lower).
- Checked: GDP and growth of all 238 regions of eur, mp_eur, egy, ita, spa and tut start positions are reproduced
  exactly with the shipped DB (`tests/economy_fidelity.rs`).
- **Round 14: two sources in this recompute were wrong, and they were the last growth mismatches**
  (Wallachia, Moravia, Bavaria; CONFIRMED against the stored values, no RE). Fixed in `economy::region_effect`
  and `economy::faction_part`:
  1. **A tax-exempt region takes no tax bundle at all** (`REGION` #19, region +0xE4). `region_effect_set` already
     had this guard for the public order; the GDP / growth path did not. `eur_moravia` (Austria) and
     `eur_wallachia` (the Ottomans) are both tax-exempt, so their stored growth is the raw sum — 5 and 0 —
     while the `tax_normal` bundles over both classes (−0.55 modifier, −5 fixed) must not be subtracted (the model
     gave −2 and −5).
  2. **The government and the faction-wide part are the governing faction's, not the owner's** — the same set
     `0x00A67530` builds that `region_effect_set` already used for the public order. `eur_bavaria` is owned by
     Austria but governed by the landless Bavaria (round 7), so it reads Bavaria's government, Bavaria's tax
     levels and Bavaria's faction-wide sum (no technologies, `gdp_mod_all` 0), not Austria's
     (`tw_growth_technologies` 5, `gdp_mod_all` 3): with Austria's part the GDP is 1277 and the growth 0,
     Bavaria's gives the stored **1275 / −3**. Every building in the region's slots still counts whoever holds
     the slot (CONFIRMED by the same 1275 = 1200 + 75).
  Result: `ECON_RECOMP` is now exact on all 9 vanilla saves — **GDP and growth 72/72** on the five eur saves and
  **31/31** on the four spa ones (before: growth 69–72/72 and GDP 71–72/72 on three eur saves). The 238
  start-position regions are unchanged. New test
  `economy_fidelity::gdp_and_growth_match_the_vanilla_saves` (484 region records, `NTW_EVIDENCE_DIR`) plus three
  unit tests in `ntw_sim`, one per region; tool mode `ECON_RGROWTH=<key filter>` prints every term.
- **Round 15: both fixes stated as properties, because the evidence that found them cannot generalise.**
  New tool mode `ECON_TAXEX=1` prints every region with `REGION` #19, its owner and the faction that
  actually **governs** it. The exposure, over the six start positions and the nine vanilla saves:
  **2 tax-exempt regions** (`eur_moravia`, `eur_wallachia`, both eur, both in the later eur saves) and
  **1 region governed by another faction** (`eur_bavaria`, in `auto_after_c8` and
  `orig_over_nr4_0252`). `auto_b2b3_0211`, `auto_nr1` and all four spa saves contain **neither**. So the
  nine saves cannot distinguish "the rule is a per-region flag read from `REGION` #19 and a governing
  faction read from the governorship" from "the rule happens to hold for these three regions".
  Two tests replace that with a sweep (`crates/ntw_campaign/tests/economy_fidelity.rs`):
  - `reference_region(model, fx, region, governor, exempt)` re-implements the recompute from its own doc
    comment, taking the **governing faction and the exempt flag as parameters** rather than looking
    them up. An implementation that read `region.owner`, or that keyed the exemption on a region name,
    therefore cannot agree with it.
  - `tax_exemption_and_governorship_hold_for_every_region_and_faction_pair` — **25260** shapes: for
    every owned region of 9 files, every live faction as the governor and both flag settings. The
    governorship listings are rewritten to build each shape (`set_governor`, clearing every other
    listing first and creating a governorship on a faction that lacks one) and the rewrite is
    **asserted to have taken** (`World::governing_faction` returns the intended faction), so a case
    cannot pass vacuously. Vacuity is guarded on the other side too: 12630 of the exemption flips
    change the growth and 18728 foreign governors change the answer, and 12194 shapes are both
    tax-exempt and foreign-governed — a combination **no shipped file has**.
  - `tax_exemption_and_governorship_hold_without_the_faction_wide_effects` — the same two rules in the
    effect-free state the start positions were computed in (1518 shapes). **Not ported to main:** it
    passes the computed faction effects (`Effects::compute`) like the sweep above, over the same start
    positions, so it is a subset of the sweep rather than an effect-free check.
  - **Mutation-checked.** Temporarily reintroducing either round-14 bug in the reference — dropping the
    `!exempt` guard, or reading `region.owner` instead of the governor — fails both tests; the switches
    used to show that are removed from the committed code. `economy.rs` (0-A's) is untouched.
  - One effect of the sweep worth keeping: rewriting a governorship does **not** change any faction's
    effect set (`faction_set` sums regions by `r.owner`), it only decides which faction the recompute
    queries. The tests assert that rather than assume it, which is what makes one `Effects::compute`
    per file safe.
- Not modelled: `slots_gdp_values` (0 / 1.0 in the shipped DB), building chain modifiers (technologies), the commodity
  terms (#29 exported, #31 produced: 0 for every shipped region), home-region bonus, town wealth
  levels (minor settlement growth: `base_wealth_increase × (n(n+1)(n+2)/6 + 1)`, 0x00AB4410).
- **Faction-wide effects** (round 5, item 4).
  - 0x00A6AFC0 reads `gdp_mod_all` (effect 7), `tw_growth_mod_all` (0x38), `tw_growth_factionwide` (0x82, int),
    `tw_growth_technologies` (0x3D) and the tax growth effects (0x3B / 0x3C) from the set 0x00A67530 builds. That set
    is the region's buildings plus the **governing** faction's sum (round 14: the owner unless another faction's
    governorship lists the region, `eur_bavaria`). So techs, faction-wide buildings, ministers,
    the saved base (#54) and the difficulty handicap (#55: AI `gdp_mod_all` +10 at normal) all count.
  - Why the start positions match without them: their stored GDP / growth were computed before any faction container
    existed. At a start position #55 = #54 and the techs, scripted bonuses and handicaps have not been summed yet. 82
    of the 238 checked regions have a non-zero faction-wide `gdp_mod_all` or `tw_growth_*` (eur 26 of 72, e.g.
    England `gdp_mod_all` 3, `tw_growth_mod_all` 10; spa 31/31; ita 25/25), and every one stores the value without it
    (`economy_check` ECON_FXGDP).
  - So the effects apply from the first round end on: the first recompute (0x00AB4410 at the end of round 1). That is
    INFERRED from the start-position data plus the CONFIRMED reader.
  - Code: `economy::recompute_region_with(model, Some(&fx), region)` at the round end; `recompute_region` (no faction
    part) reproduces the start positions.

## Trade (0x00BB3490 / 0x00B15B50, CONFIRMED; code `economy::trade_routes_value`, `campaign::trade`)
- Faction trade income (0x00BB3490) = Σ over its international routes of the route total (0x00B79B20). Hard-coded
  extra (CONFIRMED): if the campaign is `spa_napoleon` and the faction is `spa_france`, it also earns
  `Σ domestic volume × price` (over domestic routes whose last waypoint passes 0x00A8B5A0, meaning UNKNOWN; we take
  it as "not blockaded") minus `Σ volume × price` over its international routes (`trade_home_value`).
- Route total (0x00B15B50), recomputed per route. The previous accumulated value is copied into the value (+0x38 =
  +0x3C). Then:
  - **blockaded** (0x00B12050) → total 0;
  - otherwise total = commodity part `Σ volume[c] × price[c]` (+0x2C) + resource part (+0x30) + GDP part
    `trunc(var3 × sqrt(GDP_a + GDP_b))` (+0x34; 0x00B3F8F0, var3 = 2.2) + accumulated value.

  Prices are `CAMPAIGN_TRADE_MANAGER` child #4 (u32[8], manager +0xBC). This is CONFIRMED: Σ volume × price equals
  the stored commodity part of every route in the start positions and the three vanilla saves (72/72 eur, 6/6
  and 6/6 spa) (`economy_check`
  ECON_ROUTES), and 0x00BB3490 reads the same array. The resource part is 0 in every file. Total = sum of the parts
  in every file.
- Faction GDP (0x008C3110) = **`faction_gdp_other` (var 43, also for minors)** + Σ region GDP.
- Accumulated value += `trunc((1 + trade_route_all_mod_growth_rate/100) × var4 × (commodity + resource + GDP))` at
  each round end (0x00B05CC0, var4 = 0.005). A blockaded route adds 0. Stored per route at +0x3C (save layout below).
- **Routes** (loaded from the file, `World::trade_paths` by (exporter, importer)): the waypoints {region, network
  node from/to, sea hop} and the commodity volumes. Network nodes are 0..38 ports, 39..110 settlements and 111..
  off-map trade nodes (`PORT_INDICES` / `SETTLEMENT_INDICES` / `TRADE_NODES`). The importer is the owner of the last
  waypoint's region (INFERRED; SAVE_COMPAT §12). A route counts while the pair has a trade agreement and is not at
  war. A pair without a loaded path (an agreement made in play) trades its GDP part only. PROVISIONAL: the route
  builder 0x00BC0960 / 0x00BC0DC0 (a greedy cheapest-path assignment with `trade_route_land_sea_bias`, port
  capacities and caps) is not ported.
- **Domestic routes** (`World::domestic_trade`): trade node → own port, carrying the commodities the faction's trade
  ships gather. Their volumes add up to what the faction's international routes carry (Britain in eur: 37 of
  commodities 1, 5 and 7, spread 21/4/8/4 over its four partners). When a domestic route is blockaded, the model
  scales the export volumes down to the remaining supply (PROVISIONAL stand-in for the route builder's re-spread).
- **Blockade** (0x00B12050, CONFIRMED structure). A route is blockaded when, for any waypoint:
  - the waypoint is a sea hop and 0x00BC4860 finds a navy that is hostile (0x00A67430; INFERRED at war with the
    exporter) and stands in the area of the hop's `from` or `to` node (0x00A8BDE0 area `+0x14` == node);
  - or the waypoint's object (port / settlement) answers vtable `+0x98` (blockaded / besieged).

  Model: a hostile navy within 1 map unit of the node's position (PROVISIONAL: the area shapes are not read). In the
  vanilla eur save a British navy 1.17 units off Rotterdam does not blockade the Dutch sea routes. Sieges are not
  modelled.
- Checked: the computed trade income equals the stored figure for **all 83 factions** of the shipped start positions
  (`economy_fidelity`). Round 7: it also equals it for **every faction of all 9 original vanilla saves** (5 eur, 4 spa;
  `economy_check` ECON_TRADE).
- **Round 7: routes of a landless faction** (INFERRED, `auto_after_c8` / `orig_over_nr4_0252`). Bavaria owns no region,
  but its governorship still lists eur_bavaria (owned by Austria).
  - A route's **importer is the governing faction of its last waypoint's region** (`World::governing_faction`;
    normally the owner). Württemberg's route ends in eur_bavaria and belongs to its Bavaria agreement.
  - A route's **GDP part takes the owners of its first and last waypoint regions** (`economy::trade_path_gdp_value`).
    Both Bavaria ↔ Württemberg routes store 296 = trunc(2.2 × sqrt(GDP Austria 15011 + GDP Württemberg 3200)).
- **Round 4: supply, re-spread, new routes, prices.**
  - **Supply from the trade fleets** (0x00BC9930, CONFIRMED; code `trade::node_volume`, `CampaignModel::node_supply`).
    A navy standing within 1 map unit of a trade node gathers that node's commodity:
    `trunc((min((ships − 1) × per_ship, cap) + 1) × base)`, with ships = its trade ships.
    - The node values come from the DB `trade_nodes`: #1 commodity, #2 base, #3 per ship, #4 cap. eur: 20 / 0.85 / 50.
    - Trade ships are the units of category `naval_merchant` (INFERRED for the naval record flag +0x1F4 that 0x00BC7890
      counts).
    - In `spa_napoleon` the volume is also scaled by effect 0x95 (round 6: modelled, §Supply assignment and split).
    - The node key comes from the campaign map (`regions.esf` `trade_nodes`, matched by position; `trade::attach_trade_nodes`,
      called when the game builds the map grid).
    - Checked: for all 7 factions trading at nodes in the eur and spa start positions, the computed supply equals the
      volumes of their saved domestic routes (`economy_fidelity::trade_node_supply_matches_the_domestic_routes`).
    - A node whose saved domestic route home is blockaded yields nothing.
  - **Re-spread**: replaced in round 6 by the demand split (§Supply assignment and split below).
  - **New agreements** (`CampaignModel::build_trade_route`, PROVISIONAL candidate set). The route is the cheapest path
    over the file's network legs (`TRADE_ROUTES`: from, to, splines, length) from any port or settlement of the
    exporter to one of the importer.
    - Land legs cost their length; sea legs cost length × `trade_route_land_sea_bias` (the cost terms of 0x00BC0DC0,
      CONFIRMED).
    - Sea legs may total at most `trade_route_internat_sea_length_limit`.
    - Trade nodes are not passed through.
    - Without a path the pair has no route and earns nothing.
    - Not ported: the sea/land route caps (`InternationalTradeRouteCounts`).
  - **Prices** (0x00BCB020, CONFIRMED formula; not ported, prices stay as loaded). For each commodity c:
    - demand D = Σ over every faction's regions in its home theatre of region +0x154[c];
    - supply S = Σ region +0x144[c] + the fleets' node volumes;
    - first time: f[c] (#3 f32) = (`background_commodity_supply` + S) × f / D;
    - then price #4 = max(1, round(D × f[c] / (`background_commodity_supply` + S)));
    - trend #7: 0 / 1 / 2–3 / 4 / 5 by price vs #5 (> 1.2×, > 1.1×, …, < 0.8×);
    - at the round end #5 ← #6 ← price.

    Round 5: ported (`CampaignModel::commodity_demand`, `update_commodity_prices`). Region demand (+0x154 = `REGION`
    #32) is computed by 0x00AB49F0: per `commodities_demand_junction` row, `round(weight × factor × driver)`, with
    `ddr_GDP` = GDP and `ddr_TW` = trunc(sqrt(town wealth)). This equals the stored #32 for all 72 eur regions, and
    the update gives back the stored prices of the eur and spa start positions. Region production (+0x144 = #31) is 0
    in every European region and is not modelled. Prices are not written by the save writer yet (note for save-compat).
- **Supply assignment and split** (round 6; code `CampaignModel::trade_split`, `trade_path_volumes`).
  - What the builder does (0x00BC0DC0, read):
    - Sources: every owned region with production (region +0x144, one source per commodity; 0 in Europe), and every
      trade-node fleet entry (per commodity, starting at the node's position).
    - Loop: take the source whose commodity has the highest price and that still has an allowed path (ties: the
      lowest path cost), then 0x00BC56C0 sends it down its cheapest allowed path. Path cost = Σ legs: land 0x00BC5B20;
      sea 0x00BC6520 / 0x00BC66E0 / 0x00BC6910 × `trade_route_land_sea_bias`.
    - Amount (0x00BB5730): min(rest, capacity left at the path's first stop). A port's capacity is its building's
      `commodity_export_vol` (effect 2, 0x00A79780); the region's ports are summed by 0x00A999F0, and 0x00BC6C70 picks
      the cheapest port with room. A settlement's land capacity is a hard-coded 300 (0x00AA67D0). A path that starts
      at a node has no limit.
    - A path is skipped when its first stop is full (0x00BD44A0).
  - **What the saves show** (superseded by round 16: the exe rule is `0x00BC26D0`, net demand, see "Where I am"). Per
    commodity, the exporter's supply is shared over its trade partners with a route in proportion to the importer's
    demand for that commodity (Σ `REGION` #32 over its regions, `commodity_demand`):
    - The partners are taken in ascending order of demand (ties by faction id), each getting
      `trunc(rest × demand / remaining demand)`. The last one, with the largest demand, takes the rest (all of it
      when no partner has demand).
    - Example: the Netherlands, 88 tobacco in `auto_nr4_t4`: Oldenburg 3, Portugal 6, Spain 23, France 56.
    - It reproduces **358 of 362 loaded route volumes** (`ECON_SPLIT`): the eur and spa start positions 36/36 and 4/6,
      eur `auto_nr4_t4` 72/72, `auto_after_c8` 76/76, `auto_b2b3_0211` 36/36, `auto_nr1` 60/60 and
      `orig_over_nr4_0252` 82/84, spa `orig_fr_t1`, `orig_fr_may1811` and `auto_orig_spa_0245` 6/6.
    - The misses: in `orig_over_nr4_0252` the Netherlands → Portugal stores 4 where the model gives 5, which sits on
      a rounding edge (5.02; the demand has probably moved since the builder ran). The spa start position stores
      spa_britain's volumes from before its supply effect (below), like every start-position figure, which are
      computed without effects.
  - **spa supply effect** (0x00BC9930, CONFIRMED formula): in `spa_napoleon` a node's volume is
    `trunc((1 + e/100) × v)`, with e = basic id 0x95 = `trade_node_supply_mod` from faction +0x6FC (CONFIRMED: our
    `BONUS_NAMES` equals the exe's table at 0x0145B4A8, all 184 names). spa_britain has +6: 37 → 39, as all three
    vanilla spa saves store.
  - Trade income stays exact for every faction of eur `auto_nr4_t4`, `auto_b2b3_0211`, `auto_nr1` and the three spa
    saves. The Bavaria / Württemberg differences of `auto_after_c8` and `orig_over_nr4_0252` are settled in round 7
    (routes of a landless faction, above): all 9 original saves are now exact.
- **Sea route cap** (0x008DC150; code `CampaignModel::sea_route_cap`, used by `build_trade_route`): a breadth-first walk
  from the capital (faction +0x72C) over the region neighbour list (game region +0x20 count / +0x24 array) through
  regions the faction owns (neighbour +0xF4 owner = faction). Each region reached adds Σ over its slots (region +0x128)
  of `trade_routes_mod_max_sea` (effect 0x43) of the slot's building (0x00AAF770: any building, integer; trading ports
  3 / 4 / 5). CONFIRMED walk.
  - **The capital counts** (INFERRED, against the literal reading): the listing starts the walk with +0x72C already
    visited, so the capital would add nothing. But every vanilla save needs it: Britain keeps 7 sea routes with all
    its trading ports in England, and Denmark, Portugal, Sweden, the Netherlands and Mecklenburg fit only with their
    capital's port. With the capital, no faction of the 8 vanilla saves is over its cap (`ECON_CAPS`). The land walk and
    the all-regions sum give the same cap for every faction in these saves.
  - **Neighbours** (round 6) come from `regions.esf`: every outline edge run has a `connectivity` triple {u32 region
    index << 16 | area index, first vertex, last vertex}. Self references, 0xFFFF and non-game regions (seas, the
    river region `all`, `eur_lakes`, the map edges) drop out. `World::region_neighbours`, loaded by
    `ntw_campaign::trade::attach_map`. Test `region_neighbours_come_from_the_map_outlines`: France has 9 land
    neighbours, islands have none, and the lists are symmetric. Without the map every owned region counts
    (PROVISIONAL fallback).
  - A new route may use the sea only below the cap. No land cap was found: France holds 5 land routes with no cap
    effect. Other callers of 0x008DC150 (0x00BCB020 prices, 0x00BA0490, 0x00BA42E0, 0x00BBA4F0, 0x00BC7910) are not
    read.

## Capture: occupy, loot, liberate (round 9; code `ntw_sim::campaign::capture`)
CONFIRMED from the exe unless tagged. The module docs hold the formulas; this is the map of the code.

**Flow.** A capture (after a won assault, `0x00B58560`; an undefended settlement, PROVISIONAL same variant) changes the
owner and clears the queues (round 8), then queues a report object (`0x00885F90`, vtable `0x013574DC`; flags +0x98
pending, +0x99 surrender, +0x9A / +0x9B skip the choice). Its slot 9 `0x008F8D00`:
- computes the preview `0x00B14930(report+0x14, surrender, looting multiplier 0x008D20F0, faction)`;
- AI faction (faction +0x6E0 = 0) without the skip flags: choice = `0x00AAABA0` (AI virtual +0x2F8);
- human without the skip flags and the campaign flag (+0x114): a UI event (+0xC78) with the three option structs;
  the screen fills `SettlementLootingOptions` (`0x00A13DB0` → `0x009AB1B0`);
- otherwise choice 1; then `0x008C0310(choice)`.

**Option structs** (report +0x14, +0x40, +0x6C; 0x2C bytes each): {+0 cap, +4 count, +8 (building, value) list, +0xC
money, +0x10 town wealth after, +0x14 / +0x18 public order previews (`0x00A61400` / `0x00A610C0` on a copy of the region
after `0x00AAA5B0`), +0x1C public-order reduction, +0x20 flag}. The third struct's flag (+0x90) and faction (+0x94) mark
a liberation; `0x008C0310(2)` loots when +0x94 is 0.

**Preview `0x00B14930`:**
- loot list: every slot of the settlement with a building of health > 1, `0x00B14560(range 0.01–0.5)` → (new health,
  value); money = trunc(var 106 `base_loot`) + Σ values; then the fortification slot (no money).
- money += clamp(var 87 × TW, 150, 7000) (+0xCC town wealth); money += clamp(var 87 × 0.15 × GDP, 150, 6000) (+0xBC),
  each step stored as an int; TW after = TW − d with d = trunc(min(var 88 × TW, 6000)), 0 if d ≥ TW; reduction =
  int var(108 + surrender); money = trunc(money × multiplier).
- occupy list: the slots whose building's chain has `building_chains` #2 = 0 (chain record +0x18, parsed from the
  text; `sArmy` 1 and `tFactory` 2 are spared), range 0.5–0.99; then the fortification slot. TW unchanged, no money.
- liberation (grand campaign path): region +0x10C = the rebel-faction record (`REGION` #24 key, e.g. `poland_lithuania`,
  `italy`, `ireland`; in the eur start position 39 of 72 regions name a campaign faction and 22 of those a faction without land), record +0x4C clear, campaign flag +0x124
  clear, the faction exists (`0x00955B30`) and is not the capturer, then either the faction's +0x824 is set
  (INFERRED: out of the game — modelled as "holds no region") or (+0x72C = this region and a count of 1: not decoded,
  not offered); finally the capturer's relationship #28 `allows_region_return`.
- Variables: 87 `settlement_looting_pct_region_gdp` 0.15, 88 `…_pct_region_gdp_reduction` 0.8, 106 `…_base_loot` 0,
  108 `…_publicorder_reduction` 10, 109 `…_surrender_publicorder_reduction` 20.

**Damage roll `0x00B14560`:** draw `r` (campaign RNG +0xFB8, high 16 bits); in f32: frac = (hi − lo) × (r ×
1.5259022e-5) + lo; new = clamp(trunc(frac × health), 1, 99); value = trunc((health − new) × 0.01 × level cost (+0x24))
× 4; result min(value, max(15000, value / 4)) (spa: 2 and 10000). The f32 steps matter: 99 lost of a 1000 building
gives 989, not 990.

**Looting multiplier `0x008D20F0`:** 1 + the two largest `looting_increase` (effect 0x87) × 0.01 among the characters of
the army's units.

**Apply:**
- loot `0x00B541E0`: each listed building's health := the listed value (a fortification goes through its damage
  object); money through `0x00BB3810(money, 0)` (treasury and the economy tracker); `0x00AAA580(reduction, TW after)`:
  every population class's +0x74 (the war-results base, our `class_bases` .2) −= reduction (`0x008EF780`), region
  +0xD4 := TW after, then `0x00AB4410(0)` copies +0xD4 into +0xCC and refreshes the region economy without growth;
  effects recomputed.
- occupy `0x00B582E0`: the occupy list's healths only.
- liberate `0x00B4F090`: `0x00B58A10(target, 0, 1)` → `0x00B449F0` hands the region over; then an army for the target
  (units filtered by `0x00B69BA0` from what the region's `sArmy` / `rHorse` / `tGuns` allow, a named general,
  `0x008D24B0`): not modelled. The capturer's army leaves the settlement (INFERRED). spa_napoleon has its own branch
  (guerrilla units).

**Repairs** (`0x00B66260`, `0x00B66410`, `0x00B1A6B0`, `0x00B16430`): can repair = health < 100, no item in the slot,
slot held by the owner; cost = round((100 + chain cost mod) × round((100 − health) × level cost × 0.01) × 0.01), the AI
capped at its treasury; length = max(1, floor((100 − health) × 0.01 × level turns (+0x20))), health per turn =
(100 − health) / length; the cost is paid at once. A building below 100 health has no effects (`0x00A62380`), so damage
matters until repaired.

**Saves:** no vanilla save shows a capture's damage (settlement slots are all at 100 in the 13 vanilla files; the damaged
buildings there are town and resource buildings — a college, a gentlemen's club, a timber camp at 64–82, sabotage
INFERRED — and spa scripted ones at 10). Some stay unrepaired across saves (florence's college at 64 in both
`auto_nr4_t4` and `auto_after_c8`), so the AI does not repair everything it can pay for: the stand-in is eager
(PROVISIONAL). In a 10-turn AI run of ours two captured town halls show the occupy damage (70, 53).

**Checks:** `capture_damage_roll_matches_the_listing`, `human_capture_waits_for_the_choice_then_loots`,
`ai_capture_occupies_and_liberation_hands_the_region_over`; `economy_check` `ECON_REBELS=1` lists each region's rebel
faction and liberation target.

## Construction cost (round 6, traced round 16: CONFIRMED; code `CampaignModel::construction_cost`, `building_cost`)
- **Formula (CONFIRMED, `BuildSlotConstructionOptionsWithCost` `0x00B43300`).** For each option of a slot:
  `cost = FISTP(f32(level cost) × (modifier + 100.0f) × 0.01f)`. The level cost is the record's +0x24 (unsigned, to
  double, to f32); both products are f32 (SSE), in that order (cost × (m + 100), then × 0.01); the result is rounded
  by FISTP, i.e. half to even. No clamp, no difficulty or AI factor outside the modifier, no special case for any
  chain: the walls (`sFortifications1_settlement_fortifications`, 8000) take the same formula. A modifier below −100
  would give a negative cost (vanilla never does).
- **Modifier (CONFIRMED).** `0x00E1EF10(chain record (level +0xC), 0)` → `0x00E23DA0(2, chain, 0, 0)`: the
  chain-keyed `mod_cost` entry (bonus type 2, id 0) of the region's effect set `0x00A67530`, built once per option
  list by `0x00B43880`. That set is the one `economy::region_effect_set` models (EFFECTS_FIDELITY.md §2): the region's
  own buildings (+0x188), region +0x1DC, and the governing faction's part `0x008AFB50` (faction +0x6FC = #55 base +
  difficulty handicap, faction-wide buildings, researched technologies, government, ministers). So every source counts
  through the DB mapping (`effect_bonus_value_building_chain_junctions`, `mod_cost` rows), and only for the chains it
  lists:
  - `building_cost_mod_all` (timber camp / lumber mill / sawmill −10 / −12 / −24, local) maps to 19 chains (pNavy,
    pTrade, rFarm, rGold, rIron, rTimber, rWine, sAdmin, sArmy, sCannon, sCulture, sFortifications, sRoads, tCommerce,
    tEducation, tFactory, tGuns, tSecret, tSupply, plus the Spain variants): **not** rHorse, the prestige chains and
    the other unlisted chains (before round 16 the model applied it to every chain).
  - `building_cost_mod_all_global` (steam sawmill −2, faction-wide) maps to the same chains plus rHorse (before: not
    counted).
  - Technology `economy2_joint_stock_company`: `building_cost_mod_industry` −10 on tFactory (/Spain).
  - The difficulty handicap's `building_cost_mod_*` rows reach #55 as type-2 entries (e.g. rFarm −50 → −58).
- **Card = queued cost (CONFIRMED).** The card's `cost` (`PushConstructionOptionRowToLua` `0x009C8920`, from
  `BuildConstructionSlotEntryTable` `0x009FBAB0`) is the option entry's +8. The construct command (`CCQ_BUILDING_CONSTRUCT`
  `0x00931C80` → `0x00B13D50` → `StartSlotConstructionFromOption` `0x00B13DD0`) rebuilds the list, refuses when
  treasury < cost (signed), pays that cost and stores it in the item (`0x00AE6FB0` → `0x00AE6E80`). The "too dear"
  flag (option flag 1) compares **unsigned** (JBE): with a treasury below 0 every card of cost ≥ 0 shows affordable
  while the command refuses (signed); a negative cost (modifier below −100) shows too dear unless the treasury is
  below 0 too and not below it (treasury −5, cost −300: affordable; cost −3: too dear). The command builds its list
  with flags (1, 1), which drops the too-dear options, so it refuses a negative cost too, unless the treasury is
  also negative and not below the cost unsigned; then it pays the negative cost out.
- Out of the int range (or NaN) FISTP stores 0x80000000: `building_cost` gives `i32::MIN`.
- Repairs (`0x00B66410`) read the same modifier (CONFIRMED, `building_cost_modifier`), and their arithmetic is the
  same: f32 throughout, the ints (level cost, health, the rounded base) read unsigned, each step rounded by FISTP;
  the last step is `building_cost(base, modifier)` (CONFIRMED from the disassembly; `repair_cost_uncapped`). Health
  above 100 makes the base negative; the last step reads it unsigned (about 4.29e9), so the result is 0x80000000
  unless the modifier is about −50 or below, which brings the product back into range (a large finite cost). No
  repair starts there (`0x00B1A6B0`: health < 100, unsigned).
- **Paying (CONFIRMED, module `campaign::treasury`: construction and repair only).** Charging is `0x00BAF500`:
  faction +0x3F4 minus the amount, plain 32-bit (wraps; `pay` uses `wrapping_sub`; the category converter
  `0x00BA8CD0` taken as identity, INFERRED). Construction: `can_pay_construction` = the unsigned card test and
  treasury ≥ cost signed. Repair: `0x00B16430` (the panel's `can_afford_repair`, and the AI's `0x00AA4910`) is
  repair cost ≤ treasury, signed, on the cost that `0x00B66410` caps at the treasury for an AI faction, so an AI
  always passes; the repair command (`CCQ_BUILDING_REPAIR` handler `0x00931DA0` → `0x00B66260`) tests nothing and
  charges only a cost above 0, but stores the cost in the item either way (`0x00AE7010` → item +0x14). So a repair
  costing 0x80000000, or an AI repair in debt, is queued for free.
- **Cancel refund (CONFIRMED).** `CCQ_BUILDING_CANCEL_CONSTRUCTION` (registered by `0x0041F500`, handler
  `0x00931C30`) calls `0x00B1A790` with the command's flag (the panel's `CancelConstruction` `0x009E0F10` sends 1):
  when the slot's item stores a cost other than 0 (item +0x14), `0x00BB3810(cost, 3)` credits it, faction +0x3F4
  plus the amount, plain 32-bit (`refund`, `wrapping_add`; category 3 converter `0x00BA8CE0` taken as identity,
  INFERRED). The stored cost is credited whatever was charged: cancelling a free 0x80000000 repair credits
  0x80000000, an AI debt repair's −20 credits −20. Saves keep the cost bit for bit (`BUILDING_CONSTRUCTION_ITEM`
  #4, u32). Tests: `treasury` module, `a_repair_costing_i32_min_is_queued_and_charges_nothing`,
  `an_ai_repair_is_capped_at_the_treasury_and_free_in_debt`, `cancelling_a_repair_credits_its_stored_cost`,
  `walls_round_trip_in_both_slot_shapes`, `a_treasury_below_zero_shows_every_card_affordable_but_the_command_refuses`,
  `a_negative_cost_shows_unaffordable_and_is_paid_out_only_from_a_negative_treasury`.
- Recruitment, the unit pool, treaties, upkeep and script treasury changes keep their own arithmetic (BACKLOG §0-B).
- Checked (`ECON_BUILDCOST`): 445 of 452 queued items in the 19 saves of the vanilla set, the same as before the
  change (the saves hold no item of the newly differing cases). The 7 misses are items queued while a timber camp
  counted differently (the cost is fixed when queued), e.g. the Bavarian items of `auto_nr4_t4`.
- Cost: the region set (faction sum included) is built once per option list, and `region_construction_options`
  (the AI's build options) builds it once per region: 1.46 ms for every region of `auto_nr4_t4` in release (5.7 ms
  when built per slot; 0.57 ms before round 16, when no faction sum was built).
- In-game check (one quick look): the Paris prestige card now shows 15000 (full price; 13500 before).

## Recruitment cost and money (0-B recruitment details, CONFIRMED unless tagged; code `economy::recruitment_cost`, `CampaignModel::recruitable_entry_flags`, `treasury.rs`)
- **The queue command** (`QueueRecruitmentItemForUnit` `0x00B58DD0`; callers: the `CCQ` handler `0x00936B90`, the AI
  `0x00A9EF50` / `0x00A9F200`, the console `0x00924DE0`): refuses a full queue (`0x00B62040`: more than 9 items), builds
  the queue's priced and flagged recruitable list (vtable +0xC: land `0x00B31020`, naval `0x00B30E80`), refuses a unit
  with no entry or an entry with any flag set, charges the entry's cost as spending category 2
  (`ChargeFactionTreasuryAmount` `0x00BAF500`), charges the recruitable population (below), then builds the item
  (`0x00AECEE0` land / `0x00AED220` naval → `0x00AF3F80`), which keeps the cost at item +0x20 and bumps the faction's
  queued count of the unit type. It never looks at the recruitment points (they only pace the training).
- **The entry cost** (`PriceRecruitableEntry` `0x00B0D220`, over the region effect set `0x00A67530` for both lists):
  `mod` = the integer effects `recruitment_mod_cost_land_all` (`_naval_all` for a ship) + the unit category's
  `cost_mod` + the unit class's `cost_mod`; in `spa_napoleon` also `guerrilla_cost_mod` for a key ending in
  `_Guerrilla`, else `auxiliary_cost_mod` for one ending in `_Auxiliary` (case-sensitive); `mod` clamped at −100; cost =
  FISTP((mod + 100) × `units` #7 × 0.01) in f32 (half to even). The base is #7 (`UNIT_RECORD` +0x38, copied by
  `0x00E91320`), not #4. The same function adds the experience effects to entry[1] and puts the upkeep (`0x008B21D0`)
  in entry[4]. Data: `economy_check` `ECON_RECRUITCOST` matches **392 of 406** queued items in the saves the original
  wrote (`auto_after_c8` 90/97, `auto_nr1` 46/47, `auto_nr4_t4` 98/99, `orig_fr_may1811` and `orig_fr_t1_b` 13/14 each,
  `orig_over_nr4_0252` 132/135; `nr2`/`nr3` are our own saves, charged #4 by the old code). Every miss is an AI
  faction whose `recruitment_mod_cost_*_all` in the save differs from the one the item was priced with: Saxony and
  Naples items at −5 while the save reads −10 / +1 (the same factions' other items match at −5), one Spanish
  merchantman at +49 against +24. When the faction's cost mod changes between the AI's queueing and the save is
  UNKNOWN (no code depends on it: the price is taken once, at queue time, as the original does).
- **The `0x00ED49A0` experience-adjusted cost is not the campaign's**: the queue charges entry[0], which both list
  builders set through `0x00B0D220` on every entry just before flagging them. `0x00ED49A0`'s callers are `0x0045D170` (from the army setup generator `0x004765F0`,
  `0x0045CB50` and `0x00461790`), `0x004C2770` and the unit info "XpAdjustedCost" `0x005CD340`, none on the queue
  path. The old `XpCostTables` charge on the campaign is removed.
- **The entry flags** (`FlagUnavailableRecruitableEntries` `0x00B69BA0`, per entry, OR-ed into entry[5]):
  0x40 when `IsUnitAtFactionUnitCap` (`0x008F68B0`): `units` #15 (`UNIT_RECORD` +0x68) > 0 and the faction's live units
  of the type (faction +0x7C8, kept by the campaign unit constructors and the destructor `0x0088F870`) plus its queued
  items of it (faction +0x7E4, kept by `0x00AF3F80` and the item destructor `0x00AF7730`) reach it; 142 of the 442
  vanilla units have a cap. 0x02 when the cost is above the treasury compared **unsigned** (JBE), so a faction in debt
  is never too poor. 0x04 when `HasRecruitmentPopulationAvailable` (`0x00A89550`) fails. 0x01 when the queue is full.
  The card's `reasons_unavailable` list (no slot, unaffordable, population, …, limit at bit 6) matches these bits
  (INFERRED: the generator `0x009FE7B0` was not traced that far); the UI now shows the model's flags.
- **The recruitable population — not modelled (PROVISIONAL).** The region's population object (region +0x28, saved as
  `POPULATION`) holds at +0x54 (region +0x7C) the value saved as `REGION_FACTORS` #2; it differs from `POPULATION` #1
  (the model's `population`) in `orig_fr_may1811` (31 of 31 regions, e.g. 966967 against 969000) and equals it in the two
  turn-1 saves. Gate: flagged when it is below var 36 `minimum_population_after_recruitment` + var 37
  `recruitment_population_cost` (registration order: key objects at `0x0164B550` + 8 × index, `0x0164B670` /
  `0x0164B678`). Queue charge `0x00AAF190`: subtract var 37 when that leaves at least var 36, else set it to var 36.
  Cancel credit `0x00A61AA0`: add var 37. Both variables are 0 in the shipped `campaign_variables`, so with vanilla data
  the flag is never set and the charge and credit are 0.
- **Money arithmetic.** Charge and refund pass the faction economics' per-category converter (spending +0x42C,
  income +0x3F8; vtable +4). A new campaign (`ConstructNewFactionEconomics` `0x00B96100`) sets all 13 income and 12
  spending converters to the object at `0x01459050` (vtable `0x0137EA30`, +4 = `0x004A23F0`: returns its argument). The
  type table `0x01458E88` = {`0x0145904C` (vtable `0x0137EA28`, +4 = `0x0044C230`: returns 0), `0x01459050`}; only the
  save loader `0x00B95B90` picks per category from a saved type byte (and forces income category 12 to type 0 for a
  version-1 save); the saver `0x00BC4F20` writes the type back. The two writes of a base vtable `0x01312460` into those
  objects (`0x012F37F0` / `0x012F3800`) are their destructors, registered with `_atexit` (`0x004302E0` / `0x004302F0`).
  So recruitment (category 2) and its refund (income category 3) are plain 32-bit arithmetic (`treasury::pay` /
  `refund`). Cancel (`CancelRecruitmentItem` `0x00B1A820` → item slot +0x1C `0x00B5C060` land / `0x00B5C0A0` naval with
  1): credits item +0x20 as income category 3, i.e. exactly what was charged.
- Tests: `the_recruitment_cost_is_units_7_scaled_by_the_cost_effects`,
  `recruiting_charges_the_entry_cost_and_cancelling_refunds_it`, `a_faction_in_debt_may_recruit`,
  `a_region_without_recruitment_points_still_queues`, `the_unit_cap_counts_the_factions_units_and_queued_items`,
  `the_recruitment_rule_is_unsigned_only` (ntw_sim); `the_loader_takes_the_campaign_cost_from_units_7`,
  `recruiting_from_a_loaded_campaign_charges_units_7_with_the_region_effects` (ntw_campaign).
- Not traced here: the AI's own cost estimate (`ntw_ai` prices recruits from #4 with its handicap; §6).

## Recruitment and construction queues (round 8, CONFIRMED unless tagged; code `turn::region_turn`, `commands::recruit`)
- **Turn start.** The faction turn start `0x008F2620` (also run for "Campaign first round (from savegame)") calls the
  region update `0x00AAE820`. Unless `0x008CEEF0` holds, that runs the recruitment queue of the region and of each port
  slot (slot +0x1E8), then agent spawning (`0x00A249A0`).
- **Recruitment** (`0x00B71FB0`, force 0). The queue is a manager with an item list (+0x4C count, +0x50 array) and a
  vtable (land interface 0x0137CBA0, naval 0x0137CBE4).
  1. Items whose unit the region's recruitable list no longer has (`0x00B45690` lookup) are removed through
     `0x00B1A820(item, 1)`, the cancel path, so each is refunded its stored cost (CONFIRMED, §Recruitment cost and
     money).
  2. Capacity = method 1 of the manager: land `0x00B61F30` = recruitment points, naval `0x00B61EE0`.
  3. In queue order, each item that is not blocked (`0x00B5AD90`: its entry in the recruitable list is flagged) and
     has its flag +0x54 set takes one turn (item method +0x18); the loop stops once `capacity` items have.
  4. Finished items are trained.
  - Flag +0x54 is set on every queued item by `0x00A78620`, called from the faction's round-end economy
    `0x008BC650`. So an item counts down only after a round end has passed since it was queued.
  - The queue holds at most 10 items (`0x00B62040`).
  - Force 1 (`0x00B489B0` → `0x00B71FB0(1)`, from `0x008CDD60`) ignores the flag and repeats until the queue stops
    changing: an instant-complete path (cheat or script).
- **Construction** (`0x00A78670`, a virtual): `0x00B78800` for every slot of the settlement container (the slot list
  plus the two extra slots). Each slot steps its own construction item (item method +0xC) when the slot's holder is
  the region's owner (or the item's state is 2 / 3), so items progress side by side. The model skips items in slots
  held by another faction. That this runs in the same region update is INFERRED.
- **Saves agree:** Cleves (1 point) 1 started and 5 waiting; Gibraltar (6) 6 of 10; several construction items per
  region progressing together; AI items queued in round 1 untouched in the round-2 saves.

### Recruitment spawn, round 15 (open item 7)

Round 12 mapped the spawn *path* and left three sub-items open: placement, `num_men` sizing and the
garrison join. Round 15's read of `0x00B71FB0` (1523 bytes, the queue tick) settles two of them.

- **The queue tick creates no units — CONFIRMED from the decompile.** `0x00B71FB0` does exactly four
  things: set the manager's `+0x16` flag from `0x00B6A8D0`, drop items whose unit the region can no
  longer recruit (`0x00B45690` → `0x00B1A820(item, 1)`), advance the first `recruitment_points` items
  through item method `+0x18`, and — when `+0x16` is clear — collect the finished items through the
  manager's method `+0x20`, hand each to `0x00B0A270`, and delete the entries that fail `0x00B5C110`.
  There is **no unit construction and no `num_men` read anywhere on it**. So round 12's
  "no `num_men` reader and no position / garrison-branch reader found on this path" was not a failure to
  find them: they are not there. The spawn is elsewhere, in the `0x008EF790` walker.
- **Placement and the garrison join were already ported — by 0-E, not by us.**
  `CampaignModel::spawn_recruited_unit` (`commands.rs`) implements the `0x008EF790` walker and the
  branch: land into the army garrisoned in the settlement if it is the owner's and has room, else a new
  army under a new colonel; naval into the navy that received the port's last ship while it is still in
  the port, else a new navy under a new captain at the port slot's position. Both branches are marked
  CONFIRMED there and are **not re-derived here**.
- **`num_men` sizing — the one sub-item still open, and it is CONFIRMED from data (not from the exe).**
  New tool mode `ECON_MEN=1` compares every unit in a file with the rule `spawn_recruited_unit` sizes by.
  Over all nine vanilla saves, **3174 units, no exception**:
  - a **land** unit's `max_men` is exactly `unit_stats_land.num_men` (the DB column the model already
    loads into `UnitRules::men`);
  - a **ship**'s `max_men` is exactly the **sum of the `unit_stats_naval` crew triple**
    (c17/c18/c19, already loaded as `rules::ships[..].crews`) — `3_Decker_British_1st_Rate`
    50+50+204 = 304, `Trade_Ship_Indiaman` 0+28+112 = 140, `Small_Ottoman_Galley` 10+14+4 = 28,
    `Special_Ottoman_Bomb_Ketch` 0+12+30 = 42, and so on for every class including the merchant and
    galley types. **383 of the 3174 units are ships**, so the rule is exercised across the classes.
  - `men` equals `max_men` on every unit except the **14** that carry battle casualties (9 of 460 in
    `auto_nr4_t4`, 5 of 484 in `orig_over_nr4_0252`, none in the four spa saves), which is the expected
    damage reading — so a fresh unit starts at its full size.
  Added `world::recruited_unit_size(rules, unit_key)` (`world.rs`, 0-B's file) and the test
  `recruited_unit_size_is_num_men_for_land_and_the_crew_sum_for_ships`, which **calls** that function
  against all nine saves rather than restating the arithmetic. `rules::men` is no longer PROVISIONAL.
  - **Done on main:** `spawn_recruited_unit` used to size a ship by reading the `max_men` of another
    unit of the same key (`same_key()`), because `UnitRules::men` is 0 for ships, which cannot work for
    a ship type the campaign has not built yet. It now calls `world::recruited_unit_size(&self.rules,
    &unit_key)` (unit test `recruited_units_are_sized_by_num_men_or_the_crew_sum`).
- Still UNKNOWN, and not reachable from the data: what happens when the target force is full *and* no
  new commander can be found, and the population cost of a queued item (round 12 step 1 found the
  variable, not the amount).

## Research (round 7; code `ntw_sim::campaign::research`)
- **Save layout** (CONFIRMED). `FACTION_TECHNOLOGY_MANAGER` `techs[]` entries are {utf16 key, u32 state, f32
  progress, u32 researcher, u32[], u32}.
  - The researcher is the school's `REGION_SLOT` #2 id (= its residence #1), e.g. Austria's college in Graz.
  - The school's residence #14 lists the gentlemen inside; the character's #7 is the residence id.
  - Loaded as `FactionDetails::research` (progress, researcher) and `RegionSlot::id`.
- **States.**
  - 0 = researched (CONFIRMED: completion `0x008EED20` sets progress = cost and state 0; the stored progress of
    every state-0 tech equals its cost).
  - 2 = available. Research may start in state 2 or 1 (`0x008B33B0`).
  - 4 = not yet available.
- **Availability** (round 8: CONFIRMED writer `0x008F91F0`). For each tech in state 4 or 3:
  - its single requirement (record +0x60) must be absent or researched;
  - the faction must own the tech's building level (`0x008B13E0`): any slot of an owned region (region +0x120 slot
    list; any health, any holder; not the road) with a building whose chain record matches the required one (the pair
    at chain +0x10 / +0x14), at that level or higher;
  - every tech of the record's list +0x70 (`technology_required_technology_junctions`) must be researched.

  Then the state is 2, or 3 when an item of the list +0x7C fails `0x008B13C0` (INFERRED to be
  `technology_required_building_levels_junctions`, empty in the shipped DB, so 3 never occurs). The writer never
  sets 4, so a tech never goes back (Bavaria, Italy and Oldenburg keep state 2 after losing the building).
  - The chain match is modelled as "the chain, or a variant whose name starts with it" (`sAdminSpain` for `sAdmin`):
    INFERRED for the +0x10 / +0x14 pair.
  - Callers: research step `0x008DD450`, completion `0x008CDCB0`, load `0x008CB8F0`, and `0x008CDD60` after its
    per-region pass. The model runs it at the faction's turn start (after its regions) and after each research step.
  - The rule reproduces every state of the 4 shipped start positions (test
    `tech_availability_rule_keeps_every_start_position_state`) and changes nothing in the 12 vanilla saves.
- **Who researches** (`0x008B33B0` checks, `0x008EEC90` starts):
  - a school is a slot whose building's own effects give `research_points` > 0, at full health, held by its
    region's owner (colleges / universities 10 / 20 / 40, Spanish ones 15 / 30 / 60, Spanish churches 10 / 20 / 40);
  - the tech must be in state 2 (or 1) and not already be researched by another school;
  - a school that was researching something else drops it (its progress is kept).
  - Command `CampaignCommand::StartResearch`.
- **Rate** (`0x008EA9F0`, CONFIRMED formula):
  `(100 + research_rate_mod) / 100 × (min(Σ gentlemen, character_research_points_cap = 12) + research_points +
  thread effect)`.
  - The effect set is the owner's faction sum + the school building's own set + each own gentleman's set.
  - The thread effect is `research_points_military` / `_industry` / `_enlightenment` for thread 0 / 1 / 2. The thread
    (technology record +0x8C) is not a DB column; it is taken from the key prefix: military / economy / other
    (INFERRED; consistent with the saves).
  - A gentleman counts his research attribute (`0x009C7610` index 8: level + bonus array +0x394). That is
    **saved level + 1** (INFERRED: the saved level already holds trait bonuses, e.g. Academic Honours base 2 + 1 = 3
    saved, and every AI school with a gentleman needs one more point; the +1's source is not traced).
  - PROVISIONAL: the army / navy mods (`research_rate_mod_army_tech` / `_navy_tech` for techs whose record names
    "army-admin" / "navy-admin") are not applied.
  - Checked (`ECON_TECH=rate`): progress = whole multiples of the computed rate for every AI tech under research in
    `auto_nr1` (1 step), `auto_nr4_t4` and `auto_after_c8` (2), `orig_over_nr4_0252` (1–3) and the spa saves (2).
    The two exceptions are human / timing cases: France's second gentleman, and Russia's after a second gentleman
    arrived.
- **Step** (`0x008DD450`): each tech with a working school gains the rate once per round, at the faction's round-end
  economy step (CONFIRMED: `0x008BC650` → `0x008F3C40` → `0x008DD450(0, 0, 1)`; `0x008CDD40` with (1, 1, 0) is an
  instant-complete path). The saves agree: AI techs started in turn 1 hold one step in the turn-2 save.
  At progress ≥ cost the tech is researched: state 0, progress = cost, school freed, event `ResearchCompleted`
  (an exe string). Then availability is updated.
  - Its effects apply at once through the effects store (state 0 technologies).
  - Not modelled: the prestige award `0x008F8EF0` (×var 7 when 0x008BE760 holds, majors only; prestige is not
    modelled).
- **Gates** (INFERRED: the DB junctions; 0 failures over the queued items of the original saves):
  - `building_level_required_technology_junctions` (`building_tech_ok`, used by `can_build`): e.g.
    sAdmin3_court_justice needs admin1_public_schooling, so Paris cannot upgrade its magistrate at the start.
  - `unit_required_technology_junctions` (`unit_tech_ok`, used by `recruitable_units`): e.g. rocket troops need
    military5_rockets.
  - `nr1`–`nr3` are saves written by our own port (0 public-order classes), not evidence; `nr3` holds a France
    sAdmin3 queued without the tech.
- **Open.** The AI's research choice (see AI_RESEARCH.md §7 item 8); the UI's tech availability bit in the construction
  panel (0-E); the save writer must write `techs[]` #1 / #2 / #3 (save-compat). `0x008DD450`'s callers
  (`0x008CDD40` / `0x008CDD50` / `0x008F3C40`) are not read.

## Bankruptcy (0x00BBC7D0 / 0x00BABE30 / 0x00BA2030 / 0x008AE710, CONFIRMED; code `economy::settle_round`)
- Flag when `treasury + income < expenses` (computed in the round-end pre-pass). A bankrupt faction's treasury is set to
  0 instead of paying; the bankrupt-turn counter (+0x460) grows; the message (0x009CBBA0, event 0x11A) only on the
  first turn in a row.
- Each bankrupt turn: the capital region's +0xEC (`REGION` #17) += 2 (max 6), −1 at each of the faction's turn ends
  (0x008BD0F0 → 0x00A786C0); it scales town wealth growth by `(m − #17)`. A faction counter +0x828 does the same
  (+2 max 6, −1 per turn; use not found).
- **Desertion** (0x008AE710 → 0x008BA020 / 0x008BA1E0, CONFIRMED formula; code `economy::bankrupt_desertion`). It
  runs while faction +0x50C > 1. That field's meaning is UNKNOWN; it is not written by the `FACTION` saver, and the
  model uses bankrupt turns > 1: PROVISIONAL. It applies to every force of a character in the faction's +0x764 list
  that has no settlement garrison residence.
  - Round 6 search for the +0x50C writer. 0x008AE710 runs on the faction (0x00BA2030 passes economics +0x45C). A scan
    of 0x00850000–0x00C80000 for direct stores to `+0x50c` finds no faction writer. The hits are:
    - the campaign model's +0x50C (0x008751E0, 0x00A3CE70, 0x00A3EFF0);
    - a **character** counter set (0x009DA210, from the turn end 0x008BD0F0). Character +0x50C counts consecutive
      turns the character stands in his own faction's territory (0x00A28E20: the region at his position
      (0x00A1BDF0) has owner +0xF4 == his faction +0x2A0, else 0). It is the script function `CharacterTurnsAtHome`
      ("Returns the number of turns in home regions exclusively", 0x0089DFE0). +0x508 counts turns in a region of a
      faction at war with his (0x008CE9B0); +0x504 counts turns of 0x00A28D90.

    So the faction field is written some other way (through a sub-object or a bulk copy); the gate stays PROVISIONAL.
  - Every unit except the commander's own unit loses `round(r × men)`, with `r = 0.07 + rand × (p − 0.07)`. One
    campaign-RNG step (world +0xFB8) is drawn per unit.
  - `p = 0.3` if income < 1, else `clamp((expenses/income − 1) × 0.3 + 0.07, 0.07, 0.3)`.
  - Units of record classes 4 / 0xB / 0xC / 0xE are skipped (classes not mapped). **Round 14: still not
    named — a bounded negative result, do not port a guess.** The lookup is `economy_check ECON_DESERT=1`
    and `ECON_UCOLS=1`:
    - The enum those codes index is `0x00EED3E0`, the **alphabetical unit-class list**, which
      `ntw_formats::group_formation::UNIT_CLASSES` reproduces (46 entries; CONFIRMED by round 11's debugger
      finding 0x18 / 0x26 = `naval_bomb_ketch` / `naval_rocket_ship`, which are entries 24 and 38 of that
      list, and by `unit_stats_land` #3 `class` mapping onto it). Read as indices of that list, the four
      codes are **`cavalry_heavy` (4), `elephants` (0xB), `general` (0xC), `infantry_elite` (0xE)** — an
      implausible "cannot desert" set (heavy cavalry and the Old Guard are ordinary line units), so either
      the round 6/7 transcription of the codes is wrong or `unit + 0x20` is a different field.
    - No shipped `units` int column can be that enum either: #6 (recruitment turns) = {0,1,2,3,4,5,8,10},
      #9 = {23,27,29,30,32,34,37,50,55,70,85}, #20 = {0,2,3,5,7,10}, #22 = {0,1,2,3,4,6,10} — none contains
      11, 12 and 14 together. The class tables give nothing better (`unit_class` is 45 rows,
      `unit_class_to_population_class_priorities` 22 land rows, `unit_category` 12 key-only rows, so 0xE is
      out of range of the last two).
    - **Exposure** (`ECON_DESERT` over the vanilla saves): of the four, only `general` is common outside a
      settlement (16 / 14 / 11 units in `auto_nr4_t4` / `auto_nr1` / `orig_fr_t1` — one per army, the
      commander's own unit, which the model already skips explicitly); `cavalry_heavy` has 1 unit in
      `auto_nr1` and 7 in `orig_fr_t1`, `infantry_elite` 1, `elephants` **0**. So if the transcription is
      right the model over-applies desertion to at most a handful of units per outside army — small, but not
      zero, which is why this stays PROVISIONAL rather than dismissed.
    - Next step (needs the exe, one focused read): name the dword at campaign `LAND_UNIT` +0x20 — who writes
      it and from which column — and re-read the four comparisons against it. The only shipped candidate for a
      real "cannot desert" rule is one of `unit_stats_land`'s 30 unnamed bools (#52..#65, #69..#84), none of
      which is named.
    - **Round 15: that next step was aimed at the wrong address, and the four classes stay UNKNOWN — a
      sharper negative result.** The comparison is `MOV EAX,[EBX+0x48]` / `MOV EAX,[EAX+0x20]` at
      `0x008BA244`, i.e. **`*(unit+0x48) + 0x20`**, *not* `LAND_UNIT +0x20` (`lst:0x008ba1e0`; the four
      `CMP EAX,0xe / 0xb / 4 / 0xc` are at `0x008BA24A`, `0x008BA253`, `0x008BA25C`, `0x008BA265`). So
      naming campaign `LAND_UNIT` +0x20 — round 14's recorded next step — could never have worked, and
      the implausible "heavy cavalry and the Old Guard cannot desert" reading comes from reading the
      codes as indices of the wrong object.
      - **`*(unit+0x48)` is the unit's stats record — CONFIRMED, not inferred.** `0x008F9B10` (the land
        upkeep reader, and vtable slot 8 of the same table) reads the *same* pointer: `+0x24` the unit
        **category** key and `+0x28` the unit **class** key (fed to the category / class upkeep-effect
        getters `0x00E1F0F0` / `0x00E1F110`), and `+0x3C` the upkeep — exactly what §Economy already
        documents for that function. So `+0x20` is an **int sitting immediately before the category key**.
      - **It is not `num_men`.** The offset arithmetic that suggests it (record = builder − 4, so
        `record+0x20 ↔ builder+0x24 ↔ column #1 = `num_men`) is wrong, and the data says so outright:
        `num_men` takes only **{24, 32, 48, 60, 80, 120, 160}** in the shipped rows (`ECON_UCOLS2=1`), so
        none of 4 / 0xB / 0xC / 0xE can occur in it. Whatever `+0x20` is, it is not a size.
      - **It has exactly one other reader in the whole binary**, and it is a named-argument getter:
        `0x008F6810` = `return record->[0x20] == arg` (`RET 0x4`), reached only through the **38 vtables**
        at `0x013554B0` that all begin with it (and `0x01355548`, 0x98 = 38 pointers later). So the
        field is behind a *property query*, not a field name the binary spells out; a `bytes:` sweep for
        the two-instruction pattern `8B43488B4020` (and the `EAX` variant) finds only that getter and
        `0x008BA1E0` — nothing else reads it.
  - A unit left at or under the minimum strength is disbanded. The minimum strength is 0x008F68F0:
    `ceil(unit_minimum_strength × max men)`, at least 4, at most max men (`economy::unit_minimum_men`).
- Not modelled: prestige −2 for majors (0x008A8C20); regular payments are cancelled in proportion to the deficit
  (0x00B0E580).

## Action points (0x008C3250 → 0x008C3170, CONFIRMED; code `turn::force_action_point_factor`)
`factor = clamp(1 + general_admiral_action_point_bonus + max(0, effect 128)/100, 1.0, 1.6)` multiplies the movement
points of everything in a military force whose type (+0x2C) is 0 (army) or 1 (navy) — the same test the script
functions counting a faction's armies (0x0089C690) and navies (0x0089C5E0) use. So any commander qualifies (general,
colonel, admiral, captain); agents do not. The base comes from 0x008A86D0 (movement effects: not modelled). The model
scales a force commander's maximum (`max_movement_points` = round(base × factor), the base kept in `base_movement_points` = `LOCOMOTABLE` #8, as the original keeps the unscaled value at unit +0x114 and the files store the base) and refills to it (mapping to characters INFERRED, as our action points live on characters).

## Autoresolve (code `campaign::autoresolve`, land battles)
Flow (CONFIRMED): 0x008F7880 → 0x008AE5A0 → 0x007ADDD0 → 0x007ADF50:
- setup 0x0078D9E0;
- extra modifiers 0x007A4B10 (they add 0 here, INFERRED);
- winner 0x007690C0;
- settlement building damage 0x00750400 (sieges with artillery; not ported);
- losses 0x0074F450;
- captives 0x0074F710 (only when resolver `+0x14` is set, INFERRED naval);
- kill and experience bookkeeping 0x00751500 / 0x0074F550 / 0x0079B990 (no effect on men; not ported).

Side A = the first alliance (the attacker, INFERRED). The resolver reads the `autoresolve_*` variables from the
110-float array at offset 4·index; `[n]` below is the index (names by registration order).

- **Unit potential** (0x00790030): battle unit `+0xDC` (melee) + `+0xE0` (missile). These are the record potentials
  0x00757120 / 0x007575A0 for the card's men (`battle::strength`, CONFIRMED by 0-A).
  - × (1 + handicap), rounded. The handicap is [74] for a human on easy, [75] / [76] for the AI on hard / very hard.
    The model has no difficulty, so none is applied (`// EFFECT: difficulty`).
  - × 0.3 / 0.6 for cavalry, elephants and camels of side A in battle types 3/5/6/8. The type codes are UNKNOWN, so
    this is not applied.
  - `SA`, `SB` = the side sums. `+0x34` / `+0x38` = the sums over cavalry, elephants and camels only (category codes 0,
    4, 5).
- **Missile modifier r2** (0x00758830): per side, f = (units − units passing 0x007CB8A0) / units. A unit passes if it is
  artillery, or of class 0x15 with a weapon (the class is not mapped). Then:
  - `fA = 0` → `−fB`;
  - `fA < 0.5 && fA + 0.2 < fB` → `fA − fB`;
  - `fB = 0` → `fA`;
  - `fB < 0.5 && fB + 0.2 < fA` → `fA − fB`;
  - else 0.
- **Pair query** (0x0078F460 → 0x0071A1F0):
  - level = range class A − B (`+0x9C`; artillery 2, others 1, INFERRED). Then −1 if adv ≥ 0.5, +1 if adv ≤ −0.5, and
    −1 if B is infantry with `+0xE4` set (UNKNOWN flag, taken as clear).
  - men = card men × (1 − mean of the unit's previous casualty means). This is 0 at the start, so men = card men.
  - kill rates 0x0078D200 with r = adv and r2. adv is 0 at query time, because setup runs before the advantage is
    computed (INFERRED).
  - rout point = 0.4 × (1 − morale / highest morale of all units) × men.
  - 0x0078E8A0 with fuzz: 64 engagements, with each of the six inputs (melee and missile rate and rout point, both
    sides) × 0.8 and × 1.2.
- **Engagement** (0x00759860, CONFIRMED; `campaign::autoresolve::engagement`). The limits are men − rout point.
  - Pre-phase: level > 0 → B loses `min(menB, menA × max(kA) × level)`. Level < 0 → A loses
    `min(menA, remB × max(kB) × |level|)`.
  - Missile rates are used unless `kA_mis ≤ kA_mel || kB_mis ≤ kB_mel`; then both sides use melee.
  - Each step: `dA = min(remB × 0.1 × kB, remA)` and `dB = min(remA × 0.1 × kA, remB)`, each at least 0.05 and at most
    the men.
  - End: `casA > 0.9·limA && casB ≥ limB` → 1 (draw). `casA ≥ limA` → 1 if `casB > 0.9·limB`, else 2 (B wins).
    `casB ≥ limB` → 0 (A wins).
  - Output: the casualty fractions of both sides.
  - `battle::autoresolve::engage` in the battle crate is a simplified version and differs; I reported this to the
    manager.
- **Statistics** (0x90 bytes; 0x007338D0 per pair, 0x00734EF0 combining, 0x00796620 flip):
  - [0..2] = P(A wins), P(B wins), P(draw).
  - Then 16 floats per side: casualties when the own side wins / the other side wins / a draw, their population
    deviations, the overall mean and its deviation, then 8 naval floats.
  - Combining: probabilities and overall means are averaged over all entries. Each per-outcome mean is averaged over the
    entries where that outcome's probability ≠ 0. Deviations are the spread of the entries' means (0 for one entry).
  - Per A unit: its pairs. Per B unit: its pairs, flipped. Battle: all pairs.
- **Probabilities**:
  - Weighting (0x007DD740): `a = pA·SA`, `b = pB·SB`, `k = 1/((SA+SB)/2·pD + a + b)`; then pA = k·a, pB = k·b, and pD
    is the rest.
  - Prediction (0x007930A0): `x = SA/(SA+SB)`. The draw chance is 0.05 if x is outside [0.25, 0.75], 0.2 if outside
    [0.4, 0.6], else 0.35; normalised.
  - Blend (0x0075DBA0): if `|(p_side + pD/2) − (q_side + qD/2)| ≤ 0.5` for both sides, blend with
    w = max(clamp([52]), |`+0x18`|), where `+0x18` = 0 (INFERRED). Otherwise take the prediction and set `+0x3C`.
  - Stars: d = best star rating of A − that of B. The side with more stars is × (|d|·[54] + 1). Then normalise.
- **Winner** (0x007690C0):
  - One resolver LCG step. The gaussian 0x010E19B0 uses a local LCG seeded with that `state >> 16`: polar method,
    `t = ln(s)·−2/s`, root by the bit trick `((bits − 0x3F800000) >> 1) + 0x3F800000`, then × y × [48].
  - Clamp to ±[47]; `roll = ([47] + g)·0.5/[47]`, kept in [0.01, 0.99].
  - winA = pA + pD/2 and winB likewise (1 if pD = 1, 0 if pA + pB = 0). winA < [53] forces B to win; otherwise
    winB < [53] forces A.
  - Buckets, in order:
    - `roll < pA·[45]` → A, type 0;
    - `< pA` → A, type 1;
    - `< pA + pD·winA` → A, type 2;
    - `+ pD·winB` → B, type 2;
    - `+ pB·(1 − [45])` → B, type 1;
    - `+ pB·[45]` → B, type 0;
    - past all of them → type 3.
- **Advantage** (0x0078FE60(1)): `SA/SB > 1 → SB/SA − 1`, else `1 − SA/SB` (0 if equal or a side is 0). adv > 0
  means B is stronger.
- **Side losses** (0x0074FF40):
  - Wipeout: the loser loses all men in every unit (slot 3) if adv > [80] (A) or adv < −[80] (B).
  - Otherwise the units are sorted by category key (0x00793B00: infantry 0, dragoons 1, cavalry 2, camels 3,
    artillery 4, elephants 5, other 6), descending, with a stable merge sort (0x00708630).
  - Modifier for unit k of n: `(k/n·0.2 + 0.8)·(1 − |adv|/2)` when (A and adv < 0) or (B, adv > 0 and B won); else 1.
    Then slot 2 runs for each unit.
  - Reshuffle (land, `+0x14` clear). Each of the first ⌊n/2⌋ units gets one roll; `< 0.5` (or none chosen yet at the
    last of them) gives the unit ⌊losses/2⌋ men back. Then the same number of units from the second half lose
    ⌊losses/2⌋ more; they are picked by rolls < 0.5 that cycle through the list.
- **Unit loss rate** (slot 14, 0x0078FA30), from the unit's own statistics, with `fuzz(m, sd) = m + rand·sd − sd/2`.
  - Loser:
    - If P(own win) + P(draw) ≥ 1, use the draw mean when P(draw) > 0 and the mean is in (0, 1); else the fallback.
    - Otherwise use the "other side wins" mean if it is in (0, 1); else the fallback.
    - Fallback = fuzz(overall), then 0x007C7430: − diff·(own p + pD/2) when the own side's probability is higher.
    - If `+0x3C` is set: 0x007C71B0. When the casualty gap of the battle means is beyond ±0.35, ∓0.4·|d|, and the loser
      ∓0.2·|d| more; floor [71].
    - 0x007C7300: if the own side is much stronger (|adv| > 0.45), × (1 − 0.2|adv|).
    - Floor [71].
    - Type 0: `+ (1 − r)·[59]`. Type 2: `− [60]·r`.
    - Pursuit 0x0074F170, if r > 0.5 or the enemy has shock units. The chance is `(1 − lead)·enemy shock / own S`
      (halved for the unit's own shock units) + lead, where lead = the enemy's |adv| when the enemy is stronger. A hit
      sets `r = 1 − [82] + rand·[82]`.
  - Winner:
    - Use the own-win mean (the draw mean if P(own win) = 0); else the fallback. Here 0x007C7430 adds
      (1 − own p − pD/2)·|diff| when the enemy's probability is higher.
    - Minimum `[70] × modifier`, ×2 unless the own side is stronger by ≥ 0.2. A rate below the minimum becomes the
      minimum. Otherwise 0x007C7300 applies: × (2 − 0.8|adv|) if the enemy is much stronger (|adv| > 0.35);
      × (1 − |adv|) if the own side is much stronger (> 0.45).
    - `+0x3C` → 0x007C71B0 with floor [70].
    - Type 0: `− (1 − r)·[59]`. Type 2: `+ [60]·r`.
  - Clamp to [0, 1].
- **Applying the rate** (slot 2, 0x0074F840):
  - A loser whose own side is much stronger (|adv| > 0.6) loses at most 0.55.
  - Fuzz with [46], clamped to 0.99: `r += rand·f − f/2`, then clamp.
  - A winning unit with the general (card `+0xC0` in {1,2,4,5}) × 0.35.
  - losses = round(men × r). A winner that would lose everything loses half.
  - × the AI difficulty factor 0x00793010 ([77] / [78] / [79]; none applied here).
  - men −= losses. The general falls with a unit at 0 men (card `+0x5C` cleared).
- **After the battle** (round 4, item 2; the post-battle state machine 0x008F7880, states 1–7 read in full):
  - state 1: the autoresolve 0x008AE5A0 (or the real-time battle), then statistics for both sides (0x008CA100 codes 7–14);
  - state 4: statistics per participating character;
  - state 5: the battle report and event queue (0x008D3320 → 0x008A8AA0, one campaign-RNG step);
  - state 6: 0x008EB330 → 0x008EB370 → 0x009DB270 per side. This rebuilds the sides' forces from the battle's unit
    list. A unit whose force is gone is placed into a new force on a free spot within 3 map units of the previous
    entry's position (0x00B38F10, radius 3.0);
  - state 7: the attacker resumes its interrupted order (move: 0x009549B0; attack: 0x00916980).

  **No step moves the loser away: the original has no retreat after a campaign battle** (CONFIRMED that none of the
  aftermath states does it; INFERRED for the whole game). The defender can only withdraw *before* the battle
  (`can_withdraw`, a pre-battle option of the battle setup UI 0x009A2420; the AI's choice is not ported). The model
  matches this: the loser stays where it is.

  Not modelled: the attacker resuming its order (state 7), and the force rebuild of state 6. The latter matters only
  when a unit's force no longer exists, which cannot happen in the model because units belong to forces.
- **Resuming the order** (round 5, item 3; code `battles.rs` `PendingBattle::resume`, `commands::army_outside`).
  Post-battle state 7 of 0x008F7880 (CONFIRMED) moves the attacker on:
  - without a fought battle (the defender withdrew): its interrupted move or attack goes on (0x009549B0 move,
    0x00916980 attack);
  - after a won battle of type 10 with a follow-up target (+0xBC): it attacks that target.

  The model has no standing multi-turn orders, so the one case it can express is modelled: an army ordered into a
  hostile settlement first fights an enemy army standing outside it (not garrisoned, within 3 map units:
  PROVISIONAL distance). If it wins, it goes on into the settlement. A settlement without defenders is occupied;
  one with defenders gives the next battle. Test `the_winner_goes_on_into_the_settlement_after_beating_the_army_outside`.
- **Open**:
  - the resolver's RNG seed (we draw from the campaign RNG);
  - naval autoresolve and captives;
  - siege building damage;
  - the class 0x15 of 0x007CB8A0;
  - the battle types of the cavalry factor.
- **Values in the shipped DB** (eur): [45] 0.2, [46] 0.1, [47] 3, [48] 1, [52] 0.5, [53] 0.225, [54] 0.06, [59] 0.1,
  [60] 0.1, [70] 0.15, [71] 0.55, [80] 0.6, [82] 0.2.

## Religion conversion (round 9, manager's addition; code `ntw_sim::campaign::religion`)
CONFIRMED from the exe unless tagged.
- **Place:** the round-end region update `0x00AB42F0` (from the faction's round-end economy `0x008BC650`) →
  `0x00AB3FF0` → `0x00AB4070`: population growth (not modelled: the model's population is static), then the conversion
  `0x00A63FE0`, then the town wealth `0x00AB4410(1)`.
- **Strength per religion:** each character standing in the region (area query `0x00BA6840`) whose agent type is a
  missionary (+0x2C in 6..10, `0x00F9C710`) adds his rank (`0x00A198D0`) to his agent's religion (+0x1A0 = `agents` #9:
  in spa catholic and Protestant missionaries `align_anti_french`, orthodox `align_pro_french`; elsewhere `rel_hindu`,
  `rel_islamic`, ...); then each religion of the breakdown adds the region set's `conversion` bonus for it
  (`0x00E1EF70(religion, 1)`; the religion-keyed junction `effect_bonus_value_religion_junction`, now mapped as
  `BonusKind::Religion`). Sources in the data: spa universities (`conversion_pro_french` 1 / 2) and chapels, churches,
  cathedrals (`conversion_anti_french` 2 / 4 / 8); none in the other campaigns' buildings.
- **Flows:** religion i with s = clamp(strength, 0, 9) > 0 takes from each other religion j with d = s − strength(j)
  > 0: x = clamp(1.9 + 0.05 d² + mod(j, i), 1, 10) (mod = `religion_conversion_mods`, matrix `0x00877A60` at model
  +0xF98: row j, column i, INFERRED from the record order), amount = min((200 + 0.004 × pop × share(j)) × 2.4 × x,
  pop × share(j)); people as truncated integers; share += people / pop, floored at 0. Tweaks `conversion_constants_*`
  (built-in defaults, `0x0042E390`..): points_base 1.9, zeal_mult 0.05, points_mult 2.4, pop_mult 0.004, pop_added 200.
- **Public order:** the religion factor subtracts the owner's faction-level `conversion` for its state religion
  (`0x008B1B10`); the region's own buildings do not count there (with them, two spa regions with churches would
  differ from the stored factor; without them the 93 / 124 spa rows that matched still match).
- **Saves:** eur (`nr1 → auto_nr1`, `auto_nr4_t4 → orig_over_nr4_0252`): no conversion source, shares unchanged in
  the original and the model (648 / 648). spa: the only vanilla pair is 3 turns apart (`orig_fr_t1 → orig_fr_t1_b`):
  27 shares changed in the original; the model reproduces none exactly: some regions drift with no source at the
  start (Algarve, Castilla la Nueva, Cataluña, +0.19 pro-French in Cataluña), others had a missionary whose
  movements over the 3 turns are not in the files. So the spa drift has another driver: open (leads: the region
  PO refresh `0x00AA9C10`, which also calls the breakdown hook `0x00AA4860`; `0x00A4C9B0` from `0x00A46A70`; the
  spa campaign scripts). Missionaries: their religion and the formula are CONFIRMED, the AI's use of them is §6.

## Turn order (round 9 item 4; code `turn.rs`)
CONFIRMED from the exe (the calls of each function in order; event posters found by their name getters
`0x008BDxxx` and vtables):
- **Faction turn start `0x008F2620`** (called from `0x0096C160` / `0x008F4120`): (a) campaign-script hooks on +0x724
  (`0x00A072D0` / `0x00A07440`); (b) for a human: achievement counters; (c) +0x860 countdown → flag +0x85C;
  (d) `0x008ED860`; (e) **every character `0x00A24EB0`** (uses the campaign RNG); (f) **every region `0x00AAE820`**:
  slots (`SlotTurnStart` each, `0x00AAE8D0`, a port's naval queue `0x00B71FB0` right after its slot), the land queue
  `0x00B71FB0`, `0x00A249A0` (per slot: a chance roll on the campaign RNG from two building effects; INFERRED
  building-driven spawns), the settlement's virtual +0x98 → `0x00A22480`, then **`RegionTurnStart`** (vtable
  `0x01376DB8`); (g) the diplomacy manager `0x00B71FA0`; (h) **research availability `0x008F91F0`**; (i) faction
  `0x00A18A70`; (j) the character recruitment pools `0x00A24C90` (+0x6F4); (k) every region `0x00A6F720`; (l) eur only
  `0x008F2E10`; (m) `0x008DA210` (characters with skill 5 or flag +0x4E0); (n) +0x800: AI `0x008B4B70` / human
  `0x008F2480`; (o) every character's pending order `0x009653D0` (+0x9C, resumed or dropped); (p) +0x940 → `0x00A25310`;
  (q) forces in transit (+0x90C list → `0x00A2A080` / `0x00A29F20`); (r) **`FactionTurnStart`** (vtable `0x01356328`,
  name getter `0x008BDE80`) last.
- **Faction turn end `0x008BD0F0`**: human flags; **`CharacterTurnEnd`** per character (`0x009DA210`, vtable
  `0x0136A748`); per region `0x00A786C0` (slot virtuals +0xAC, **`RegionTurnEnd`** vtable `0x01376CFC`, then the
  bankruptcy offset +0xEC −1); the +0x7C0 list's virtual +0x2C (forces → **`UnitTurnEnd`**, posted by `0x008BD4B0`,
  vtable `0x013569C4`); the diplomacy manager `0x00B29940`; +0x828 −1; **`FactionTurnEnd`** (vtable `0x0135626C`).
- **Round-end economy `0x008BC650`** (per faction, from `0x00948CF0`): settle `0x00BABE30`; `0x00A44DB0(f, 1)`; per
  region `0x00AB42F0` (discontent growth); `0x008BC9C0`; characters matched by `0x009DA190` → `0x00A0C6F0(3)`; per
  region `0x00AB3DA0` and the queue flag `0x00A78620`; **the relationships `0x00B29100`**; `0x008F3C40`; `0x008AB200`,
  `0x008ABF50`.

The model now follows (a)–(r) for what it models: characters (`CharacterTurnStart`, action points) → regions (slots,
construction, land queue, `RegionTurnStart`) → research availability → `FactionTurnStart`; and at the end
`CharacterTurnEnd` → `RegionTurnEnd` (+ offset) → `UnitTurnEnd` → `FactionTurnEnd`. Before, `FactionTurnStart` came
first and `RegionTurnEnd` closed each region update. Not modelled: (a)–(d), (g), (i)–(q) and `0x00A249A0`.

## Diplomacy rules (round 9 item 3; code `ntw_sim::campaign::treaties`)
CONFIRMED from the exe unless tagged. The relationship record is S1_LEFTOVERS.md §1 (29 fields, 24 attitude factors).

**Attitude events.** 30 {limit, drift, value} triples at campaign model +0xFAC (`0x00B52070`): built-in defaults
(`0x0042F110`, triple constructor `0x00518210`) overridden by the `diplomacy_attitudes` table, which the game does
not ship, so the defaults are the values (`treaties::ATTITUDE_EVENTS`): e.g. alliance (80, +1, 30), war (−200, −2,
−140), peace (0, +2, +120), trade (60, +1, 15), trade_broken (0, +2, −20), state_gift (0, −1, 100),
war_dragged_by_ally (−130, −2, −70), trade_embargoed (0, −2, −40).

**Factor writes** (each on one factor: the relationship + 8 + 0x28 × slot): set `0x00B69640` (value, drift, limit,
limited), add `0x00B02CB0` (value += add, then drift / limit, clamped toward the limit: min for a positive drift,
max otherwise), reset `0x00B69620`. Per turn `0x00B290D0`: value += drift, clamped the same way when limited. So
`trade_embargoed` (−40, drift −2, limit 0) is 0 after one turn: the exe's own values.

**Attitude category** `0x00B0DBA0`: half-way between neighbouring `diplomatic_relations_attitudes` rows (integer
halves; the upper two minus 1): total ≤ −65 hostile (0), ≤ −22 (1), ≤ 21 neutral (2), ≤ 64 (3), else very friendly (4).

**Actions** (which relationship and factor each call writes is read from the call sites: the factor = ECX offset):

| Action | Owner → target | Target → owner | Others |
|---|---|---|---|
| War `0x00B26700` | `war` set (`war`, or `war_dragged_by_ally` with the ally in #5); a patron clears #28; an alliance / patronage / protectorate broken first; access #3 = 0; trade broken; during friendship #15: backstabbing `0x00B0E420`; payments dropped; stance war | `war` set likewise; payments dropped; the access it gave is "abused" (`0x00B0DA90`: cancel grievance + `abused_military_access` add) | X hostile to the target (cat 0): `declared_war_against_enemies` += 15; very friendly (4): `declared_war_against_friends` −15 |
| Backstabbing `0x00B0E420` | — | `peace_treaty` set: value −3·n·t, limit −5n (n = treaties the declarer broke, manager +0x1C; t = friendship turns) | same subculture as the target: value −5n·t, limit −25n; then n += 1 (3 when t = 10) |
| Peace `0x00B262C0` | `war` += `peace` (or `peace_dragged_by_ally`), #5 cleared, stance neutral, #15 = 10, #10–#13 = 0 | the same | allies' suspended access restored (`0x00B0D0C0`) |
| Alliance `0x00B29DA0` | `alliance` set, stance allied, #6 = 20, #15 = 10 | the same | — |
| Break alliance `0x00B13840` | `alliance` set from `break_alliance` (0, −2, 0), stance neutral, #6 = 0, access from #17 restored | `alliance` reset to 0, `alliance_broken` set (−40, +2, 0), stance neutral, protectorate lines cleared | while #6 runs: same subculture as either, `cultural_alliance_broken` value −#6 − 5k, limit −5k (k = alliances broken, manager +0x14) |
| Trade `0x00B55090` | `trade` set, #2 true | the same | — |
| Break trade `0x00B29BB0` | `trade_broken` set from `break_trade` (no effect), `trade` cleared, #2 false | `trade` cleared, `trade_broken` set (−20, +2, 0), #2 false | — |
| Embargo `0x00B28DB0` | trade broken first; #27 = 10 | `trade_embargoed` set | — |
| Military access `0x00B44550` | #3 = −1 or += turns, #24 = #3, #15 = 10, #25 = 0 | — | — |
| Cancel access `0x00B67BD0` | #26 += 50 − 10e / 60 − 6e / 70 − 7e/2 / 90 − 5e/2 (granted 5 / 10 / 20 / other, e = #25), #3 = 0; manager +0x20 += a third (`0x00B67B20`) | — | — |
| State gift `0x00B44590` | — | `state_gift` += trunc((100 − current) × x × 0.01), x = min(100, `state_gift_multiplier_linear` × m + `…_quadratic` × m² / √(GDP sum)) | — |
| Protectorate `0x00B105C0` | stance 4, alliance factor; the protectorate's other ties broken | stance 3; the patron's income line = tribute (`0x00BBCD00` = a faction value / 5, not read) | — |

**Per-turn update** `0x00B29100` → `0x00B29170`, called from the faction's round-end economy `0x008BC650` after its
regions (CONFIRMED place). Skipped when the owner or the target is out of the game (+0x824; INFERRED: no regions
and no forces). Order: factors drift; #6 −1; access: timed −1 with #25 and #19 +1, indefinite #19 +1, none #19 = 0;
#17 items of dead factions dropped, their turns −1; #15 −1, then 10 while allied or given access; at war #10 / #11
(armies in the other's regions, GDP and army balance: `CampaignModel::war_balances`, army value PROVISIONAL) and
#12 / #13 +1; #7 toward 0 (−2 / +1); protectorate tribute and patron income lines; payments: turns −1, finished
removed (`0x00B298F0` also adds to faction +0x954, a counter, only between two humans); #26 −2; #27 −1. Then the
manager's +0x20 −1.

**Checked against the vanilla saves** (`economy_check` `ECON_DIPLO=<next save>`): for the pairs whose stance did
not change, every factor of the next save equals this save's after one drift step, except where an action of that
turn wrote it: `nr1 → auto_nr1` 661 drift steps matched, 50 differ; `auto_nr4_t4 → orig_over_nr4_0252` 775 matched,
16 differ; spa `orig_fr_t1 → orig_fr_t1_b` (3 turns) 33 matched, 0 differ. The differences are new trade agreements
(15 + 1 drift = 16), `allied_with_enemies` adds, a war declaration; records of factions out of the game do not drift
(the skip rule above). Not compared: the computed factors religion (15), government_type (16), faction_leader (21),
enlightenment (22), which the exe rewrites (`0x00B1B540`, `0x00B1B5A0`, `0x00B71EC0` from the effects 0x52
`diplomacy_bonus_faction_leader` and 0x7E `diplomacy_bonus_enlightenment`): ported in the round 10 additions
below (and the government change's drift in rounds 12-13, the last corrected in round 13 and not re-checked on the saves: no vanilla save has one)

**Round 10 additions** (code `treaties.rs`):
- **Computed factors** (`0x00B71EC0`, from the round start `0x0096C050` → `0x008BAF30` per faction, CONFIRMED place):
  - `faction_leader` (slot 21) is reset to trunc(the target's faction effect `diplomacy_bonus_faction_leader`, 0x52);
  - `enlightenment` (22) is reset to trunc(its `diplomacy_bonus_enlightenment`, 0x7E);
  - the protectorate tribute and patron income lines are refreshed in the same call.

    `religion` (15) is reset at setup to `diplomatic_relations_religion` #2 for (own religion, target religion)
   (`0x00B1B540`). `government_type` (16) is reset to `diplomatic_relations_government_type` #3 for (own, target)
   (`0x00B45A40`, `LEA ECX,[EBX + 0x288]` / `PUSH [EAX + 0xC]` / `CALL 0x00B69620`; `0x00B69620` sets the
   value, drift 0 and the limited flag off). A government change (`0x00B1B5A0`) goes the **other way**:
   value = #2, limit = #3, drift +2 — see below and `CampaignModel::change_government`.

  Checked on the vanilla saves (`economy_check` `ECON_CFACT=1`, both factions in the game):

  | Factor | Matches |
  |---|---|
  | religion | 506 / 506 (`auto_nr4_t4`) |
  | government | 506 / 506 |
  | enlightenment | 506 / 506 |
  | faction leader | 484 / 506 |

  The 22 leader misses are all relationships towards France: the model sums −9 where the saves hold +9. The −9
  comes from the leader post's ministerial effect level (item 11). The spa saves match 12 / 12 in all four factors.

### The row key of `diplomatic_relations_government_type` (round 14: the PAIR, CONFIRMED — round 13 said "single key")

Round 13 read `record_index` (`0x0047AA40`, `RET 4`) as taking one government key, which left open whether the
table row is `f(own government)` or `f(own, target)`. **It is the pair, and our pair-keyed map was right.** Two
independent lines of evidence:

**From the bytes (CONFIRMED).** `0x0047AA40` is `DATABASE_TABLE<RECORD>::record_index` — one template member
folded across every table (the per-table assert string names `DIPLOMATIC_RELATIONS_GOVERNMENT_TYPE_RECORD` at
the `0x00B45A40` site), so its single argument is the table's **key**, not the whole row selector. The key it is
given is built by concatenation:
- `0x00B1B5A0`: `0x00B1B5C6` pushes `DAT_013305F8`, `0x00B1B5CB`/`0x00B1B5D1` read the record **owner's**
  government key (faction +0x70C's string at +0xB0) and `0x00B1B5F1` the **argument** (the new government), and
  the two `0x004F1200` calls splice them into the one string `0x00B1B61B` looks up.
- `0x00B1B190` builds the key the same way (`0x00B1B1AC` the separator, `0x00B1B1D3` `0x004B4A50` the record's
  other faction, `0x00B1B1BC` `[ESP+0x34]` = its argument, `0x00B1B20A` the lookup), as does the setup writer
  `0x00B45A40`.
- **The separator literal is the same global, and worker 1 already identified it**: `DAT_013305F8` = `";"`
  (DB_BUILDERS.md §2b, `fatigue_effects_tables`: "the two strings … combined into the record key `s1 + ";" +
  s2`. The separator literal is at 0x013305F8 = ';'"). The key is `govA + ";" + govB`.
- Round 13's "three `UniString` copies … dead code" was a misread of `0x004F1200`: it is called here with
  **three** pushed arguments twice, so it is the concat helper (`RET 8`), not a copy ctor. Nothing is dead.

**From the shipped rows and the saves (CONFIRMED, no RE).** The table is 16 rows = 4 government types × 4, and
**both columns vary inside a single row of the other axis** (`economy_check` `db <data>
diplomatic_relations_government_type ssii`, and the new `ECON_GOVPAIR`):

| fixed | #2 takes | #3 takes |
|---|---|---|
| own = `gov_absolute_monarchy` | −100 / −50 / 0 / +50 | −30 / −10 / 0 / +15 |
| target = `gov_republic` | −100 / −50 / 0 / +70 | −30 / −15 / 0 / +30 |

So **#2 is not a function of one government** (this was the sub-question round 13 left open). The save agrees
without reference to the table: grouping the 506 stored `government_type` factors by either single column gives
**3–4 distinct values per government** (only `gov_empire`, whose row is all zeros, is constant). A
single-column model is arithmetically impossible. The pair-keyed `#3` reproduces **506/506, 462/462 and 12/12**
on the nine vanilla saves.

Code: `CampaignRules::government_relations` stays `BTreeMap<(String, String), (i32, i32)>` — now documented as
pair-keyed, with a lock test `economy_fidelity::government_relations_are_keyed_on_the_pair` (4 × 4 shape, no
column constant, absolute→republic = (−100, −30)). `ECON_GOVPAIR` prints the whole table, the save's pairs and the
distinct-value count per column.

### Government change `0x00B1B5A0` (round 13 full decode)

Read from the bytes, not from the round 10 note. `0x00B1B5A0` is 263 bytes, `0x00B1B5A0` .. `0x00B1B6A4`;
everything below is CONFIRMED unless tagged. `ECX` = the relationship record, the one argument = a
`CA::UniString` government key.

| address | what it does |
|---|---|
| `0x00B1B5A9`-`0x00B1B5B8` | the table getter `0x00E1A7E0`, whose own string is `"Loading database: %s\n", "diplomatic_relations_government_types_table"` — so the table is `diplomatic_relations_government_type`, cached on the database object at `+0x4DC` |
| `0x00B1B5BD`-`0x00B1B5D1` | `operator->` (`0x00445230`, `MOV EAX,[ECX]`) then `*(record->0 + 0x70C + 0xB0)` |
| `0x00B1B5D7`-`0x00B1B60E` | **builds the composite key** (round 14 corrected this row; round 13 read it as dead copies): `0x004F0800(ESP+0x20, DAT_013305F8)` assigns the separator literal, then two **three-argument** `0x004F1200` calls — `(ESP+0x2c, ESP+0x1c, gov_own) [ESP+0x38 = the argument]` and `(that, ESP+0x14, gov_argument)` — i.e. `gov_own + SEP + gov_argument`. The two destroys free the temporaries, not the key, which lands at `ESP+0x10` |
| `0x00B1B613`-`0x00B1B61B` | `record_index(table + 0x14, ESP+0x10)` — a **single-argument** hash lookup (`0x0047AA40`, `RET 4`) on that one composite string |
| `0x00B1B624`-`0x00B1B65C` | the row: `*(table->0x10 + index * 4)`; a missing key logs `"In table %S: '%S' is not a valid key..."` and then dereferences a null row (`0x00B1B660`), so the key must exist |
| `0x00B1B660`-`0x00B1B67E` | `limit = [row + 0xC]`; `mag = [campaign_model + 0xFAC + 0x94]`; `drift = limit < [row + 8] ? -mag : +mag` |
| `0x00B1B684`-`0x00B1B68F` | `0x00B69640([row + 8], drift, [row + 0xC])` on `record + 0x288` |
| `0x00B1B694`-`0x00B1B6A4` | destroy the key copy, `RET 0x4` |

**Answers to the round 12 questions.**

1. **The drift magnitude — CONFIRMED 2, and it is neither a constant nor a lookup in the row:** it is
   the `government_type` attitude event's own drift field. `0x00B27FF0` → `0x008BAF10` is
   `return *(this + 0xFAC)`, the attitude events array. `0x00B52070` builds it by copying `0x5A` (= 90)
   dwords = **30 triples of 12 bytes**, then applies `diplomacy_attitudes` overrides **in alphabetical
   order of the event name**, each at `+0xC * index`. The triple is `{limit, drift, value}`:
   `0x00518210` stores its three arguments into dwords 0, 1 and 2 of `this`, and `0x00B45A40` reads
   `events[25]` (`war`) as `[0x12C]` (limit), `[0x130]` (drift) and `[0x134]` (value) and calls
   `0x00B69640` with them in the order `[0x134]`, `[0x130]`, `[0x12C]` = `(value, drift, limit)` =
   `(-140, -2, -200)`, the shipped war row.
   So `0x94` = 12 × 12 + 4 = **triple 12, field +4 = the drift of `government_type`**, built-in 2
   (`0x0042F110`). The exe takes the field raw — no magnitude.
2. **The sign / limit logic — CONFIRMED.** It is not a drift-target rewrite: one `0x00B69640`
   (`value`, `drift`, `limit`, `limited = 1`) on one factor, with `value = [row + 8]` (**#2**) and
   `limit = [row + 0xC]` (**#3**), and the sign `+` unless the limit is below the value. Every shipped
   row has #3 above #2, so the factor climbs two a turn from #2 to #3 — the shock and then the
   recovery, the opposite sense to the round 12 port. The clamp is the ordinary per-turn one
   (`0x00B290D0`: `value += drift`, then `min(limit)` for a positive drift, `max(limit)` otherwise).
3. **The `religion` factor is NOT rewritten — CONFIRMED.** The only factor written is `record + 0x288`,
   which is slot 16 in the documented layout (`8 + 0x28 * 16 = 0x288`). Religion is slot 15 at `0x260`,
   which is exactly what the setup writer `0x00B1B540` touches (`LEA ECX,[EDI + 0x260]`). The round 12
   UNKNOWN is closed: a government change leaves `religion` alone. A `scal 0x288` sweep over
   `0x00B00000` .. `0x00B80000` finds exactly three writers of the factor — `0x00B1B190`,
   `0x00B1B5A0` and the setup `0x00B45A40` — and none of them touches `0x260`.
4. **One direction only — CONFIRMED.** The single caller is `0x00B1B100`, which walks the changed
   faction's **own** relationship records (stride `0x848`; `[EBP + 0xC]` records at `[EBP + 0x10]`) and,
   for each one, calls `0x00B1B190` on it, then resolves the counterpart's own record with `0x00B64C50`
   and calls `0x00B1B5A0` on **that**. `0x00B64C50` (listing `0x00B64C50`) returns the first record of a
   list whose **`+4` field** equals its argument — the record of the counterpart's list that has the
   changed faction as its *target*. So the factor written is **the counterpart's attitude towards the
   changed faction**, and it is written once per owned record onto the same record (the loop is
   redundant; `0x00B64C50` falls back to the list base when nothing matches — a quirk, not ported).
   The changed faction's own records are handled by `0x00B1B190`, its structural twin, which sets only
   the **drift and the limit** (`0x00B69A70`, `LEA ECX,[EDI + 0x288]`) and leaves the value alone — so
   the own side does move, but never its value, and that pass is not ported.
   `record + 4` is the record's **target**: `0x00B29170` (the per-turn update) tests
   `*(record + 4 + 0x824)`, the out-of-the-game flag, while `0x00B29100` tests the list owner's.

### The own-side pass `0x00B1B190` (round 15: the contradiction is resolved — there was none)

Round 13 declined to port `0x00B1B190` because "its literal pushes set `drift = row #3, limit = ±2`,
contradicting its twin" and looked like an argument-order artifact. **The bytes settle it: the reading
is right and there is no contradiction**, because the two functions have different signatures.

| | `0x00B1B5A0` (the twin, ported) | `0x00B1B190` (the own side) |
|---|---|---|
| writer | **`0x00B69640`** (`0x00B1B68F`) | **`0x00B69A70`** (`0x00B1B27E`) — its only caller |
| signature | 3 args: `+0x1C = arg1` (**value**), `+0x18 = arg2` (**drift**), `+0x20 = arg3` (**limit**), `+0x24 = 1` | **2 args: `+0x18 = arg1` (drift), `+0x20 = arg2` (limit), `+0x24 = 1`, and `+0x1C` (the value) is NEVER written** |
| pushes (last = first arg) | `PUSH EDI=row+0xC`, `PUSH EAX=±drift`, `PUSH [ESI+8]=row+8` | `PUSH ESI=row+0xC`, `PUSH EAX=±drift` |
| so it means | `value = #2`, `drift = ±2`, `limit = #3` — shock the value, let it recover | `drift = #3`, `limit = ±2` — aim the drift at the steady value, leave the value alone |

So round 13's transcription of the pushes was **correct as read**; what looked like an argument-order
artifact is two different setters. `0x00B1B190` is the "let the own side drift towards its steady value"
half of the same operation, which is exactly why it exists: `0x00B1B100` calls it on the changed
faction's own records and `0x00B1B5A0` on the counterpart's.

The `±2` is **not a literal** either, now confirmed on both sides: `0x00B1B25C` / `0x00B1B26B` call
`0x00B27FF0` and read `[EAX+0x94]` — the same `government_type` attitude event's drift field its twin
reads at `0x00B1B66F` / `0x00B1B67E` — and `NEG` it on the low branch (`0x00B1B267`).

The row key is the **pair**, and here it is built the other way round: `0x00B1B1D3` `0x004B4A50` gives
the record's *other* faction, `0x00B1B1D8`/`0x00B1B1E2` read that faction's government key
(`+0x70C`, `+0xB0`), and it is concatenated with `[ESP+0x34]`, the argument — so the own side looks up
`(other, argument)` while its twin looks up `(own, argument)`. Round 14's pair-keyed map covers both.

**Why it is still not ported**, now for a different and sharper reason than round 13 gave:
- **The sign is chosen by an unnamed field.** `0x00B1B254` is `CMP ESI,[EDI+0x28C]` — the row's `#3`
  against `record + 0x28C`, which is **factor 20's `+0x04`** (factors are 0x28 apart from `record+0x8`,
  so factor 20 sits at `0x288`). That is not the value (`factor+0x1C` = `record+0x2A4`), the drift
  (`+0x18`) or the limit (`+0x20`); it is a fourth, still unnamed field. Round 13 called this
  "`factor + 0x04`" without pinning the absolute offset; it is now pinned.
- **No vanilla save reaches it** (only a peace-deal item calls `0x00B1B100`), so nothing would check a
  guess — unchanged.
- **New side finding, to resolve before anything is ported.** The setup writer `0x00B45A40` calls
  `0x00B69620` at `LEA ECX,[EBX+0x2B0]` — **factor 21**, not the factor 20 (`record+0x288`) that both
  change-path writers touch. The same function walks the factors 0x28 at a time from `record+0x8`
  calling `0x00B0C220(ESI, ESI+0x3C)`, and separately seeds `+0x2B0`, `+0x198` and `+0xA8`. So "the
  setup's fixed value for `government_type`" and "the factor a government change writes" may not be the
  same slot — which would also explain why the round 10 save check (506/506) could not discriminate.

**Ledger addition:** | what round 13 called a contradiction | different signatures —
`0x00B69A70(drift, limit)` vs `0x00B69640(value, drift, limit)`; the value is untouched by design |
CONFIRMED (`0x00B1B27E`, `0x00B69A70`) | | the `±2` literal | `[campaign_model+0xFAC+0x94]`, the
`government_type` event drift, read on both sides | CONFIRMED (`0x00B1B261`, `0x00B1B270`) | | own-side
row key | `(other, argument)`, the mirror of its twin's `(own, argument)` | CONFIRMED (`0x00B1B1D3`) |
| remaining blocker | the **sign** reads `record+0x28C` = factor 20 `+0x04`, unnamed; and the setup seeds
factor 21, not 20; no save exercises the path | UNKNOWN, so not ported |

**Callers — why no vanilla save exercises the path.** One caller only, and it is a **peace-deal item**:

```
0x00B449F0  a peace-terms deal applier (797 bytes; 5 callers 0x00B58890 0x00B58C00 0x00B58A30
            0x00B58A10 0x00B58560, plus 0x008AB6C0 0x00C18BF0 0x00B4F090)
  +0x6x   FUN_008BEAA0(faction, x)                <- the only caller of 0x00B1B100
           0x008BEAA0 (517 bytes, 1 caller): the post-deal bookkeeping for a faction
             -> FUN_008F3610 / FUN_008B3B50 (UI refresh; a 9-dword record rotation)
             -> FUN_00B1B100(new_government_key)  <- the key comes from *(deal + 0x98) + 0x18
```

`0x00B449F0` is the terms applier this file already names for peace terms (it calls `0x00B2B810`,
`0x00A64AC0`, `0x00A1B6C0`, `0x00AB3DF0` and blits the `0x00AF3C00` results). The new government key
arrives as a deal-item field (`*(FUN_008CFC20(0) + 0x98) + 0x18`), and the call is made only when the
recipient faction is the local human (`*(FUN_006649C0() + 0x818) == the deal's faction`). So a
government change is a **peace-treaty option applied by the deal applier**, not a UI panel action and
not a Lua binding; there is no engine path that changes a government on its own, which is exactly why
no vanilla save reaches `0x00B1B5A0`. `0x008BEAA0` also calls `0x00A1B4E0` and `0x009D1E40` in the same
block, so a deal can bundle the change with other effects.

**Ledger.**

| item | answer | tag |
|---|---|---|
| drift magnitude | the `government_type` event's drift, `[campaign_model + 0xFAC + 0x94]`, raw; 2 | CONFIRMED (`0x00B1B66F`, `0x00B1B67E`) |
| sign | `+` unless the limit is below the value (`0x00B1B665`); +2 on every shipped row | CONFIRMED |
| value / limit | value = `#2`, limit = `#3` — the reverse of the setup's `#3` | CONFIRMED (`0x00B1B684`, `0x00B1B686`, `0x00B69640`) |
| `religion` rewritten? | no, only `record + 0x288` (slot 16) | CONFIRMED (`0x00B1B689`, `scal 0x288`) |
| directions written | the records whose **target** is the changed faction, resolved by `0x00B64C50`; the own side only gets drift + limit from `0x00B1B190`, which is not ported | CONFIRMED |
| in-the-game test | none in `0x00B1B5A0`; only the per-turn drift skips (`0x00B29170`, target `+0x824`) | CONFIRMED |
| callers | `0x00B1B100` ← `0x008BEAA0` ← `0x00B449F0`, the peace-deal terms applier: a peace-treaty option | CONFIRMED for the chain; the item's UI label INFERRED |
| own-side drift + limit pass `0x00B1B190` | drift + limit from the counterpart's government row, value untouched; not ported — **round 15 resolved the contradiction** (it calls the 2-arg `0x00B69A70(drift, limit)`, not `0x00B69640`, so `drift = #3, limit = ±2` is correct as read); still blocked on the unnamed field the sign test reads (`record+0x28C` = factor 20 `+0x04`) | CONFIRMED behaviour, open in code |
| the row key | `record_index` takes **one** key (`0x0047AA40`, `RET 4`) and all three writers pass a single government key, so at runtime the row is a function of one government — yet the file table is `ssii` with 16 rows and the port keys its map on the **pair**. The round 10 save check (506 / 506) shows #3 does not vary with the pair, so the setup value is right, but #2 — which only a government change reads — is unverified | **SUPERSEDED by round 14** (§The row key of `diplomatic_relations_government_type`): the key is the composite pair and the map is right |

- **Allies called into a war** (`0x00B268B0`, CONFIRMED flow). Every ally of the attacked side (stance 2–4, in the
  game, not already at war with the attacker) is asked, then every ally of the attacker:
  - a human ally gets an offer (UI event; not modelled, so a human ally is not called);
  - an AI ally decides through the AI (`0x00AAA950` → `0x00AAA920`, §6).
  - Join (`join_war`): war on the enemy on the ally's behalf, i.e. #5 = the ally, the `war_dragged_by_ally` event
    (−70 drifting to −130), and `0x00B0CCE0` on both alliance records (access turns saved in #17, access −1).
  - Refuse: the alliance with the called side is broken (`0x00B27500`, `0x00B13840`).
  - PROVISIONAL AI decision: join, unless also allied to the enemy.
- **Treaty money** (PROVISIONAL place, the round-end economy):
  - regular payments move their amount from the payer to the payee each turn (economy line 0);
  - a protectorate pays its patron a fifth of its revenue (`0x00BBCD00` = Σ three economics lines / 5; the three lines
    are INFERRED to be taxes, trade and other). These are economy lines 6 and 2, kept in #8 / #9.

**Not modelled / open:** the AI's decisions (accepting deals, joining an ally's war: §6); `allied_with_enemies`
(`0x00B0CE30`, callers not traced); the spy / sabotage / assassination factors (`0x00B11ED0` .. `0x00B12010`: the agent
worker's hooks); `annexed_territory` (`0x00B25FF0`, `0x00B62250`); `threatened` (`0x00B72EC0`); peace terms
(regions); the manager counters and the changed relationship fields are not saved (save-compat).

## Naval autoresolve (decoded round 9, ported round 10)
The naval resolver is the land resolver's pipeline (`0x007ADF50`: setup, winner, losses, captives) run on ship units.
What differs is held in two vtables and the ship data. CONFIRMED from the exe unless tagged:
- **Unit classes** (`0x007CA330` builds them): land unit `0x0070D4E0`, vtable `0x01342908`; ship unit `0x0070D580`,
  vtable `0x01342944` (base `0x0070D740`, vtable `0x013428D0`). Slot 11 (+0x2C) returns the land unit and slot 13
  (+0x34) the ship unit (`MOV EAX,ECX` or `XOR EAX,EAX`). Ship slots: 1 the pair query `0x0078F5A0`, 2 the damage
  `0x0074FA70`, 3 sinking `0x007DD800`, 5 capture `0x007CD570`, 6 kills `0x00750C40`, 7 `min(n, 10)` `0x00750C60`. The
  ship class has no slot 14 (the land loss rate); its loss rate `0x0078F6A0` is called from slot 2.
- **Ship unit fields** (`0x0070D580`): +0xC8 the battle ship, +0xCC the campaign card, +0xDC the highest morale of the
  battle (passed in), +0xE8 the potential `0x00758950` = (ship +0x1D8 + ship +0x1DC) × (ship type +0x7C >> 1) × 3, × 1.5
  for unit categories 0x18 / 0x26. Over the ship's parts (`0x00F055F0`: 18 named parts — side panels, prow and stern
  panels above / below water, masts 1–3 and bowsprit, top / middle / bottom / front / back / top-gallant sails,
  magazine — each a triple from the ship record +0xF0..+0x1C4): +0xD0 = mean of the first values, +0xE0 = Σ second
  values (hit points), +0xD4 = (1 − mean of the ship's 4 damage fractions +0x6C..+0x78) × that sum, +0xD8 = mean of the
  third values. The DB has the 18 triples in `unit_stats_naval` #61–#114 (e.g. 2_Decker_74: (3, 127, 0.8), (3, 198,
  0.8), (4, 330, 0.9), ...), but the runtime ship record's layout is not the builder's (+0x1D8 / +0x1DC / +0x1C8 do
  not map to the builder offsets): **not read**.
- **Pair engagement** (`0x007201A0`, vtable `0x013428A8`): crews A / B = the sums of the three crew counts (`0x0057E1E0`),
  hulls A / B = +0xE0, damage so far = mean of the 4 damage fractions × hull, all scaled by the unit's previous casualty
  means as on land (crew × (1 − m), damage = min(hull, (1 + m) × damage)). Kill rates `0x0078D6C0`: a = (1 + Mnav·|r| if
  r < 0) × potential A, b = (1 + Mnav·r if r > 0) × potential B (each 1 if 0), rate A = Knav·a/b, rate B = Knav·b/a,
  with tweaks `unit_combat_query_naval_outnumbering_multiplier_tweak` Mnav = 2 and
  `unit_combat_query_base_kill_rate_naval_tweak` Knav = 0.2. Rout points `0x0078FFA0`: 0.4 × crew × (1 − ship morale
  (+0x1C8) / highest morale). Fuzz runner `0x0078EE10`: 64 engagements with the six inputs × (1 ∓ 0.2), as on land.
- **Engagement loop** `0x00759B20`: hull damage of each side grows by the other side's rate × the other side's
  remaining hull fraction (A: dA += (hullB − dB) / hullB × rateB, likewise B), after a pre-phase by the range level;
  crew lost = damage / hull × crew; it ends when a side's crew loss reaches its rout limit or its damage reaches
  (1 − part mean third value) × hull (the sink point), with the land code's 0.9 "shaken" draw rule. Output: result,
  A crew fraction, B crew fraction (both divided by A's crew: a quirk of the original), A and B hull fractions (1 when
  past the sink point).
- **Statistics:** 8 more floats per side (the "naval floats" of the 0x90 block) hold the hull-fraction means and
  deviations in the land order (own win, other win, draw, deviations, mean, deviation).
- **Loss rate** `0x0078F6A0` (from the unit's naval floats): pick the outcome mean as on land with `fuzz(m, sd)`, adjust
  by `0x007C7430`, then by victory type: winner × (1 − v) with v = `autoresolve_minor/normal/major_naval_victory_win_percent`
  (0.14 / 0.16 / 0.18; types 2 / 1 / 0), loser r + (1 − r) × `…_lose_percent` (0 / 0.2 / 0.4); clamp 0..1; × 0.95 for
  category 0xB.
- **Damage** `0x0074FA70`: r += fuzz `autoresolve_ship_damage_fuzziness` (0.3); × 0.4 when its two flag arguments are set (meaning UNKNOWN); a coin picks
  which pair of the card's 4 damage fractions (+0x94..+0xA0) takes the full r and which a random part; if the larger
  damage is below `autoresolve_ship_damage_required_for_sink` (0.875) the crews (+0xA8..+0xB0) lose r × (0.6..0.95) of
  their numbers, × the AI difficulty factor; otherwise the ship sinks (all damage 1, flag +0xC4), or, with the two flags set, two damage fractions
  drop to the card's +0x60 / +0x68 and r halves.
- **Sinking** `0x007DD800`: damage pairs raised to 0.81..0.90 plus random amounts. **Capture** `0x007CD570`: damage ×
  clamp(`autoresolve_ship_damage_capture_multiplier` (0.35) − 0.2 + rand × 0.4, 0.25, 0.6), crews × (0.3..0.6) of their
  maxima, the ship changes sides (+0xCC, captor +0xD0). Captives `0x0074F710` (`autoresolve_base_best_ship_kills_to_capture`
  0.3, `…_chance_of_not_capturing_ship` 0.4) via `0x0075C090`: not read.

**Ported (round 10; code `ntw_sim::campaign::naval`, `battles::autoresolve_naval`).**

Inputs, CONFIRMED:
- **Ship state** = `NAVAL_UNIT` #5 `SHIP_DAMAGE_INFO` v2: f32 ×5 damage (card +0x94..+0xA4), i32 ×3 crews
  (+0xA8..+0xB0), i32 ×3 full crews (+0xB4..+0xBC), u32 guns (+0xC0), bool sunk (+0xC4), u32 full guns (+0xC8).
  - The layout follows from the exe's uses: the sinking sets the five damages to 1 and zeroes the crews and guns; the
    damage step reduces +0xA8..+0xB0.
  - Values agree with the saves: crews (30, 30, 144) = `unit_stats_naval` #17..#19 of a 74, crew sum = the `UNIT` men,
    guns 74 / 122 / 38 = the ship's rating.
  - Loaded into `World::ship_states`. A ship without an entry gets its type's crews.
- **Hull and sink weight** = Σ / mean of the 18 part triples of `unit_stats_naval` (#61..#114). The engagement only
  uses the sum and the mean, so the part order (not read) does not matter.

The runtime ship record behind `0x00758950` / `0x0078FFA0` (the battle ship object `0x006B1530` +0x204) was not
decoded in the time box. Its +0x1D8 / +0x1DC are not `unit_stats_naval` columns: the row reader zeroes them, and
the two post-load writers found for those offsets belong to the database object. Stand-ins, PROVISIONAL:
- **Potential** = guns × (crew >> 1) × 3, in place of the exe's (r+0x1D8 + r+0x1DC) × (model+0x7C >> 1) × 3 with
  ×1.5 for two classes.
- **Morale** = `unit_stats_naval` #28: 100 for a first rate, 95 for a 74, 85 for a frigate, 65 for a brig.
- **Range level** = 0.

Ported as decoded:
- the pair query (the rates, rout, 64-sample fuzz, engagement with the "both by A's crew" quirk);
- the crew and hull statistics (`sample_stats` / `combine_stats` on each);
- the shared probability step (`autoresolve::finalize_probabilities`, now used by both resolvers), the winner roll;
- the wipeout (`0x007DD800`) and per-ship damage (`0x0074FA70`) with the naval victory percents and the sink rule.

What happens after the battle:
- sunk ships and ships without crew are removed, and a navy left empty is destroyed;
- men = crew total;
- navy against navy is no longer refused; a navy and an army still do not fight.

Not ported: ship captures (`0x0074F710` → `0x0075C090` → `0x007CD570`), the 0.95 factor of class 0xB, armies aboard
a sinking navy.

Checks:
- `naval_engagement_and_battle`, `navies_fight_through_the_model`;
- `economy_check` `ECON_NAVAL=1` on `auto_nr4_t4`: the Ottoman galleys against the Spanish line fleet are wiped
  out, and the Spanish lose 0–40 men per ship.

## Britain income check (round 5, item 1; tool `ntw_ai --example economy_breakdown -- <human> <faction> [rounds] [ai|noai]`)
Per round, before End Turn, eur, Britain's figures:

| run | taxes | trade | other | land upkeep (plain) | naval upkeep (plain) | net |
|---|---|---|---|---|---|---|
| Britain AI (France human), rounds 0–3 | 3586–3588 | 3430 → 3478 | 1700 | 3492 (3880), mod −10 | 2871 (4420), mod −35 | +2353 → +2403 |
| Britain human, rounds 0–3 | 3586–3588 | 3430 → 3478 | 1700 | 4268 (3880), mod +10 | 3755 (4420), mod −15 | +693 → +743 |
| round 3 notes (before the effects were wired) | 3324 | 3430 | 1700 | 3880 | 4420 | +154 |

The difference is the **difficulty handicap**; nothing is wrong:
- The upkeep modifiers decide it:
  - `campaign_difficulty_handicap_effects` at normal gives the AI (flag false) land/naval upkeep −10 and gdp +10.
  - It gives the human (flag true) upkeep +10/+10.
  - Britain's saved base (#54) adds naval −15, and its techs and buildings add a further naval −10.
  - So AI Britain pays 3492 + 2871 = 6363 and human Britain 4268 + 3755 = 8023: 1660 less for the AI, which is exactly
    the gap between +2353 and +693.
- The tax rise 3324 → 3586 is the minister bonus (`tax_bonus_minister` +3).
- The trade rise is route accumulation.
- The AI's own orders (recruiting, building) are spending, not income: with the AI on, Britain's treasury is 7753
  after round 1 instead of 9353, while income stays the same.
- That the flag-true set is the human's is CONFIRMED by the vanilla saves: `effects_check` gives the stored #55 for
  the human, and the same basic set for every AI faction. The human-Britain game run (about +700) agrees.
- The old +200 came from the plain upkeep, before the effects store was wired.
- Not wired yet: `gdp_mod_all` (+10 for the AI at normal; item 4 below).

## Resolved / open
| question | answer | tag | code |
|---|---|---|---|
| campaign_variables storage / getter | above | CONFIRMED | `CampaignRules::variables` (same override order) |
| Tax efficiency | above | CONFIRMED | `economy::tax_efficiency` |
| Effective tax rate per class | above (character / tech bonuses 0: not modelled) | CONFIRMED formula; bonuses PROVISIONAL | `economy::effective_tax_rate` |
| Region taxes | both classes on GDP + town wealth | CONFIRMED | `economy::region_taxes`, `class_taxes` |
| What `REGION` #9..#19 are | above | CONFIRMED | `Region::base_gdp/gdp/town_wealth/town_wealth_growth/wealth_growth_offset/discontent_growth/tax_exempt` |
| Where tax levels are stored (CAMPAIGN_PLAY Q4) | `GOVERNORSHIP_TAXES` u8 rates; the exe reads +8/+9 | CONFIRMED | — |
| When income is paid (part of CAMPAIGN_PLAY Q1) | at the round end, for all factions | CONFIRMED | `turn.rs` `TurnStep::Economy` |
| GDP / town wealth growth recomputation | above; all 238 start-position regions reproduced | CONFIRMED (slots_gdp_values, chain mods, commodities not modelled) | `economy::recompute_region` |
| Town wealth growth per turn | `max(0, tw + growth)` | CONFIRMED | `settle_round` |
| Other income: major-power flag | `FACTION` bool before `CHARACTER_ARRAY` (+0x524) | CONFIRMED (83 factions in 7 startpos) | `FactionDetails::major` |
| Trade income | routes: GDP + commodity (Σ volume × price, prices #4) + accumulated, 0 when blockaded; spa_france home sales; supply from trade fleets at nodes (0x00BC9930) | CONFIRMED (83/83 start-position factions exact; node supply 7/7); re-spread, new-route candidate set and blockade radius PROVISIONAL; prices not moved | `economy::trade_routes_value`, `campaign::trade` |
| Bankruptcy | treasury 0, counter (saved as `FACTION_ECONOMICS` #3), capital growth offset, desertion | CONFIRMED (desertion gate +0x50C PROVISIONAL; prestige and payment cancellation not modelled) | `economy::settle_round`, `bankrupt_desertion`, `World::bankrupt_turns` |
| AP bonus: who qualifies | everything in an army or navy (force type 0/1), any commander kind | CONFIRMED (mapping to character AP INFERRED) | `turn::force_action_point_factor` |
| Upkeep / recruitment cost modifiers | spec above | CONFIRMED formula; effects 0 | `economy::faction_upkeep` (plain sum) |
| Public order structure, garrison repression, automated policing | above | CONFIRMED | `economy::public_order` |
| Other public order sources: education, religion, gentlemen, war results | modelled (round 4) with loaded religion shares, class bases and gentleman attributes | education / war CONFIRMED; gentlemen skill INFERRED; religion table and sign PROVISIONAL | `economy::public_order` |
| Public order: ministers, characters and traits, techs, events, difficulty | faction sum of the effects store: class-keyed `happiness_*` and `repression_gov_type` / `repression_ministers` / `repression_gov_building` (round 5) | CONFIRMED source (0x008EDF70 via 0x00A67530); per-factor slot mapping INFERRED | `economy::public_order`, `Effects::faction_sum` |
| Recruitment points | land: effect 32 + home-region effect 33 in the capital + mp_eur AI France +1 | CONFIRMED (faction-wide effects 0) | `CampaignModel::recruitment_points` |
| Where trade accumulation and bankrupt turns are saved | INTERNATIONAL_TRADE_ROUTE +0x3C, FACTION_ECONOMICS #3 | CONFIRMED | written by save-compat |
| Autoresolve: where the `autoresolve_*` variables are read | through a pointer to the variable array in the resolver (offset 4·index) | CONFIRMED | `autoresolve::ArVars` |
| Autoresolve (land): potentials, pair engagements, statistics, probabilities, winner roll, advantage, wipeout, unit loss rates, reshuffle | above | CONFIRMED (inputs marked INFERRED above: side A = attacker, adv = 0 at query time, range class, `+0x14`/`+0x18`/`+0x3D`) | `campaign::autoresolve`, `battles.rs` |
| Retreat after a campaign battle | none: the loser stays (aftermath states 1–7 read); the attacker resumes its order (not modelled) | CONFIRMED absence / INFERRED | `battles.rs` |
| Autoresolve: captives, naval, siege damage, star ratings | not ported | open / PROVISIONAL | — |
| Autoresolve difficulty handicaps | [74]–[79] by the human's campaign difficulty | CONFIRMED formulas; difficulty source INFERRED | `autoresolve::potential`, `ai_loss_factor` |
| Validation against vanilla saves | eur after 3 turns, spa turn 1 and after 3 turns (round 6) | see the round-6 checks | `economy_check`, `effects_check` |
| `units` #6 meaning | recruitment time in turns (new item turns = UNIT_RECORD +0x34) | CONFIRMED (round 4) | `UnitRules::turns` |

## Save writer (for the manager / save-compat owner)
Round 9 (diplomacy): the model now changes the relationship records (factors #1, #2, #3, #6, #7, #10–#15, #17, #19,
#24–#27) and keeps three manager counters (`World::treaty_breaks` +0x1C, `alliances_broken` +0x14,
`access_cancel_marks` +0x20 of `DIPLOMACY_MANAGER`); `save.rs` writes only the stances, so a save keeps the loaded
values.

Round 9: capture damage changes `BUILDING` #0 health and a repair is a construction item of the building's own level
(the original writes its repair item, type 2 `0x00AE7010`, in the slot's `BUILDING_MANAGER`); loot changes `REGION` #12 / #14
and the classes' #12 war-results base.

The model now changes `REGION` #10 (GDP), #12 and #14 (town wealth), #15 (growth) and #17 (bankruptcy offset) every
round; `save.rs` does not write them, so a save keeps the loaded values. `World::trade_accumulated` and
`World::bankrupt_turns` are not saved (the original keeps them in its trade and economics records).

Where the original saves them (round 3, item 3; CONFIRMED from the writers and a save):
- **`World::bankrupt_turns`** = economics `+0x460`, the cannot-pay turn counter. It is the **last field (#3, u32) of
  `FACTION_ECONOMICS`** (writer `0x00BD46E0`). That record holds:
  - `history[]`, with the current index at +0x3F0 and the count at +0x3EC;
  - #1 i32 treasury (+0x3F4);
  - #2 u8[25] (+0x3F8..+0x45C, each dword through 0x00BC4F20);
  - #3 u32 (+0x460).
- **`World::trade_accumulated`** = the route's `+0x3C`. The path is
  `CAMPAIGN_MODEL/CAMPAIGN_TRADE_MANAGER/INTERNATIONAL_TRADE_ROUTES[]` (one per faction: utf16 faction key, then
  `FACTION_INTERNATIONAL_TRADE_ROUTES_ARRAY[]` of {`INTERNATIONAL_TRADE_ROUTE` v3, u32}). The route writer is
  `0x00AFD490`. A route record holds, in order:
  - u32 n, then n path points (i32, coord2d, u32 from, u32 to, bool);
  - bool;
  - u32[8], the commodity volumes;
  - **seven u32**: +0x28 route total, +0x2C commodity part, +0x30 resource part, +0x34 GDP part, +0x38 (equal to
    +0x3C in the saves seen; UNKNOWN), **+0x3C accumulated value** (0x00B05CC0 adds to it), +0x40;
  - u32[20];
  - u32 count, then 8-byte entries.

  Example (vanilla eur save `auto_nr4_t4`, spain route 0): 3548 = 3201 + 0 + 313 + 34. Which partner faction a route belongs to is not
  written as a key; it follows from the path's end points (port / settlement indices of `PORT_INDICES` /
  `SETTLEMENT_INDICES`; mapping not checked). The writer has to keep the record layout and set +0x3C (and +0x28 / +0x34)
  from the model.

## 0-B closing summary (round 10)
Pass 0-B (campaign rules fidelity) ends here. Tags as everywhere: CONFIRMED (read in the exe, or reproduced on the
vanilla files), INFERRED, PROVISIONAL (a tested stand-in), UNKNOWN. Evidence is vanilla files only: the 8 shipped start
positions and 9 vanilla saves (§Where I am).

**What the campaign model now does as the original does** (each a section above, with addresses and checks):

| Area | State | Evidence |
|---|---|---|
| Economy: taxes, other income, upkeep, GDP and town wealth growth, bankruptcy | CONFIRMED | 238 / 238 start-position regions; tax figures of the saves |
| Trade: routes, supply split, importer, accumulated value, prices | CONFIRMED (split `0x00BC26D0`, round 16) | trade income exact for every faction of the 9 saves; 352 / 352 save route volumes |
| Public order (13 happiness + 6 repression factors) | CONFIRMED | 1180 / 1184 classes; garrison factor 284 / 288 (militia flag #70) |
| Effects store (buildings, techs, government, ministers, traits, saved bonuses) | CONFIRMED structure | used by every rule above |
| Research: availability, rates, step, gates | CONFIRMED | every start position and save; AI rates whole steps |
| Recruitment and construction queues, construction cost | CONFIRMED (cost INFERRED: 141 / 143) | §Recruitment and construction queues |
| Turn order: round start, turn start / end, round-end economy | CONFIRMED | `0x008F2620`, `0x008BD0F0`, `0x008BC650` |
| Land autoresolve | CONFIRMED | §Autoresolve |
| Naval autoresolve | ported; ship potential, morale and range level PROVISIONAL | §Naval autoresolve |
| Capture: occupy / loot / liberate, damage, repairs, fortifications | CONFIRMED (AI choice PROVISIONAL) | §Capture |
| Diplomacy: attitude events, treaties, war / peace, allies called, per-turn update, computed factors, treaty money | CONFIRMED rules (AI decisions PROVISIONAL; money place PROVISIONAL) | drift 1,469 steps; computed factors 506 / 506 except the leader (484) |
| Religion conversion | CONFIRMED formula | eur saves unchanged as in the original |
| Action points, characters' campaign effects used by the rules | CONFIRMED factor | §Action points |

**Left open, by impact** (each with evidence and a tested fallback; rows in §Open items):
1. The AI's own decisions (§6, AI_RESEARCH.md §7 item 8): capture choice (PROVISIONAL occupy), repairs (PROVISIONAL
   eager), joining an ally's war (PROVISIONAL join unless allied to the enemy), deal acceptance.
2. Naval autoresolve inputs from the runtime ship record (potential, morale) and ship captures (item 8).
3. Movement path and road cost, contact distances (items 4, 5: the pathing worker's area).
4. France's faction-leader attitude (item 3 → item 11: the leader post's ministerial level).
5. The spa alignment drift that no conversion source explains (item 16): one 3-turn vanilla pair only.
6. Desertion gate (item 6): faction +0x50C is a loaded value, not written by any rule. The fallback uses the
   bankrupt-turn count.
7. Low: theatres, research army / navy mods, region recompute omissions, recruitment details, peace terms, and
   the government change's own-side drift + limit pass (`0x00B1B190`; the change itself is ported and CONFIRMED in rounds 12-13), and the 16 shipped `diplomatic_relations_government_type` rows (the exe keys a row by ONE government, the port keys it by the pair).

**For the save writer (save-compat):** `save.rs` does not write the following, so a save keeps the loaded values:
- capture damage to buildings and the fortification;
- repairs and fortification construction items;
- the relationship fields and diplomacy manager counters;
- religion shares;
- ship damage states;
- the region economy fields listed in §Save writer.

**Tools:** `economy_check` modes, each documented in its section: `ECON_TECH`, `ECON_PO`, `ECON_DIPLO`, `ECON_CFACT`,
`ECON_RELIGION`, `ECON_REBELS`, `ECON_DAMAGED`, `ECON_SHIPS`, `ECON_NAVAL`, `ECON_FORTS`.

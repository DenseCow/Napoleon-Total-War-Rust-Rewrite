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
Deal regions and technologies: traced and ported 2026-10-10 (UI_FIDELITY.md §4.9; `0x00B449F0` is
the settlement's change of owner, not a peace-terms applier; the deal leaves armies in place; no
turn gate). The
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
- **Recruitment details — CLOSED (CONFIRMED, ported).** The campaign cost is
  `units` #7 with the region's cost effects (not #4, not the experience-adjusted `0x00ED49A0`, which is the battle
  army-setup price), charged and refunded unchanged; a faction in debt may recruit; `units` #15 caps a unit type per
  faction; the recruitable population gate, charge and credit are ported (`population::RecruitmentPopulation`;
  vars 36 / 37, 0 in the shipped data). See §Recruitment cost and money.
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
    `agents::rank_original` (the `character.rank` seam). Check: `ECON_MAP=1 ECON_RELIGION_CHARS=1 ECON_RELIGION=nr16_spa_t2 nr16_spa_t0`
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
    - Peace terms: regions and technologies as deal items — DONE 2026-10-10 (UI_FIDELITY.md §4.9). The
     round 13 reading of `0x00B449F0` as a terms applier was wrong: it is the settlement's change of
     owner (see the correction in §Diplomacy rules).
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
| 17 | Repairs (round 9, §Capture): **done 2026-10-15** (§Recruitment and construction queues, item types): the repair item's step and its ESF tail are CONFIRMED and modelled. Left: the walls' damage-object restore when a repair finishes (vtable +0xC0). | low | `0x00B78490` |
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
**REGION fields** (writer 0x00A51E30 SaveRegionRecord, child index → region offset): #9 +0xB8 base GDP, #10 +0xBC GDP, #11 +0xC0,
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
5/3/4/1/5/7: income = categories 5..11 (5 taxes, 6 always 0, 7 trade 0x00BB3490, 11 other income), expenses = 18..24 (19 land
upkeep, 20 naval upkeep; set through 0x0088F950 / 0x0088F990 → 0x00BBE930); the other groups are one-off spending and
refunds (0x00BAF500, 0x00BAF660, 0x00BB3810, 0x00BBE9A0).

**Running record** (worker polish62 2026-10-19, disassembly, CONFIRMED): the history's last record (`(cursor − 1) mod
10`, 0x00BABE00) is the running one, and the saver 0x00BD46E0 writes records `cursor − count .. cursor − 1`, so a
save's last item is it. 0x00BABE30 pays its sums (0x00BBE970 income, 0x00BBE910 expenses), then puts the next record
at the cursor (0x00BB8EE0 → 0x00B955C0: categories 8..11 and 18..24 copied, all others 0), cursor + 1, count + 1 (at
most 10), and ends with 0x00BBD7C0(0..2) at 0x00BAC093. 0x00BBD7C0(p) writes category p + 5 from the current state:
p 0 taxes (0x008F9E50 + 0x008D22F0), p 1 always 0 (it builds a vector through 0x00AA25C0 and discards it), p 2 trade
(0x00BB3490). Its other callers: TransferRegionToFaction (0x00A6512D new owner, 0x00A6513E old owner, rebels
included), the trade update tail 0x00BCB020 at 0x00BCE78E (every listed faction), the tax-exemption setter 0x00AAA140,
the tax-rate setters 0x00BC5380 (lower) / 0x00BC5440 (upper), and 0x00BC7080, called only from RunFactionRoundStart
0x008F1F60 step 1d after the upkeep trackers 0x0088F950 / 0x0088F990 (listed factions in the game). Trackers:
0x00BBE930(amount, g) adds expense group g into category 18 + g (21 callers; g 3 → 21 from 0x00A51230 / 0x00AAF220 /
0x00A4E460, g 4 from 0x0088CB40 / 0x00AF80F0 / 0x00AE5540, g 6 from 0x0088CB70 / 0x00AF8130, g 0 from 0x0099C2C0);
0x00BBE9A0(amount, g) adds income group g into category 8 + g (57 callers). Vanilla saves (9 checked): category 7 of
the running record = our `trade_routes_value` for every non-rebel faction; category 5 = our taxes but for small gaps
(it holds the taxes of the last refresh); 6, 8, 9, 10, 18, 22..24 always 0; 21 non-zero in some (France 2177 in
orig_fr_may1811). Ported: `economy::next_running_record`, `refresh_running_income` (tax rate, owner change, trade
update), `refresh_running_record_round_start`.

**Expense trackers and the record's payment** (worker recordpay 2026-10-19, disassembly + saves, CONFIRMED unless
marked): the round end pays the running record's sums. The faction's land / naval upkeep trackers +0x738 / +0x744
(0x0088F950 / 0x0088F990: when linked, book new − old into 19 / 20 through 0x00BBE930, then store; callers the load
0x0087B548 / 0x0087B551, PrepareFactionRoundEnd 0x008E10BA / 0x008E10C9, RunFactionRoundStart 0x008F2048 / 0x008F2057)
are saved as FACTION #46 / #47 i32, right after the bool array #45 (orig_fr_may1811: France 12019 / 563 = its record's
19 / 20). Each region's policing tracker, at population +0x14 ({+4 owner, +8 amount}, 0x00A2C9F0): the setter 0x00A51230
books new − old into its owner's 21; saved as POPULATION #4 i32 (the loader 0x00A456D0 reads it after REGION_FACTORS
and #1..#3); France's regions in orig_fr_may1811 sum to 2177 = its 21 (REGION_FACTORS #9 sums to 2143: #9 is the round
start's refresh, the tracker the last round end's). The owner attach 0x00AAF220 (from 0x00AA16E0 at 0x00A65103 in
TransferRegionToFaction) moves the amount from the old owner's record to the new one's; then 0x00BBD7C0(0) for both
owners with only null checks (reached only when the owner changes: TransferRegionToFaction returns at 0x00A64D13 for the same owner, see "Same-owner transfer"). PrepareFactionRoundEnd 0x008E1030 sets the
upkeep trackers to 0x008B2150's split and each region's tracker to the cost of a refreshed copy (0x00AB4400 →
0x00AB43A0: 0x00A72990, then 0x00A51230(copy +0x5C)). The load (0x00B95B90) zero-fills 8..11 and 18..24 and the
trackers book again; 11 is the other income (0x00BBC710 at 0x0087AD51). The script's `other_income_mod` (0x0097BB70
→ 0x00A15B30, lists on the campaign model +0xFA8): a positive amount books 11 (g3); a negative one, only for a
faction that is not human (+0x6E0), books 18 (g0, 0x00984B60; taken back by 0x00984B30 / 0x0099C2C0); the entries
are not saved, so the load drops them (`spa_napoleon` adds its boosts again from `OnModelCreated`, which the exe posts
after the record rebuild and ours fires there too: CAMPAIGN_DATA.md §4.1, `ScriptHost::open_campaign`). 9 holds
the live raid entries' takes (g1, 0x00B83C30 / 0x00B83360, taken back by 0x00AC8810, see the commerce raid notes below). The
treaty money (treatymoney, 2026-10-20, static trace, CONFIRMED) lives in the relationship records: a regular payment
item on the payer's record (list +0x7A4 / +0x7A8; ctor 0x00AE90A0 → 0x00AE5570, copy 0x00AE5540) books its amount
into the payer's 22 (expense g4, 0x00BBE930) and the payee's 8 (income g0, 0x00BBE9A0); its destructor (0x0086A120
→ 0x0088CB40) takes both back. Adding one (0x00B54FC0 → 0x00B55010) builds a temporary, pushes a copy and destroys
the temporary: one booking net. Items leave at their last turn (per-turn update 0x00B29170 → 0x00B298F0), at war
(0x00B27070 / 0x00B27250) and when a faction leaves (0x00B2B920 / 0x00B72900). The protectorate's tribute tracker
(record +0x7AC, ctor 0x00AE55D0, setter 0x00AF8130 books new − old) books its 24 (g6); the patron's income (+0x7B8 /
+0x7BC) books its 10 (0x00BBE9A0(new − old, 2)). Both equal 0x00BBCD00: (5 + 6 + 7 of the protectorate's running
record, 0x00B803A0) / 5, truncating (11 does not count). Set when the tie is made (0x00B104E0 / 0x00B10480), again
by the per-turn update (0x00B29170, after the war-momentum drift, before the payments tick: stance 4 → tribute from
the owner's record, stance 3 → income from the target's) and the round start (0x00B71EC0); cleared when the tie ends
(0x00B64C10, 0x00B54530, 0x00B0CA9A / 0x00B0CAB6) and when a faction leaves (0x00B2BA44 / 0x00B2BA60, 0x00B72A04 /
0x00B72A20). The load's record copy constructor (0x00AE9160) books them all again. All 0 in every vanilla save. Not
modelled: the bankruptcy cancel of payments (0x00B0E580, see `settle_round`). Ported: `economy::book_treaty_money`,
`protectorate_tribute_value`, `CampaignModel::change_treaty_money` / `refresh_protectorate_money` (treaties.rs, every
writer goes through them), `Relationship::payments_total`; tests `treaty_money_tests.rs` (booked once, paid once by the
settle until the last round, taken back by war, the tie's end and a faction leaving, refreshed by the round end and
start, rebooked by a load). Saved (treatysave, 2026-10-20, CONFIRMED): the relationship writer `0x00AFC340` puts the
tribute (the +0x7AC tracker's value +0x7B4, read through `0x004A5140`) at `DIPLOMACY_RELATIONSHIP` #8 i32, the income
(+0x7BC) at #9 i32 and the payments at #14 `REGULAR_PAYMENTS[]`, one item per list entry written by `0x00B7A4F0` as i32
amount (item +0x08) then u32 turns left (item +0x14), with no inner record; the reader `0x00AE90E0` reads them back in
that order through the booking constructor. Our loader (`details::relationship`) reads the same fields and
`save::write_treaty_money` writes them where the exe does, leaving an unchanged field as loaded; test
`diplomacy_counters::treaty_money_round_trips_through_a_save`. Evidence: 0 at #8 / #9 and no payment item in all
4,956 relationships of 5 vanilla saves and the eur_napoleon start position (`diplo_probe stats`). Ported:
`economy::set_upkeep_trackers`, `set_policing_tracker`, `move_policing_tracker`, `script_other_income_mod`,
`rebuild_running_record` (each load), `CampaignModel::prepare_round_end` (`TurnStep::RoundEndRefresh`),
`World::upkeep_booked`, `PopulationState::{policing_cost, policing_booked}` (saved; own save format 9);
`settle_round` pays Σ 5..11 − Σ 18..24 of the record, writing only 9. Tests: `expense_tracker_tests.rs`,
`economy_tests::the_round_end_pays_the_recorded_taxes`, `economy_fidelity::the_expense_trackers_rebuild_the_saved_running_record`
(6 vanilla saves: the rebuilt 9 and 18..21 equal the saved record, 11 too but for the spa factions; our land / naval
split of the upkeep equals the trackers for every faction; every region's policing cost equals REGION_FACTORS #9).
**A force's upkeep set** (navyupkeep 2026-10-20, disassembly + saves, CONFIRMED): each force's units are costed with
the force's own set, `0x008F9C50` → `0x008AFC00` on the force: a copy of the faction sum (the force's +0x7C faction
handle → +0x6FC; a land army's container army +0x98 → +0x24 instead when set, UNKNOWN, not modelled) with the
commander's own set summed in (the +0x74 commander handle → character +0x48C, traits + ancillaries); every upkeep
modifier (`upkeep_cost_mod_*_all`, category / class `upkeep_mod`, unit-key effect) is read from it. A force without a
commander counts 0 in both the upkeep and the ranking power: `0x008F9C56` resolves the +0x74 handle (`0x00898E80`)
and on null `0x008F9C62` returns EAX = 0 for the whole force, before the flag test at `0x008F9C69` (CONFIRMED
disassembly; ours skips such a force in `faction_upkeep_split_with` and `faction_powers`, test
`forces_tests::a_force_without_a_commander_costs_and_counts_nothing`). A unit raised alone gets a colonel and a dead
commander's force a new one (SAVE_COMPAT.md), so no force of the 12 evidence saves lacks one.
This was the Netherlands' missing input: its one fleet's admiral has `C_Admiral_Dutch_Seamanship` (−5 naval upkeep)
in auto_nr4_t4 / auto_after_c8 / orig_over_nr4_0252, so (100 − 1 − 5)% per ship gives 1006 = FACTION #47, where the
faction sum alone gave 1059. Ported: `Effects::force_set`, `economy::faction_upkeep_split_with`; `unit_upkeep`
takes the set (the ranking power passes an empty one). Test `economy_fidelity::an_admirals_traits_lower_his_fleets_upkeep`.

**Round end** (0x00948CF0, when the turn passes the last faction): for every faction 0x008E1030 (region pre-pass,
upkeep = Σ forces 0x008B2150 → land / naval, then the cannot-pay flag `treasury + income < expenses`, 0x00BBC7D0),
then for every faction 0x008BC650 (0x00BABE30: if it can pay, treasury += income − expenses; the record goes to the
history; then each region 0x00AB42F0: GDP / town wealth recomputed by 0x00A6AFC0 and `tw = max(0, tw + growth)`, town
wealth levels), one more 0x008BC650 for the rebels (world +0x1C, CONFIRMED: see the next paragraph), then 0x00BCB020 (trade). So **income is paid at the round
end for all factions**, not at each faction's turn start.

**Who gets a turn** (`PassTurnToNextActiveFaction` 0x00948CF0, disassembly, CONFIRMED; worker deadqueue
2026-10-17): after the turn end 0x008BD0F0 the walk goes on from the current faction (world +0x18) through the list
(+0x2C count, +0x30 data) and stops at the first faction whose +0x824 (out of the game) and +0x93C bytes are both
clear. Past the list the rebels (world +0x1C) get the turn with no such test. When the current faction is the rebels
(0x008CEEF0: no DB record), the walk restarts at the list's head and the round end above runs (the function returns
true). Not modelled: a set +0x93C on the rebels skips their turn; no byte store in the exe writes +0x93C (UNKNOWN
meaning). So a destroyed faction starts no turn, turn end or handover until it re-emerges; its round-end economy
still runs (both loops walk the whole list). Code: `CampaignModel::takes_turns` (ntw_sim turn.rs).

**Turn order and round start** (worker turnorder 2026-10-17; disassembly, CONFIRMED):
- **Order:** the turn list (+0x2C/+0x30) is the startpos FACTION_ARRAY in file order: the loader 0x0090BEB0 appends
  each faction as read and keeps the rebels (no DB record) out of it at world +0x1C; the writer 0x00910E40 writes the
  list back in that order. The current faction (+0x18) comes from a saved index, the rebels when unset. So the
  rebels always play last, wherever the startpos lists them, and their turn ends the round (0x00948CF0 above).
- **Round start** `RunCampaignRoundStart` 0x0096C050 (campaign +0xF5C): flag +0x39 = 1; `RunFactionRoundStart`
  0x008F1F60 for every listed faction; 0x008BAF30 for every listed faction; 0x00AAAF60 for each entry of the +0x34
  list; owner +0xF90 0x00BCB020(0) and +0xF84 0x008F5990; then a tail call of 0x008F1F60 on the rebels, so the
  rebels' FactionRoundStart is the round's last.
- **`RunFactionRoundStart` 0x008F1F60:** only for a faction that is not the rebels and has +0x824 (out of the game)
  clear: effect sum 0x008B16C0, per owned region 0x00AAE710 (population factors) and the 0x00A9BD00 flags, upkeep
  0x008B2150 → +0x738 / +0x744, 0x00BC7080 on +0xAC. Then for every faction: 0x008F2140 on +0x820, a per-region
  vtable call, and the FactionRoundStart script event (vtable 0x013563E4, name getter 0x008BDE70) posted through
  0x00E30700. So **an out-of-game faction still gets FactionRoundStart**, but no turn. Our model: the same order and
  events (`World::factions_in_turn_order`, ntw_sim world.rs; `TurnStep::RoundStart`, turn.rs; tests
  `turn_order_tests.rs`).
- **The other round-start steps** (worker roundstart 2026-10-17; decompilation and disassembly, CONFIRMED unless tagged):
  - Population: only the regions of a listed faction in the game are refreshed (`0x008F1F60` step 1, see above);
    the rebels' and out-of-game factions' are not refreshed at the round end either (`0x008E1030`, polish54 below). Model:
    `refresh_population_factors` (population.rs), the same call at both.
  - Computed factors `UpdateFactionComputedRelationFactors` 0x008BAF30: returns early on `IsFactionWithoutRecord`
    0x008CEEF0 or +0x824, else `UpdateComputedFactorsForAllRelations` 0x00B71E90 on faction +0x528 (every record,
    stride 0x848). The rebels are not in the list it walks either, so their records keep their factors. Ours reset
    them (the filter `in_the_game` lets the rebels through); fixed in `refresh_computed_factors` (treaties.rs).
  - **Siege attrition** (worker siegeattr 2026-10-20; disassembly, CONFIRMED unless tagged). `ApplyRegionSiegeAttrition`
    0x00AAAF60 runs once per region of the region manager (world +0x34; count +0x10, array +0x14), called at 0x0096C0F2
    between the computed factors and the trade update. The array is filled in startpos file order by
    `ConstructRegionManagerFromEsf` 0x00A4CE20, and the ids are **not** sorted in that order in mp_eur_napoleon, ita,
    mp_ita, spa, tut and every vanilla save (whose region ids are regenerated); eur, egy, mp_egy and our own saves are
    sorted. For each region: if the settlement (+0xFC) is under siege (virtual +0x98 = 0x00A8B9C0, besieger list +0x12C
    non-empty), `ApplySiegeAttritionToResidence` 0x00B796B0; then the same for each fort (+0x240 / +0x244; no shipped
    startpos or save holds a fort).
    - 0x00B796B0: the turns besieged +0x10C go up by 1 (also with no garrison). The garrison army is virtual +0x1C;
      none ends the step. n = min(turns, surrender turns +0x108); percent = trunc((n·n − 1) as u32 → f64 → f32 × 1.5),
      below 1 (unsigned: n = 0 wraps to a huge value) → 1, else at most 30 (0x00B79732). For each unit of the army
      (+0xB4 count, +0xB8 array) in order, the commander's own unit (army +0x74 → 0x00898E80 → character +0x2B0,
      0x00B79790) is skipped with no draw; any other draws r = `CaRng::int_range(0, percent)` inlined at 0x00B797B2
      (campaign RNG residence +0x3C → +0x8 → +0xFB8; (next16 × (p + 1)) / 65535, capped at p) and `RemoveUnitMenPercent` 0x00B5C0E0 removes
      r **percent** of the unit's current men (trunc(men × r / 100); all of them at r ≥ 100) through 0x008AABB0. A unit
      left at or below `unit_minimum_men` 0x008F68F0 is kept in a list; after the loop each listed unit's character
      (+0xF4) is killed (0x00A0C6F0(1)) and the unit deleted.
    - **Meaning CONFIRMED:** `GetSiegeTurnsUntilSurrender` 0x008AF520 returns +0x108 − +0x10C (0 when past) for the
      UI string "siege_turns_until_surrender" (0x009A2BDB). The cap is set when a siege starts, in
      `AddBesiegerToResidence` 0x00B112F0 (+0x108 = virtual +0xB4, +0x10C = 0): a settlement's is
      `GetSettlementSiegeSurrenderTurns` 0x00B57B10 (population < 25 000 → 2, < 100 000 → 3, < 1 000 000 → 4,
      < 10 000 000 → 5, else 6); a fort's 0x00B57B00 is fort +0x188 × 2 + 2. ESF SIEGEABLE_GARRISON_RESIDENCE: #2 cap,
      #3 turns besieged, #12 garrison, #13 besieger force ids (reader 0x00AF4B00, links at 0x00AF4D6D; writer
      0x00B7A5F0). None of the evidence saves holds a siege (#13 empty everywhere).
    - **Siege ends** (siegeattr fix rounds 1–2, 2026-10-20, disassembly, CONFIRMED unless tagged). Every end of a
      siege goes through `EndForceSiegeOfSettlement` 0x008BD550(force), a destroyed besieger's too: it destroys the
      siege object at force +0x94 (unlinking the force from the target's besieger list), calls the untraced
      0x00B084F0 on the target's character, then, unless the campaign is torn down (+0xFC5), the price update
      0x00BCB020(0) at 0x008BD65B and `RebuildRegionRecruitablesAndEffects` 0x00A6AE40 at 0x008BD678, then posts
      event 0xDF (model +0x9A8). The turns stay. 17 callers. Traced: (1) destruction: the military force's
      destructor 0x0088CE30 (sole caller 0x0089A2F3) calls it at 0x0088CE93 when force +0x94 is set and +0xFC5 is
      clear, then tail-jumps to the base teardown 0x0088ECC0; (2) the three capture variants 0x00B58560 / 0x00B58890 /
      0x00B58C00 end the captor's own siege after `TransferSettlementOwnership` (0x00B586E9, 0x00B5898A, 0x00B58D3B,
      each when force +0x94 is set); (3) `TransferSettlementOwnership` 0x00B449F0 fires the settlement's
      owner-changed event +0x24 at 0x00B44B31 (after 0x00A64AC0, before the slot pass 0x00B1B300 at 0x00B44B69; the
      whole function is skipped only when a passed character's army already belongs to the owner); each siege object
      listens (0x008DB7D0, its call 0x008DB84A) and ends its siege when the besieger's faction is the new owner or not
      at war with it (0x008CE9B0), an at-war besieger goes on; (4) peace: 0x00A25570 per commander of either side
      (from 0x00B262C0) ends his force's siege when the besieged settlement's owner (vt+0x14, compared by
      0x00898B20) is the other side (0x00A255C5); a force with no commander is not reached. PROVISIONAL (untraced
      callers): 0x008FF4B7, 0x008BEB42, 0x008AC130, 0x00B290B0, 0x008DB7BD (listener 0x008DB7A0), 0x008F801B
      (post-battle placement), 0x00A787C0, 0x00B584ED, 0x00B5881D, 0x00B58BD8, 0x00932794 (moving off is among
      them), and 0x00B084F0 / event 0xDF. Nor the siege start's other callers of 0x00B112F0.
    - **Model:** `Region::siege` (`SiegeState`: besiegers, surrender turns, turns besieged; ntw_sim campaign/siege.rs),
      read from and written to #2 / #3 / #13 (ntw_campaign world.rs `read_siege`, save.rs `write_region`; every vanilla
      startpos.esf and evidence save has the residence at version 1 with all three fields, so only a corrupt save
      lacks them and the load fails then; no evidence save holds a siege, so #13's writing rests on the writer
      0x00B7A5F0 and a round trip); the regions run in `World::regions_in_manager_order` (the region manager's order, kept by `World::region_order`, accessloss2).
      `CampaignModel::siege_attrition_round_start` runs at the exe's place in `TurnStep::RoundStart` (turn.rs) through
      the seam `siege.attrition`, drawing from the shared campaign RNG in the exe's order. The ends share
      `CampaignModel::end_force_siege` (siege.rs): `remove_force` (battles.rs), `take_settlement` for the captor,
      `change_region_owner` in both branches through `end_sieges_after_owner_change`, and `make_peace_rules`
      (treaties.rs) through `end_sieges_on_peace` (commanded forces only). When it finds a siege it runs
      `update_commodity_prices_with(false)` (the prices, `validate_commerce_raids`, the trade income); the
      recruitables rebuild needs no port (recruitables and building effects are derived from the slots when read).
      `RemoveUnitMenPercent`'s men × percent is a 32-bit IMUL that wraps (then unsigned /100),
      and ours wraps alike. The state hash covers the siege. Forts are not modelled (their residences' sieges are
      not read). Tests: siege.rs (13), turn_order_tests.rs `ending_a_siege_updates_the_prices_keeping_the_history`,
      save_compat.rs
      `siege_state_is_written`; the order: zoc_install.rs `vanilla_save_region_order_is_the_manager_order`. ORIGINAL BUGs: none.
  - The trade update 0x00BCB020 takes a byte flag: 0 here, 1 at the round end (`PUSH 0x1` at 0x00948E0E before the
    call at 0x00948E19 in 0x00948CF0). Both recompute the factors (first time), the prices #4 and the trends #7 (vs
    #5), then the route builder 0x00BC0960, 0x00B97310 twice and `RecomputeAllInternationalTradeRoutes` 0x00BBD870,
    then 0x00BD1020 and the route broadcast (model +0x498, slot +8). Only with the flag set: #5 ← #6, #6 ← price, and
    `AccumulateTradeRouteValue` over every exporter's routes (0x00B05CC0 from 0x00BCC5DA). Model:
    `update_commodity_prices_with(false)` in `TurnStep::RoundStart` (trade.rs); the model splits the route volumes
    where they are used (`trade_split`), so the route steps need no call here. Flag 0 also on a region loss
    (0x00BBE7C4) and when a port slot changes hands (0x00B2B84F): see "Price update on a region loss" below.
  - **Historical events** `0x008F5990` (this = model +0xF84; worker histevents 2026-10-19, decompilation and
    disassembly, CONFIRMED unless tagged). Model: `ntw_sim::campaign::historical_events` behind the seam
    `events.historical`, called in `TurnStep::RoundStart` after the trade update; data from
    `ntw_campaign::historical_events`.
    - *Candidates:* the manager posts the `HistoricalEvents` script event once (0x0087F960 → 0x00E30700). The vanilla
      `export_historic_events.lua` (required by every campaign's `scripting.lua` but `tut_napoleon`'s) has one handler
      per `events` row, in table order, calling `conditions.IsTriggerableHistoricalEvent(turn_of_year, key, year)`
      (#7, #0, #3) and on success `effect.historical_event(key)` (0x008C97B0, appends the record to the candidates).
      The condition (0x008A1DC0): record lookup 0x008C9850 (none: false); date = 0x008EDC80(year, u32((turn_of_year
      − 1) × 48) / turns_per_year) compared with the whole calendar date (0x00898AF0); a once-only record (#2, +0x18)
      is then inserted in the triggered list (0x008BBC50: binary search with CompareUniStrings, case-sensitive,
      UTF-16 code unit order; already present → condition false). So the list grows even when no faction receives
      the event. Saved as `HISTORICAL_EVENT_MANAGER` v1 / `TRIGGERED_DYNAMIC_EVENT_ARRAY` {utf16} (loader 0x0087F980,
      writer 0x00895050 `WriteHistoricalEventManagerEsf`; empty in vanilla saves); our import reads it
      (`details::triggered_historical_events`), our own save carries `World::historical_events_triggered`, and our
      ESF writer writes the block (`save::write_triggered_events`) where the exe does: CAMPAIGN_MODEL writer
      0x008EBAB0 calls 0x00895050 at 0x008EBC53, after PENDING_BATTLE (0x00895840) and before
      HISTORICAL_CHARACTER_MANAGER (0x00894F60); record v1 (0x0047AF90), array v0, one UTF-16 key per list node
      (first mgr+8, end mgr+0xC, next node+4, key node+8). CONFIRMED (disassembly; the empty block matches
      orig_fr_t1.save byte-for-byte, test `triggered_historical_events_are_written`).
    - *Per faction:* for each faction of the turn list whose human byte +0x6E0 is set: 12 forced and 12 random
      buckets; each candidate in order goes to forced[p] with p = 0x008C98C0(effect groups +0x48, faction, once
      flag) when p ≠ 13, else to random[p] with p = 0x008C98C0(view groups +0x78, ...) when p ≠ 13. Any forced
      event: the forced buckets 0..11 concatenated, the first 4. Otherwise the first non-empty random bucket and one
      campaign RNG draw (+0xFB8, the 0x005F0830 `uniform_below(n)` draw inlined at 0x008F5F58): the only draw, one per human faction with random and
      no forced candidates. ORIGINAL BUG: with more than 0xFFFF candidates in the bucket the inlined draw loops forever (the hang); ours draws among the first 0xFFFF (code says `ORIGINAL BUG`; no data has that many). Each chosen event whose text is not `n` gets its message 0x00A06DA0(model +0xFA0,
      faction, record): title +0x1C, text +0x20, picture 0x00F840E0(recipient's culture, `ui/eventpics`, +0x38). The
      effects are not applied (the effect text is the literal "Not Implemented: displaying event effects").
    - *Matcher* 0x008C98C0: starts at 13; per grouping in order, its lists: continents (a region the faction owns
      has that `regions` #1 continent) 4 / 10 (with / without the flag); cultures 1 / 7; empires (0 / 6) never
      match; factions 3 / 9; regions (the faction owns it) 5 / 11 stored without comparing (a later match can
      worsen the value; kept 1:1); subcultures 2 / 8; every other list only replaces a worse value.
    - *Empire bucket* (worker empirebucket 2026-10-20, CONFIRMED by disassembly; not an ORIGINAL BUG, 1:1 with
      nothing to fix): the empire loop (0x008C9AC3..0x008C9AD7) only steps its pointer while best > 0 / 6: no item
      is loaded, nothing is called or compared, so no data can make it match (the compiler kept the walk of an
      empty or constant-false test). The list holds `EMPIRE_RECORD` pointers looked up from `groupings_empires_junct`
      #1 by the groupings converter 0x00F56770 (a key not in `empires` makes it return 0, and the table builder
      0x00F47450 then drops the grouping). An `EMPIRE_RECORD` (converter 0x00F873D0) is a key, its
      `empires_onscreen_<key>` name and a region list from `empires_regions_junct`; nothing links a faction to an
      empire, and the converter is the only code that looks up an empire record (sole xref of its record_index
      assert, 0x00F56991). So there is no traced rule to restore (owning one region, all of them, or something
      else would be invented): an unimplemented feature, like the event effects' "Not Implemented" text. Vanilla:
      `empires` 6 rows, `empires_regions_junct` and `groupings_empires_junct` 0 rows, so no grouping names an empire.
      Ours: `grouping_priority` ignores `EventGrouping::empires` (test
      `the_matcher_takes_the_best_grouping_and_regions_overwrite`, both flags); a mod wanting empire buckets replaces
      the `events.historical` seam. Gap: our loader keeps a grouping whose junction member is missing from its
      table (any of the six lists); the exe drops it (BACKLOG).
    - *Records* (converter 0x00F875E0, constructor 0x00F74A90): a row is dropped when `events_onscreen_name_<key>` or
      `events_event_text_<key>` is missing, its season is not `season_summer` / `_winter` / `_spring` / `_autumn`
      (0x00F55970 returns 4), or a row of `events_effect_group_junct` / `events_view_group_junct` names a key not in
      `groupings`, of `events_hist_chars_junct` one not in `historical_characters`, of `events_to_policies_junction`
      one not in `policies`. Groupings: `GROUPING_RECORD` (0x00F41480), lists from the six `groupings_*_junct`.
    - *Vanilla data:* `events` 8 rows (all once = false), `events_view_group_junct` 8 rows (all `all_factions`: the 9
      major factions), the other event junctions empty; all 8 are kept and post messages (real-install test
      `the_install_historical_events_reach_a_human_faction`, ntw_campaign own_save_install.rs).
    - *Not modelled:* a mod's own `HistoricalEvents` handlers (the model runs the generated script's check over the
      table and reads whether `scripting.lua` requires it); the message's title, text and picture in the UI feed.
    - *Ghidra names (V5 plates):* `FireRoundStartHistoricalEvents` 0x008F5990, `FindHistoricalEventGroupingBucket`
      0x008C98C0, `LUA_IsTriggerableHistoricalEvent` 0x008A1DC0, `GetSeasonIndexFromKey` 0x00F55970 (strings
      `g_szSeason*Key` 0x01357E60..90; `g_szNotImplementedEventEffects` 0x0135827C, `g_szEventTextNone` "n"
      0x013582A8). The culture tested is the faction subculture's (`0x008F3490`: faction record
      +0x10 is its subculture, whose +0x10 is the culture), as `CampaignRules::faction_cultures`.
  - Tests: `turn_order_tests.rs` (`the_round_start_and_end_refresh_only_listed_in_game_factions_regions`,
    `the_computed_factors_skip_the_rebels_and_out_of_game_owners`, `the_round_start_price_update_keeps_the_price_history`).

**Campaign over** (worker campaignover, 2026-10-17; CONFIRMED by decompilation and disassembly unless tagged):
- **The flag:** campaign +0xFE9, one byte. Its only setter is `SetCampaignOverFlag` `0x008C30A0` (stores 1, nothing
  else); nothing clears it. Saved as `CAMPAIGN_MODEL` #18 (a bool; loader `0x00872550` at `0x0087393A` when the record
  version is above 2, writer `0x008EBAB0` at `0x008EBF3F`); false in the original's running-campaign saves (v10).
- **Writers (callers of `0x008C30A0`):** (1) `0x008AB6C0` destroying a human faction (human branch, single player and
  multiplayer alike); (2) the victory check `0x0096CD10`, only in multiplayer (campaign +0x1034): a human winner exists
  and another human is still in the game (winners get event +0x120, losers +0x138); in single player winning or losing
  by the victory conditions does not set it, the campaign goes on; (3) `0x008BEAA0` (from a settlement transfer) and
  `0x008DB1A0` (the faction's listener at +0x58), the two ends of a human faction's revolution (faction +0x814, set by
  `0x008EE3E0`, ended by `0x008BD4F0`), then event +0x108. Details (worker campover2, 2026-10-20):
  - **(2), the multiplayer victory** (`0x0096CD10`, CONFIRMED by decompilation): it runs only while its done byte
    (+0x38) and the flag are clear. Each human faction (+0x6E0) is sorted by the victory test `0x008F3DF0` into
    winners, losers or undecided (a non-human with +0x824 set goes in a fourth list, unused here). With a winner, it
    first sets byte +0x9C of the object from campaign +0xF70's first entry (vfunc +0x1C; the byte the
    `CCQ_CONTINUE_GAME_SELECTED` handler `0x00932B40` clears). Then for each winner, unless the victory notice is off
    (EPISODIC_RESTRICTIONS +0x125): in multiplayer the first other human in the faction list gets event +0x120 (a
    shared win, flag 1) when the campaign key (campaign +0x30) is `spa_napoleon`, the winner passes `0x008CE390` and
    both factions' +0x724 → +0x7C equal 5, else event +0x108 (a defeat, flag 1); then the flag is set (only that one
    other human gets the event). The winner then gets +0x120 {winner, the flag just set}; a stats reporter
    (`0x009C7660`, vfunc +0x50) gets the winner's key and campaign numbers. Every loser gets +0x138. The done byte is
    set when a winner exists, or when a loser exists and no human is undecided. Ours: there is no victory check in
    the model yet (BACKLOG §6 victory conditions), nor a multiplayer campaign; this writer goes with them.
  - **(3), the revolution** (CONFIRMED by decompilation): faction +0x814 = in a revolution. Its only setter
    `0x008EE3E0` (copies {settlement → +0x818, side byte → +0x81C, two more bytes → +0x81D/+0x81E}, clears +0x93E) is
    called by the region unrest outcomes `0x008B46A0` (case 5; case 1 is the riot) when it strikes a faction's capital (`0x00ABE4E0` / `0x00A8B5A0`:
    the region is its owner's +0x72C): for an AI faction with side byte 1 (then the rebels spawn,
    `0x00AAB440(.., 1, ..)`); for a human it only sets +0x93E and fires event +0x8A0 (the choice), and the human's
    answer, the command `CCQ_BEGIN_REVOLUTION_IN_REGION` (registered `0x0041F480`, handler `0x00931BE0` →
    `0x00A67210`), calls the setter with the chosen side. A third caller, `0x0096BA50`, reads its arguments as
    strings ("i" side 1, "r" side 0 for an AI faction; "u"/"l" for a human), a debug or script command. Side byte
    clear = the revolutionaries' side (as `PlayerPlayingAsRevolutionaries` `0x009EED40` reads it). The two ends:
    `0x008BEAA0`, called by `TransferSettlementOwnership` when the old owner is in a revolution and loses its +0x818
    settlement: when `0x006A3110` is non-zero empties the settlement's +0x220 list (`0x00A1B4E0` per entry, not read further), fires +0x8D0 (the revolution movie),
    ends the state, rebuilds the effect sum, fires +0x2A0 and +0xAE0; side byte set and human → the flag and +0x108
    (the loyalist player lost the capital); side byte clear → `0x009D1E40` (settlement +0xFC; not read) when the +0xAE0 event leaves another recorded faction. `0x008DB1A0`, the
    listener at faction +0x58, when an event's object (vfunc +0x38, its +0x98 → +0x68) is the faction's +0x818
    holder and a count (`0x006A3110`) is below 2: sets +0x88C (faction +0x8E4), fires +0x8B8, ends the state, fires
    +0xAE0; side byte clear and human → the flag and +0x108 (the revolutionary player lost; the event's exact
    meaning INFERRED: the revolution crushed). Both are guarded by `0x008CA0B0` (+0x814 set). This replaces the
    older reading of +0x814 as a handover / co-op shared view. Ours: revolutions are not in the model (BACKLOG §6
    "revolutions", and the +0x814 / +0x81C line); these writers go with them, through `end_campaign`.
- **Readers:** the campaign update `0x008F4120` runs its queued-command loop (`0x008F4158`..`0x008F4197`), then at
  `0x008F419E` returns false while the flag is set, skipping time, the CAI step loop (`0x008F4524`..`0x008F4863`, the
  AI turns; it has no test of the flag, see "The CAI steps after the flag" below), the positions update, pending
  peace and the victory check; the victory check `0x0096CD10` returns at once; `0x008BE4A0` (can save) returns false; `0x00970230`
  (camera / UI update) skips the multiplayer turn timer.
- **The CAI steps after the flag** (worker campover2, 2026-10-20; CONFIRMED by decompilation and control flow):
  - The step loop (`0x008F4524`): a budget of 200 iterations (cut to 1 or 5 by `0x00951400` / +0xFCB when
    `DAT_015c0d27`, the current faction's +0x6E0 or `0x00951500` say so); it loops while EPISODIC_RESTRICTIONS +0xD0
    is clear. Each iteration makes at most one step: the director step `0x00BA9220` (campaign +0xF80) when
    `0x008EFB40` (location manager idle and campaign +0x1036), else "CAI do_cai_step" `0x00A761F0` (campaign +0xF7C)
    when `0x008EFAB0` (the current faction is not +0x824, the location manager is idle, faction +0x93D clear and the
    turn state allows); then the turn-phase block (state 0 → `0x0094A940`; state 2 → `0x008A9870` →
    `AdvanceFactionTurn`), `update_positions` `0x00973B90` (campaign +0xF60) and pending peace (+0x1020). The loop
    ends at `0x008F485D` when `0x009559D0` (the location manager's moving list empty) or the budget is spent. A step
    needs an idle location manager and an idle one ends the loop, so a frame has at most one CAI or director step,
    in its first iteration.
  - The pool: `0x00A761F0` runs the faction's BDI pool (`0x00CCCE90` → `0x00D01910`) unless the pool waits (+0x8D,
    read by `0x00CB1DA0`): a waiting pool only counts its waits (+0x718) and is released (`0x00CC3B60`, via
    `0x00CC3CF0`) after more than 3. The intention pass `0x00D024D0` acts one intention per pass and returns 0 (ending
    the pool run) when +0x8C (the faction's end of turn, `0x00CCD1B0`) or +0x8D is set. +0x8D is set only by
    `0x00CB1D90`, called by the intentions right after they give a move order (16 functions; e.g. `0x00CCD470`,
    `0x00CCD6A0`, `0x00CD1140`: order, the order object's vfunc +0x1C, `0x00CB1D90`). So a pool run ends at its first
    move, and the faction's next run comes only once that move has played out (idle location manager) and four more
    steps have passed.
  - So a flag set by an AI move (the arrival's capture in `update_positions`, or the battle that follows it) leaves no
    AI order after it: the frame has no second step and every later frame returns at `0x008F419E`. The same frame
    still runs its turn-phase block, `update_positions` and peace, then movement-only iterations and the end calls
    (the victory check returns at once). `AdvanceFactionTurn` cannot run there for the moving faction: turn state 2
    comes only from that faction's end of turn (`0x0096E200` from `0x008BD6D0`, reached by the CAI through
    `0x00CCD1B0` → `0x00A787D0`), which a run that ended on a move has not reached. No CAI-run path that destroys a
    human synchronously was found (the destroy callers: `TransferRegionToFaction`, the mission rewards `0x00A06050`,
    `grant_faction_handover` `0x00978950`; regions change hands through captures on arrival, deals the human accepts,
    liberation and the director).
- **What is shown next** (worker victoryscreen, 2026-10-19; CONFIRMED by disassembly and decompilation unless tagged):
  - **Listeners.** The campaign loader (`0x00872550` at `0x00872CE5`; also `0x008742C0` at `0x00874A2F`) builds one
    movie-listener object (`0x0099B720`, its movie manager at +0xA8) whose sub-listeners subscribe to: event +0x108
    (faction destroyed, `0x005301C0` at `0x0099B8AA`) → defeat movie `0x00A14E80`; event +0x120 (faction won, inline
    list add `0x0099B8C4`..`0x0099B932`) → victory movie `0x00A14E00`; +0x8D0 → revolution movie `0x00A14EE0`; +0x798
    → duel movie `0x00A14CF0`; +0x8B8 (handler not read). The UI manager's listener `0x00A134E0` (+0x790) takes event
    +0x138 (faction lost by the victory conditions).
  - **Defeat (human destroyed, event +0x108 with flag 1; the flag is 1 only for a human):** `0x00A14E80` makes the
    panel data {ExitFunction "ExitGame" (fill `0x00A07790`), show_everyone 1, only_call_function_for_local_faction 1}
    and queues the movie of the `movie_event_strings` row whose id column equals the faction's movie id (`factions`
    column #39, raw +0x114, built record +0xD8 by `0x00F74CB0`; France "24"): `0x009FBA20` → `0x00F55030` scans every
    row and the **last** match wins (CMOVNZ) → `0x00A20700` queues it on campaign +0x288 (vfunc +8, event object
    `0x0099A4F0`). With vanilla data the row is `game_lost_<faction>` (e.g. `EUR_Lose_FR.bik`). OK on the panel runs
    `CampaignUI.QuitToMainScreen`. The id compare (`0x004F0F40` → `0x004EFB20`) is CompareUniStrings, case-sensitive;
    no matching row queues nothing. `movie_event_strings` is `ssss` (event, culture, movie, id; header 01, 57 vanilla
    rows, every row has an id); the minor factions' movie id is "" or "0", which no vanilla row has: no movie.
  - **Victory (event +0x120):** `0x00A14E00` picks movie event 0x1D `game_victory_absolute_monachy`, 0x1E
    `..._constitutional_monachy` when the faction's government (+0x70C → +0xC8) is 1, 0x1F `..._republic` when it is 2
    (event names: table `0x0145CAE8`); `0x009FBA60` → `0x00F54F80` walks the rows with that event in table order: a
    row of the faction's culture (`0x008F3490` +0x10, compared as records) is returned at once; a row whose culture's
    `cultures` #2 (culture record +0x14, a 16-byte string compared with `0x004F0E80`) equals the faction's culture key becomes
    the pick and sets a flag, the last such row winning; otherwise the first row is the pick, replaced by a `european`
    row (`0x004F0EA0` against `0x0132EE78`) while the flag is clear and the pick is not european. CONFIRMED by
    disassembly; the culture record layout (key +0, #1 +0x10, #2 +0x14, stride 0x24; both strings 16 bytes
    with their data pointer at +0x0C / +0x20) by its builder `ConstructCultureRecordFromBuilder` `0x00F73880`
    (via `0x00F86A80`, table build `0x00F79C60`; builder row: key +0, #1 +0x0C, #2 +0x10, 12-byte strings). Vanilla #2 (all 9 `cultures` rows, CONFIRMED from the shipped table by
    the real_install test `outcome_movies_from_the_real_data`): egy_european, PIR_european, PIR_natives,
    PIR_pirates, tribal → european; egy_middle_east, indian → middle_east; middle_east → indian; european
    empty. Data: ExitFunction "ExitGame" when the event's multiplayer
    flag is set, else "ShowVictoryOptionsPopup" (fill `0x00A07760`), which opens `campaign_result` as a victory.
  - **Lost by the victory conditions (event +0x138):** `0x00A134E0` calls the root's `ShowCampaignResultPopup(false)`
    when the faction is the local player: the defeat popup (Continue / Exit), no movie.
  - **Messages:** 37 `campaign_won` is posted by `0x009D0870` (`0x009D3250(faction, 0x25, ..)`), whose only caller is the
    victory conditions `0x0096FCE0` at `0x0096FFF7`. 34 `campaign_lost` and 35 `campaign_lost_your_faction_destroyed`
    have no poster: every caller of `0x009D3250` (~88) and of `0x009D39A0` was read, none passes 0x22 / 0x23
    (INFERRED unused: names in the table only). The exe never calls `ShowCampaignLostMessage` (scripts only);
    `InitialiseMoviePanel` is called by the UI's movie queue `0x00998E20`.
  - **Who opens the movie panel** (worker victoryscreen2, 2026-10-20; CONFIRMED by disassembly): `0x00998E20` is the
    constructor (vtable `0x0136DA80`) of the 0x2C movie-panel object, thiscall (lua root, movie event, bink, volume int,
    local-involved byte). It calls the root's `InitialiseMoviePanel` with `bink_control_ptr`, `title` (only when the
    event's +0x18 is non-null) and `Data` = {`ExitFunction`, `show_everyone` (event data byte +4),
    `only_call_function_for_local_faction` (+5), `local_faction_involved` (the argument)}. Its only caller is the call
    at `0x00A13FD0` in `0x00A13EA0` (was not a Ghidra function; reached through the thunk `0x00A13E90`, `this` -=
    0x3A0, in vtable slot `0x0136B5A0`; `this` = the CampaignUiManager +0x3BC sub-object): it returns unless the
    local faction (`[[ui +0x9FC] +0xB0]`) is human (+0x6E0); involved = the event's faction is the local one
    (`0x00898B20`); not involved and `show_everyone` clear → nothing; else it frees the previous panel (+0xCC0,
    `0x009D5A60` / `0x0099DD60`), builds the new one (its volume argument is int(setting 4 × 0.01 × setting 5), CVTTSS2SI at ~`0x00A13FC0`) and keeps it at +0xCC0. So the panel opens only
    on a human machine, for that player's own movie or one shown to everyone, and `local_faction_involved` is
    exactly "the event's faction is the local player's".
  - **The title:** the movie event (`0x0099A4F0`, queued by `0x00A20700`) keeps at +0x18 the movie-event row's +0x14.
    The row builder `0x00F57D60` (record constructor `0x00F43E00`) formats `"movie_event_strings_title_%S"`
    (`0x013D43E4`) with the row's key (event + culture) and stores that loc string at +0x14: the title is the loc
    text `movie_event_strings_title_<event><culture>` (shipped, e.g. `..._game_lost_franceeuropean` "Faction
    Destroyed", `..._game_victory_absolute_monachyeuropean` "Supreme Victory"). The panel script shows `data.title`.
  - **`set_ui_notification_of_victory_disabled`** (script command, handler `0x0097B980`) sets EPISODIC_RESTRICTIONS
    (campaign +0xFA8) +0x125; the victory check `0x0096CD10` then posts no +0x120 for a winner and skips its
    multiplayer campaign-over step (test `0x0096CE9C`), and the victory test `0x0096FCE0` still marks the faction won
    but skips the `campaign_won` post (test `0x0096FFE7`). Saved at `EPISODIC_RESTRICTIONS` #32; set in the tut start
    position. Ours: `World::victory_notification_disabled` (CAMPAIGN_DATA.md §4.2); the victory check must read it.
  - **CampaignUI bindings** (registration thunks hold name and handler): `QuitToMainScreen` (`0x00429CF0`) →
    `0x009EF630` → `0x009BFED0` sets byte +0x7C of the UI manager's +0x9FC object to 1 (the request to leave). Its
    neighbour `QuitToWindowsFromEscapeMenu` (`0x00429D10` → `0x009EF640` → `0x009BFEE0`) sets +0x7D. Both are read
    by `0x00A065A0` (worker victoryscreen2), the campaign mode's next-mode step (called by `0x00A166F0`, vtable
    `0x0136A540` slot +0x14, which copies the result {pointer, code} to its out +0xC / +0x10): +0x7D → code 6;
    +0x7C → if the mode's +0x2C is set and EPISODIC_RESTRICTIONS (`[[obj +8] +0xFA8]`) +0xD0 is set, it calls +0x2C
    vfunc +4 with `"campaign|<mode +0x30>"` (an episodic campaign chain, UNKNOWN), else code 2; code 1 builds the
    0x178 battle object (`0x00506B50`), 3 the campaign loader (`0x00872380`), 0 stays (CONFIRMED by its other
    callers, worker victoryfix: `0x00506B50` is called by `0x005D73A0`, which formats `"battle|%C|%S"`, and by the
    tutorial-battle starter `0x00488C60` (`Battle_Tutorial.xml` / `NavalTutorial.xml`); `0x00872380` by the mode
    factory `0x00485B90` (`"campaign"`, `"campaign_load"`), and its vtable `0x01357168` sits before the string
    "Creating campaign env"). So code 1 comes from a pending battle (mode +0xD0, a 0x144 object freed after) or the
    next-mode build, code 3 from mode +0x3C (a campaign to load). Ghidra's plate of `0x00A065A0` and its
    `CampaignGameModeState` fields now say so (`pPendingBattleSetup` +0xD0, `bCampaignLoadPending` +0x3C,
    `aCampaignLoadArgs` +0x40, `dwCampaignLoadArg` +0x38; corrected 2026-10-20). The consumer of the code is
    not traced (the SGM_* string enum `0x013A484C`, parser `0x00DEE560`, is a different one), so code 2 = the main
    menu stays INFERRED. `MovieDismissed` (`0x004295F0`) → `0x009EE380`
    → `0x00A0FC30` frees the playing movie (UI manager +0x107C, 0x2C bytes, `0x0099DD60`). `ContinueGameSelected`
    (`0x00428910`) → `0x009E1D20` sends the command `CCQ_CONTINUE_GAME_SELECTED` (registered `0x00420380`), whose
    handler `0x00932B40` clears byte +0x9C of the object returned by vfunc +0x1C of campaign +0xF70's first entry
    (UNKNOWN meaning). Its only setter through that path (worker victoryscreen2): the victory check `0x0096CD10`
    sets it to 1 at `0x0096CE7C` whenever a human winner exists, in single player too (early-outs only on its done
    byte +0x38 and campaign +0xFE9). Campaign +0xF70 is the 8-byte object from `0x00882AC0` (stored at `0x008732D1`
    in `0x00872550` and at `0x00874E8C` in `0x008742C0`); the reader of +0x9C is not found, so what continuing
    changes stays UNKNOWN.
- **The scripts' side** (worker victoryscreen, 2026-10-17; CONFIRMED by `luac_dump` of the shipped bytecode; the exe
  side is "What is shown next" above):
  - `ui/campaign ui/event_movie_panel_scripts/event_movie_panel.luac`: the panel (`panelmanager.luac` registers
    `event_movie_panel` with layout `data/ui/campaign ui/event_movie_panel`) is opened by the root's
    `InitialiseMoviePanel(data)` (`layout.root.lua:1033`, `OpenPanel("event_movie_panel", .., "ShowMovieType", data)`;
    `message_handler.luac:96` names the panel too, caller not read yet). `ShowMovieType(data)` (line 7) calls
    `AssignExitFunction(data.Data)` when `data.Data.ExitFunction` is not nil, sets `title_frame`'s text to `data.title`
    (or ""), attaches `data.bink_control_ptr` to `movie_window` and, in single player, `CampaignUI.PauseCampaign(true)`.
    `AssignExitFunction(d)` (line 30): returns at once when `d.only_call_function_for_local_faction == true` and
    `d.local_faction_involved ~= true`; else the OK callback is `CampaignUI.QuitToMainScreen` for ExitFunction
    "ExitGame", the global `FinalisePendingDuel` for "FinalisePendingDuel", the global `ShowVictoryOptionsPopup` for
    "ShowVictoryOptionsPopup". `DismissMovieEvent` (line 66) calls that callback if set, then `CloseThisPopup`
    (root `LuaCall("ClosePopup", Address)`); `RemoveMoviePanel` (line 76) calls `CampaignUI.MovieDismissed()` and, in
    single player, `PauseCampaign(false)`. `ShowVictoryOptionsPopup` (line 49) is root `LuaCall("ShowCampaignResultPopup", true)`.
  - `layout.root.luac`: `ShowCampaignResultPopup(won)` (line 1015) takes `CampaignUI.LocalisationString("defeat")`, or
    `"victory"` when `won == true`, and opens the `campaign_result` panel with `InitialiseCampaignResultPanel(title)`;
    `RemoveCampaignResultPopup` (line 1025) closes it. `ShowCampaignLostMessage` (line 1044) opens `dialogue_box` with
    `LocalisationString("quit_warning")` and the callback `CampaignUI.QuitToMainScreen`.
  - `ui/campaign ui/campaign_result_scripts/campaign_result.luac` (layout `ui/campaign ui/campaign_result`):
    `InitialiseCampaignResultPanel(title)` sets `title_frame`'s text; `ExitGame` = `CampaignUI.QuitToMainScreen()` then
    `CloseThisWindow`; `ContinueGame` = `CampaignUI.ContinueGameSelected()`, `CloseThisWindow`, then
    `message_handler.CheckForAutoOpen(true)`; `CloseThisWindow` = `Component.Call("Root.LuaCall", "RemoveCampaignResultPopup")`.
  - Next (not traced): which buttons of the `campaign_result` layout call `ExitGame` / `ContinueGame`. Extract the
    scripts with `pack_probe extract-prefix ui/ <scratch dir>` and dump them with
    `luac_dump "ui\campaign ui\<file>.luac" --all` (pack paths use backslashes).
- **Ours:** `TurnState::campaign_over` (ntw_sim turn.rs, hashed, own save field with `serde(default)`, no format bump:
  an older save loads it as false), set only by `CampaignModel::end_campaign`, which also drops the queued turn work.
  `destroy_faction` calls it for a human faction. Then `step` runs nothing, `end_turn` / `begin_end_turn` do nothing,
  and `may_act` is unchanged (the queued-command loop still runs): a human destroyed in their own turn may still give
  commands, one destroyed in an AI's turn leaves nobody who may act (the AI's `AiTurn` step went with the queue). The
  campaign AI gives no more orders once the flag is set (`ntw_ai::campaign::apply_orders_with_events` stops quietly,
  no rejection; CONFIRMED by "The CAI steps after the flag": our AI gives a turn's orders as one list with moves
  resolved at once, and the orders after the one that ends the campaign are the ones the exe's faction would give in
  later pool runs, which never come; worker campover2 replaced the PROVISIONAL). The ESF
  loader reads #18 for start positions and saves, the ESF writer writes it. `stops_for` no longer has the PROVISIONAL
  all-AI fallback for listed humans who are all out of the game: that state (an own save from before the flag, or a
  hand-built world) plays out the round and ends the campaign at the next round start (tested once per round), with
  one warning (our own guard; the exe cannot reach it). Not modelled
  (BACKLOG): writers (2) and (3) (they go with the victory check and multiplayer, and with revolutions), the save refusal (the UI's save check), the victory event (+0x120: the victory
  check does not post it yet, so only the defeat movie is reached in play). Tests:
  slots_exit_tests `a_destroyed_human_ends_the_campaign_and_the_turn_loop_stops`,
  `a_human_destroyed_in_an_ai_turn_drops_the_rest_of_the_round`,
  `every_human_out_of_the_game_without_the_flag_ends_the_campaign`; own_save
  `campaign_over_round_trips_and_an_older_save_loads_it_as_false`; economy_fidelity
  `campaign_over_loads_and_round_trips_through_the_esf_save`; ntw_ai campaign
  `an_ai_order_that_destroys_the_human_drops_its_remaining_orders_quietly`.
- **Ours, the defeat / victory movie** (worker victoryscreen, 2026-10-19): `CampaignModel::outcome_movie` (ntw_sim
  outcome.rs, behind the seam `campaign.outcome_movie`) picks the row as above from `CampaignRules::movie_events`
  (`movie_event_strings`, merged database), `faction_movie_ids` (`factions` #39, now named `movie_id`) and
  `culture_alternates` (`cultures` #2, new typed table `ntw_data::characters::CultureRecord`), with the
  exit and the panel flags of `0x00A14E80` / `0x00A14E00`. `destroy_faction` emits `CampaignEvent::OutcomeMovie` for a
  destroyed human (after `FactionDestroyed`). The game (`CampaignSim::command` → `hud::update`) hands it to
  `UiScriptHost::campaign_outcome_movie`, which calls the root's `InitialiseMoviePanel` with the data table (our
  `bink_control_ptr` is the movie's file name); the original `event_movie_panel` script attaches it with the new
  component method `AttachCustomControl`, and `hud::panel_movie` plays it into a texture drawn in `movie_window`.
  `CampaignUI.QuitToMainScreen` → `CampaignRequest::QuitToMainScreen` → the main menu (INFERRED, see the bindings
  above); `MovieDismissed` drops the movie; `ContinueGameSelected` is a logged PROVISIONAL no-op. The panel's `title`
  is the loc text of `OutcomeMovie::title_key` (`movie_event_strings_title_<event><culture>`, the row's new `culture`
  field; a missing string gives "" and logs once), and `local_faction_involved` = the faction is the local
  player's, a movie not shown to everyone opening only for that player (both CONFIRMED, `0x00A13EA0` and the title
  above; worker victoryscreen2, 2026-10-20). INFERRED: the movie plays once with its sound. Tests: outcome.rs `the_defeat_movie_is_the_last_row_with_the_factions_id`,
  `the_victory_movie_follows_the_government_and_the_culture`,
  `the_victory_movie_takes_a_culture_standing_in_for_the_factions`, `a_destroyed_human_plays_its_defeat_movie`; ntw_campaign
  `the_outcome_movie_rows_and_faction_ids_reach_the_rules`, real_install `outcome_movies_from_the_real_data`;
  ntw_script movie_panel_tests (6, `the_movie_panel_title_is_the_rows_loc_text` added) and campaign_ui `the_defeat_movie_panel_shows_the_movie_and_its_ok_quits`,
  `the_victory_movie_panel_opens_the_campaign_result_as_a_victory` (the shipped scripts, real install).
  Next: the victory check's +0x120 post (BACKLOG, the victory conditions), the consumer of `0x00A065A0`'s code 2
  and the episodic +0xD0 chain, the reader of +0x9C (`CCQ_CONTINUE_GAME_SELECTED`'s effect), play-once, the
  `campaign_result` buttons, and the movie volume: `0x00A13EA0` passes the panel (+0x24) `setting 4 × 0.01 ×
  setting 5` from the options object's vfunc +0xDC; `0x00A23330` hands +0x24 to the bink open `0x00D9F600`, which
  scales it by 0x102 (0 when the control's +0xC vfunc +0xD4(5) is false) for the player's vfunc +0xC. Not traced: which
  two settings, and how the player applies the value; ours plays it as any movie (movie table × `MOVIE_VOLUME` × master),
  PROVISIONAL in `panel_movie`.
- **Ghidra (V5):** `0x008C30A0` renamed `SetCampaignOverFlag` (prototype, plate; completeness 89.4, effective 95);
  plate comments without renames on `0x0096CD10`, `0x008F4120`, `0x008BEAA0`, `0x008DB1A0` (campover2 rewrote the last three and added plates on `0x008EE3E0` and `0x00CB1D90`, no renames).
- **Ghidra (V5), the movies** (worker victoryscreen, 2026-10-19; prototype + plate each, completeness / effective):
  `0x0099B720` ConstructCampaignMovieEventListeners 83.6 / 100 (`this` typed: new struct `CampaignMovieEventListeners`, 0xAC, six `CampaignMovieListenerSub` of 0x1C), `0x009D0870` PostCampaignWonMessage 73.6 / 95,
  `0x009FBA20` QueueMovieEventByFactionMovieId 93.3 / 100, `0x009FBA60` QueueMovieEventByIdAndCulture 88.6 / 98.1,
  `0x00A134E0` ShowDefeatPopupForLocalFaction 84.7 / 95 (worker vsnotes: locals typed, plate names `this` as the
  sub-object at CampaignUiManager +0x790), `0x00A14E00` QueueVictoryMovieForFaction 81.9 / 95,
  `0x00A14E80` QueueDefeatMovieForHumanFaction 89.4 / 95, `0x00F54F80` FindMovieEventRowByCulture 92.2 / 100
  (worker vsnotes: `pCulture` typed with the new struct `CultureRecord`, 0x24: key +0, #1 +0x10, alternate key +0x14, both keys `AllocWideString` (16 bytes)),
  `0x00F55030` FindLastMovieEventRowById 84.0 / 98.1. The rest is structural (the other `this` stay `void *`,
  register-only locals). Globals: vtables `g_pVtblDefeatMovieEventData` (0x0136F3B4), `g_pVtblVictoryMovieEventData`
  (0x0136F3C8), `g_pVtblCampaignMovieListeners{Main,Sub1C,Sub38,Sub54,Sub70,Sub8C}` (0x0136DADC..), vbtables
  `g_adVbtblCampaignMovieListeners0..5` (0x0136DB24 + 8n), strings `g_szUI_ShowCampaignResultPopup` (0x0136E3C4) and `g_szCultureKeyEuropean` (0x0132EE78); labels
  in `0x00F54F80` / `0x0099B720`; `0x00F73880` `CultureRecord::ConstructCultureRecordFromBuilder` 96.9 / 96.9 (this typed; new struct `CultureRecordBuilder`, 0x1C) and `0x008F3490` `CampaignFaction::GetFactionSubcultureRecord` 93.1 / 96.9 (new struct `SubcultureRecord`, 0x1C; the faction record stays `void *` until a `FACTION_RECORD` struct exists). The listener
  subscriptions: main on +0x798, +0x70 on +0x108 (destroyed), +0x54 on +0x8D0, +0x38 on +0x8B8, +0x8C on +0x120
  (won); `0x00A134E0` listens on +0x138, subscribed by `0x0098C2F0` at `0x0098EB59`.
- **Ghidra (V5), the movie panel** (worker victoryscreen2, 2026-10-20; prototype, plate, EOL / PRE comments,
  locals typed and named; completeness / effective): `0x00A13EA0` ShowQueuedMovieEventPanel (function created,
  339 bytes) 78.9 / 91.9; `0x00998E20` ConstructMoviePanelAndInitialise 70.3 / 75.9 (args confirmed from the
  stack offsets: pLuaRoot, pMovieEvent, pBinkControl, nVolume, fLocalFactionInvolved; nVolume is an int, CVTTSS2SI
  of setting 4 × 0.01 (0x01318034) × setting 5 at ~0x00A13FC0; its +0x28 holds MovieEventData +8, a dword, not a
  pointer; vtable `g_apCampaignMoviePanelVtbl` 0x0136DA80); `0x00F57D60` BuildMovieEventStringRecord (cdecl:
  pResult, pRowKeys UniString[2] event / culture, ppCultureTable, pLocDatabase, pFailedTables) 80.9 / 80.9;
  `0x00A065A0` GetNextCampaignGameModeTransition 73.6 / 90.0 (this typed as the new struct `CampaignGameModeState`, 0xD4, the fields this function reads). Structs `QueuedMovieEvent` (0x44) and
  `GameModeTransition` (8); the Lua key strings, the format strings and the two pending / new mode arg-list
  statics (`g_pbPendingModeArgList`, `g_abNewModeArgList` with their counts, guards and atexit labels) renamed
  with plates. Left at the score floor: the `this` pointers (ECX, not retypeable by the API) and register-only
  temporaries.

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
max(0, 1 + policing_cost_mod/100))` (0x00AA9C10; `population::policing_cost`; var 18 = `policing_automated_cost_per_pop`, CONFIRMED
from the variable table; an AI owner pays 0).

Other sources read in round 3, all CONFIRMED formulas but not modelled because the model lacks their data:
- **Education** [9] (0x008B1A70): only in the capital, or where effect 39 ≠ 0. The value is the class's base
  (`+0x70`) + `round((1 + education_happy_mod/100) × class-keyed category-1 effect)`.
- **Religion** [3]/[13] (0x008B1B10). Per religion share s of the class, `s × 100 × religion factor`, minus the faction's
  religion effect (0x00E1F030). Then `× (1 + happiness_mod_religious_unrest/100)` (0 if negative), rounded and
  negated. Needs the regions' religion shares (`POPULATION` / `TRAITS`, not loaded).
- **Gentlemen** [12] (0x008C55D0). Each gentleman resident in the region's settlement (region +0xFC → +0xC8; in `spa_napoleon` also the three missionary types)
  gives `ceil(x / gentleman_happiness_[103]) + gentleman_happiness_bonus` (effect 129). x = 0x00A198D0, the
  character's rank (see round 4 below for the trace). The total is positive for the side's own and allied gentlemen (0x008CE390) and negative otherwise. The positive
  part is clamped to [0, var 104]; the negative part is floored at var 105.
- **War results** [11]: 0x00AA10F0 is true when a character of another faction stands in the area query of the
  region (0x00BA6840) and passes 0x008CE9B0 (hostile).

The other sources (ministers, character traits and ancillaries, events) come from the effects system (slot 0-F).

**Round 4: the four sources are modelled** (code `economy::public_order`, `religion_factor`, `gentlemen_factor`;
test `religion_gentlemen_war_and_education_in_public_order`).
- Data loaded per region:
  - `POPULATION/REGION_FACTORS/RELIGION_BREAKDOWN`: (religion, share) → `Region::religions`;
  - per population class (`POPULATION CLASSES`, `POPULATION_CLASS` v3): #11 the education base (class +0x70) and #12
    the war-results (resistance) base (class +0x74) → `Region::class_bases` (since classfactor 2026-10-20 also #1
    element 7, the war factor +0x20: §Population, "The class war factor").
- The class record also holds the original's computed factors: #1 i32[13] happiness factors [1..13], #2 i32[6]
  repression [16..21], #3 positive sum, #4 negative sum, #5 repression total. These are CONFIRMED by 0x008EDF70's
  layout. Round 6 compares the model with them on the vanilla saves (§Public order, round 6).
- **Education** [9] (CONFIRMED): only in the owner's capital or a region with research points. The value is
  `base + round((1 + education_happy_mod/100) × clamour)`. Clamour is the class-keyed `happiness_clamour_for_reform`
  effect (category 1); the model routes the buildings' `happy_clamour_for_reform_*` effects here instead of summing
  them everywhere.
- **Religion** [3] (CONFIRMED on the vanilla eur save, round 6): `−round(max(0, 100 + happiness_mod_religious_unrest)
  / 100 × (Σ share × 100 × value − conversion))`, with `value` = `diplomatic_relations_religion` #3 for (region religion,
  state religion). A missing pair counts 0. The `conversion` effect (religion-keyed, bonus type 10, id 1; EFFECTS_FIDELITY.md §1) is the owner's faction-level value for its state religion, without the region's own buildings (the vanilla spa saves: churches with `conversion_anti_french` leave the stored factor unchanged); `economy.rs` `public_order_factors_with` reads it.
- **Gentlemen** [12] (CONFIRMED, disassembly of 0x008C55D0, dealattitude fix 2026-10-20): each gentleman resident in
  the region's settlement gives `ceil(rank / gentleman_happiness_divisor) + gentleman_happiness_bonus`.
  - Which characters (CONFIRMED, disassembly and the vanilla saves, fix round 2): the walk reads the settlement
    (region +0xFC) list +0xC8, filled by the settlement loader from `SIEGEABLE_GARRISON_RESIDENCE` #14 (`LEA ECX,
    [ESI+0xC8]` at 0x00AF4DFF, then 0x008CFA70 per id), and tests each with 0x00F9C6D0 (type +0x2C 4 or 5). A
    gentleman standing in the region, or studying in a school (the school's own residence #14), does not count.
    The vanilla capitals' gentlemen all sit in their schools (every settlement #14 is empty) and every save stores
    0 in slot 11. The model stands in for the resident list with the gentlemen `garrisoned_in` the region.
  - Residence in play (CONFIRMED, disassembly; residence 2026-10-20): the settlement residence (vtable 0x0137C9A4)
    keeps its forceless characters in +0xC8. Load: 0x00AF4B00 appends each #14 id through 0x008CFA70 (the
    character's residence handle +0x2C0; `CHARACTER` #7 is the residence id). Enter (slot +0x88, 0x00B4E760): a
    character without a force is appended to +0xC8 and `CharacterEntersGarrison` fires (event vtable 0x013561B0),
    then his zone / hidden flag are rebuilt (0x00A285C0); an army sets +0xC0 and fires the event for its commander
    and unit characters, and its accompanying characters (army +0x60) move in with it (loop at 0x00B4E874). Leave
    (slot +0x8C, 0x00B4EE70): he is taken off +0xC8 (0x008B0210), no script event. Can-enter (slot +0x6C,
    0x00B165B0): a character with a force asks the force; else, for the owner's agent, a type with `agents` #4
    false (vanilla: minister, bandit; 0x00A0B9C0) always enters, and with a garrison army the room is that army's
    accompanying characters below 20 (0x008B2620); otherwise (a foreign agent, or no garrison) the residents below
    20 (`CMP 0x14` at 0x00B16632). No stance check. The order: the move-to-residence order 0x009549B0 (13 callers,
    UI and AI 0x00A845B0) → 0x00918AB0 → command 0x00904EF0 (vtable 0x0135F8F4, execute 0x0095B7F0) → 0x0091BE30,
    which builds the walk and an enter step (0x009071F0, vtable 0x013613DC); on arrival the step (0x0094CFF0)
    checks can-enter again (false: he stops outside), and with its enter flag (+0x58) set enters him and sends a
    world event (+0x3D8); with it clear it opens diplomacy (PushPendingDiplomacyAction). Any later move leaves.
    Model: the importer makes each #14 agent resident (`garrisoned_in`, a listed id that is not a loaded forceless
    character is logged and dropped); `CampaignCommand::AgentEnterSettlement` walks and enters (`may_reside`, the
    room is the moddable `max_residence_characters`, default 20), `leave_garrison` (every walk) leaves; our save
    writes #14 (the source's ids still resident, then new ones) and `CHARACTER` #7; own saves keep `garrisoned_in`.
    One rule says who is a resident (`World::residence_of`: `garrisoned_in` and in no force, neither commander
    nor unit rider); the room check, the gentlemen factor, the save writer and the importer all read it. A
    command change (General hire 0x00A164C0, admiral hire 0x00A16110, field promotions 0x008E1C20 / 0x008E2260)
    reaches no residence list (0x008CFA70 / 0x00B4EE70; call graphs to depth 3-4): the new commander holds the
    army's place inside and the old one rides on with his unit (`pool.rs` `pass_garrison_on`, CONFIRMED).
    The map's right click on a settlement orders it for a forceless character. Tests: `residence_tests.rs`,
    `residence_install.rs` (the vanilla GB April 1811 save: five Spanish missionaries), own-save round trip.
  - PROVISIONAL (residence): the model keeps no accompanying characters (`MILITARY_FORCE` #2, army +0x60), so the
    garrison branch counts 0 and an army entering brings none; which callers of 0x009549B0 set the enter flag
    (+0x140 → step +0x58) and the arrival step's world event (+0x3D8) and foreign-owner branch (+0x59 / +0x5A,
    event +0x978) are not traced. Not modelled (BACKLOG §5): residents when a settlement changes hands, agents
    spawning into a residence (0x00A241E0), the returning-agent countdown (character +0x530, 0x00A24EB0 →
    0x008B3790), the re-enter / leave after an agent action (0x00915150), school-slot residents.
  - `economy_check` (`ECON_MAP=1 ECON_POSUM=1`, the 8 vanilla saves):
    slot 11 went from 134/144 on each eur save (model +1 / +2 from the capitals' gentlemen, rank 2) to 144/144;
    exact classes 854 → 904 of 906 (the 2 left are `auto_nr1`'s garrison slot 17, older). `ECON_RESIDENTS=1`
    prints each settlement's #14 residents and the gentlemen standing in its region.
  - PROVISIONAL: in the `spa_napoleon` campaign (character +0x194 → +0x30) the walk also counts characters
    passing 0x00F9C6F0 and the three missionary types (0x008C562F..0x008C568D); not modelled.
  - The rank is `GetCharacterRank` 0x00A198D0 (called at 0x008C5698; `agents::rank`, CONFIRMED for gentlemen: his
    `research` plus trait / ancillary bonuses, clamped to [−1, 9]); the ceiling is `(rank − 1 + divisor) / divisor`
    with a truncating `IDIV` (0x008C56A9).
  - The bonus is effect 0x81 read by 0x00E1F130 from the gentleman's own set (ECX = character +0x48C at 0x008C569E;
    `Effects::character_effects`: his traits and ancillaries), not his faction's sum.
  - The sign is positive for gentlemen of the owner or a faction allied to it and negative for others'
    (CONFIRMED, disassembly: `0x008C55D0(region, side)` tests `0x008CE390(gentleman's faction, side)` at
    `0x008C56CC`: neither rebels, the same faction or an allied / patron / protectorate stance;
    `economy::gentlemen_factor_for`, worker dealattitude 2026-10-20). The deal attitude `0x00A79050`
    calls it with the AI as the side (AI_RESEARCH.md §4 "Offered-region attitude").
  - The positive part is clamped to [0, positive limit]; the negative part is floored at the negative limit.
  - Round 6 had read the rank as 0 and the bonus from the faction's sum, untraced; replaced by the trace above.
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
  is missing from the stored factors, while the Austrian-held town's gentlemen's club counts. On an owner change
  (CONFIRMED, `TransferRegionSlotOwners` `0x00B1B300`) every slot passes to the new owner through
  `SetSettlementOwner` `0x00B2B810(new, 0)`, a third faction's hold included, except a slot with an occupant (slot
  virtual +0x1C, the slot garrison; not modelled) or a port with a navy docked (+0x1E4), which keep their holder;
  the walls (+0x24) and road (+0x20) always pass. Model: `change_region_owner` (test
  `an_owner_change_hands_every_slot_over_but_a_port_with_a_navy_docked`); the docked check inherits
  `docked_port`'s PROVISIONAL position test (the occupant, `GetSlotGarrison` `0x00A64440`, and the exe's docked test are BACKLOG items).
- **Governing faction** (INFERRED, one region): the region's faction part (government, ministers, technologies, tax
  levels) comes from the faction whose governorship lists the region (`GOVERNORSHIP` #2), normally the owner. The
  same `eur_bavaria` is still in the dead Bavaria's governorship and shows Bavaria's government and none of Austria's
  minister effects (`happiness_character` −2 elsewhere in Austria). `World::governing_faction`; on capture the
  region moves to the new owner's governorship.
- **Gentlemen** (CONFIRMED, 2026-10-20): the term is the gentleman's rank `0x00A198D0` (call at `0x008C5698`) and
  his own set's `gentleman_happiness_bonus` (character +0x48C at `0x008C569E`), counted only for the settlement's
  residents (region +0xFC → +0xC8), see round 4 above; the earlier reading (rank 0, the faction's bonus) is withdrawn.
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
- **Breakdown and predicted mode (round 17, 0e-panel; code `economy::region_wealth`).** `0x00A6AFC0` also fills a
  14-pair GDP breakdown (`region_wealth_factors` slots at `0x015C5B50`: base_gdp, farm / industry / mine / port gdp
  (`0x00A6C4E0`: effects 4 / 5 / 6 / 8), the eight trade commodities, town_wealth), a 14-pair "lost" commodity breakdown
  and a 10-slot growth breakdown (`town_wealth_growth_factors` slots at `0x015C67F8`, set up by `0x0042ED30`:
  education, government, industry, port, roads, tax, bankruptcy, technologies, ministers, discontent): industry starts
  with effect 130 (the faction-wide growth), the buildings' education / government / industry / port / roads effects
  52 / 53 / 55 / 57 / 58 go to their slots (`0x00A6C570`; home_region 54 to none), technologies takes trunc(effect 61),
  ministers trunc(effect 54) at home, tax trunc(trunc(growth × m) + fixed) (m only when growth ≥ 0), bankruptcy
  −growth × #17, discontent #18. With its last argument 1 it predicts: the set is `0x00A9B930` (constructions and
  research done, §Population) and each slot counts the level under construction in its place, else its building unless
  it is being demolished (item type 3) or below full health without a repair (type 2). Readers: region +0x2A8 / +0x318
  / +0x388 (the current breakdowns, `0x00AB4410`), the panel's RegionWealth (`0x00A992A0`) and TownWealth (`0x00A995A0`)
  against a fresh predicted run, and the wealth trend: `0x00AB4410` sets region +0xC4 from a second, predicted
  recompute's growth (0 above +20, 1 above 0, 2 at 0, 3 down to −20, 4 below; `economy::wealth_trend`). `0x00AB4410(0)`
  (current breakdowns and the trend, no town wealth step) also runs at the round start (`0x00AAE710`) and from eight
  other callers (constructions, taxes, ...); the model recomputes GDP and growth only at the round end (BACKLOG §0-E).
  The panel's RegionWealth lists no factors: a slot needs a `region_wealth_factors` record and the shipped data has
  no such table (`table_list wealth`).

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
  modelled. Whether the waypoint object's vt+0x98 reads the port blockade below (port +0x68 / +0x74) is untraced, so
  ours does not yet let a port blockade blockade a route (BACKLOG).
- **Port blockades** (portblockade 2026-10-20, CONFIRMED by decompile and disassembly; V5 in Ghidra). A port is the
  PORT_GARRISON_MANAGER object at region slot +0x1E4 (+0x70 = its slot, +0x60 = the docked navy's handle, +0x68 = the
  blockading navy's handle, linked both ways with the navy's +0xE8, +0x74 = a byte).
  - **Start**, `StartPortBlockade` `0x00A66F80` (`this` = the port, the navy): links +0x68 with the navy's +0xE8, sets
    byte +0x58 of the slot's naval recruitment manager (slot +0x1E8), posts message 0xBA when the navy's faction or
    the slot's holder is human, runs the price update `0x00BCB020(0)`, posts the slot's +0x480 event, sets slot
    byte +0x220 = 1 (never cleared by the lift), calls `0x00A17BE0` and, with a navy docked, `0x00B653A0`, then fires
    CharacterBlockadedPort (vtable `0x01376C40`) and the holder's +0x2A0 event. Its only caller is the move-order
    step `0x0094B9B0` (`0x0094BA2A`), when the target port has a docked navy or a garrison. Not modelled yet
    (BACKLOG: ours has no naval move orders onto ports).
  - Byte +0x74 (save #2): set to 1 only at `0x0095B667` in the attack step `0x0095B370`, when the attacker's +0xE8
    port is the port the attacked navy is docked in; cleared only by the lift.
  - **Lift**, `LiftPortSlotBlockade` `0x00A784E0` (ECX = the port): unlinks +0x68, clears the +0x58 byte, always
    runs the price update `0x00BCB020(0)`, calls the untraced `0x00B084F0` on the docked navy's character if any,
    posts the slot's +0x480 event and the holder's +0x2A0 event, and sets +0x74 = 0. Its 7 callers:
    1. the peace, `ClearCommerceRaidsBetweenFactions` `0x00A25570` (the call at `0x00A25627`, from
       `MakePeaceWithFaction` `0x00B262C0`): per character of either side, after the siege end and before the
       raids, a navy whose +0xE8 port's slot holder (vt+0x14) is the other side;
    2. the owner change's slot pass `TransferRegionSlotOwners` `0x00B1B300` (`0x00B1B3EF`): after the holder pass
       (which runs only with no garrison and no docked navy, +0x60), when the port is blockaded, the slot's holder
       is now the new owner, and the blockading navy is the new owner's or not at war with it (`0x008CE9B0`);
    3. the navy destructor `0x0088EF90`: restores the docked port's holder, then lifts the navy's +0xE8 port (both
       skipped while campaign +0xFC5 is set);
    4. the move wrapper `0x008AC010` when the move result is 2 (the navy sails off);
    5. the post-battle state machine `0x008F7880` (`0x008F8981`, state 1);
    6. the attack-step helper `0x008FF470`;
    7. the `CCQ_CHARACTER_END_BLOCKADE` command handler at `0x00932710` (no function defined there; command
       name string `0x0135E534`).
  - Saves: PORT_GARRISON_MANAGER v2 = [u32 #0 docked navy, u32 #1 blockading navy, bool #2 = +0x74] (reader
    `0x00A45860`, #2 only from version 2; writer `0x00A51D90`). All 64 vanilla and evidence saves and every start
    position hold #1 = 0 and #2 false.
  - Model (`campaign::port_blockade`): `RegionSlot::blockade` (navy, +0x74) is the one state, loaded from and
    written to #1 / #2, and in our own save (a defaulted field, so no format bump). `lift_port_blockade` is the one
    lift and runs the price update; the peace (`make_peace_rules`), the slot pass (`pass_region_slots`; its lifts'
    price updates fold into the owner change's one, as the port passes' do) and the navy's removal (`remove_force`) call it.
    Callers 4 to 7, the start, the +0x58 byte's reader and `0x00B084F0` wait for naval moves, battles and the
    command (BACKLOG).
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
    - at the round end #5 ← #6 ← price (the flag argument 1; the round start calls it with 0 and keeps #5 / #6,
      see "Turn order and round start").

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
  of `trade_routes_mod_max_sea` (effect 0x43) of the slot's building (GetSlotBuildingSeaRouteCapBonus 0x00AAF770: any building, integer; trading ports
  3 / 4 / 5). CONFIRMED walk.
  - **The capital has its own cap** (broadcasters3, CONFIRMED): the walk starts with +0x72C already visited, so
    0x008DC150 leaves the capital out (code `sea_route_cap`). The capital's port has a separate cap, 0x008DC360
    (code `capital_port_route_cap`): the capital region's cap (0x00AAF770) when it has an export port (HasRegionExportPort 0x00A6CA70),
    else 0. Routes out of the capital's port count against it, the others with a sea hop against the sea cap
    (0x00BA42E0's counts; `TradePath::capital_port`, saved since own-save format 5, derived for older saves). The
    old reading (capital included in one sea cap) fitted the saves only because the two caps add up: with the split,
    no faction of the 8 vanilla saves is over either cap (`economy_check` `caps_detail`).
  - **Compiled bonuses, not DB keys** (CONFIRMED): 0x00AAF770 reads compiled basic bonus `trade_route_cap_sea`
    (effect 0x43; DB key `trade_routes_mod_max_sea` in vanilla), and the export port test 0x00A6CA70 → 0x00A797B0
    reads `commodity_export_vol` (effect 2; DB key `export_capacity_port`), both through
    `effect_bonus_value_basic_junction`. Code `CampaignModel::building_local_bonus`: a mod mapping another key to
    these bonuses works.
  - **Neighbours** (round 6) come from `regions.esf`: every outline edge run has a `connectivity` triple {u32 region
    index << 16 | area index, first vertex, last vertex}. Self references, 0xFFFF and non-game regions (seas, the
    river region `all`, `eur_lakes`, the map edges) drop out. `World::region_neighbours`, loaded by
    `ntw_campaign::trade::attach_map`. Test `region_neighbours_come_from_the_map_outlines`: France has 9 land
    neighbours, islands have none, and the lists are symmetric. Without the map every owned region counts
    (PROVISIONAL fallback).
  - A new route may use the sea only below the cap. No land cap was found: France holds 5 land routes with no cap
    effect. Other callers of 0x008DC150 (0x00BCB020 prices, 0x00BA0490, 0x00BBA4F0, 0x00BC7910) are not read;
    0x00BA42E0 is (below).
- **Route check** (0x00BA42E0, broadcasters3, CONFIRMED; code `trade_check::trade_route_check`, seam
  `trade.route_check`): 0 when either side has no capital; 1 for a land path; 3 for a capital-port path while the
  capital-port count is under 0x008DC360; 2 for a sea path while the sea count is under 0x008DC150; with both full, the
  swaps 4 / 6 / 5 / 7 (an open sea or capital-port route whose importer is also reached by land frees its slot).
  - Path search FindTradePathBetweenCapitals 0x00BB1260 with neighbours GetTradePathNeighbourNodes 0x00BB0240 (a node
    is a region plus a sea-hop flag; it is started on network g_dwTradePathTypeSeaLeg 0x015F1670, capital port, start
    node flagged as a sea hop; g_dwTradePathTypeLandLeg 0x015F1674, sea; or 0x015F1AF8, land only): land neighbours
    owned by `b`, or by `a` except `a`'s capital on 0x015F1670 (0x015F1674 and 0x015F1AF8 let land paths re-enter it).
    Sea hops (none on 0x015F1AF8) leave only a region with an export port: to every other port of `a` that is not full
    (IsTradePortFull) and, on 0x015F1674, is not `a`'s capital; and to every port of `b` except its capital whose cap
    less `b`'s own routes leaving that port is not 0, or `b` has no exporter entry. All hops are within
    `trade_route_internat_sea_length_limit` (1e8 in vanilla). `b`'s capital is entered only over land.
  - Port full IsTradePortFull 0x00BB4F80: the owner (region +0xF4) is looked up in the exporter map (+0x11C, find 0x00BB0980); the
    end iterator → not full (0x00BB5039), even with a cap of 0. Else the region cap as a u32, less one per owner route whose
    first sea hop leaves it, is 0; an entry with an empty route vector is present, so a 0-cap port is then full; an
    over-full port wraps and is not full. Entries are made by operator[] 0x00B9E480 (loader 0x00B89270, 0x00BA0490,
    0x00BC0120, 0x00BC02B0, 0x00BC26D0) and never erased (none seen). Which owners have one (portfull, CONFIRMED): the
    supply split 0x00BC26D0 ends with operator[] (at 0x00BC3699, unconditional) for every faction of the registry list
    (+0x20: count +0x2C, data +0x30; every faction but the rebels, who sit apart at +0x1C); the split runs first in
    every rebuild 0x00BBD870, so in every price update 0x00BCB020 (at 0x00BCC52F); the campaign constructor 0x00872550
    (new campaign and loaded save, its one caller 0x00987070) builds the trade manager (0x00B89270 at 0x0087336B) and
    then reaches the price update 0x00BCB020 at 0x00873D57 on every path. So every registry faction has an entry from
    the first turn to the end of the session. The rebels get one only through operator[] as an exporter (load, build,
    route removal), which needs a route or agreement they never hold. Ours: an entry is "a world faction that is not
    the rebels, or an owner exporting a stored route" (`trade_port_full`); no persisted set needed. No ORIGINAL BUG: a
    0-cap port having no room is what its data says. Vanilla's export ports all have caps of 3-5 (pTrade1-3,
    pSupplySpain1-3, pTradeTutorial1), so only a mod sees the 0-cap case. Tests `port_full_entry_for_every_faction_but_the_rebels`,
    `port_full_outlives_the_last_route`.
  - The counts are compared with the caps unsigned (JC/JNC at 0x00BA4439, 0x00BA4485, 0x00BA461F, 0x00BA4718; the
    equality tests at 0x00BA462D, 0x00BA4727): a negative cap (a mod's negative bonus) is never reached. Counts stay
    0 when the exporter has no entry (0x00898B40 at 0x00BA43D1). Test `route_check_negative_cap_is_never_reached`.
  - The capital-port (g_dwTradePathTypeSeaLeg 0x015F1670) and sea (g_dwTradePathTypeLandLeg 0x015F1674) networks
    differ in the start node's sea flag and in `a`'s capital: on 0x015F1670 it is not re-entered over land but is a
    sea target; on 0x015F1674 it is re-entered over land but is never a sea target (0x00BB0240). Each rule bars only
    the capital node that is not the start node (0x00B85160 / 0x00B849C0 match nodes by region and flag), so both reach
    the same regions; the per-hop cost (0x00BC9FB0, its type picked from the hop at 0x00B84AE7–0x00B84B16, not from the
    network) is the same on both. In the model they are one search and codes 5 / 7 never come back (INFERRED to agree
    in practice; the original's cached pair entries can differ).
- **Route loss** (0x00BBE1A0 ← owner change 0x00A64E87 when the old owner is not the rebels, broadcasters3,
  CONFIRMED; code `trade_routes_after_region_lost`, called from `change_region_owner`): every route with a waypoint
  in the region costs its pair one route each way (0x00BC02B0 remove-first; the partner is the old owner when the
  route ends there, else the importer). Then for each pair: route check 0 either way, both in the game and still
  trading → `BreakTradeBothWays` 0x00B73430 (the `break_trade` broadcast); else a new route each way (0x00BA0490; the
  model builds routes when trade is valued, PROVISIONAL). Tests in `trade_check.rs`.
  - **ORIGINAL BUG** (0x00BBE1A0): a route ending in the region whose importer is not the old owner, with no route
    to remove in either direction, is found again forever (the game hangs). Ours removes that route itself.
  - The third argument is the OLD owner: the caller reads region +0xF4 (0x006649C0 at 0x00A64D06,
    reloaded at 0x00A64E28) before the transfer and pushes it at 0x00A64E7C; inside 0x00BBE1A0 the
    region's 0x006649C0 is the new owner. So step 1 (no capital → listener +0x38) and the partner
    for a route ending in the region use the old owner; the pair cache and the final refresh
    (0x00BAC0D0, 0x00BBDBA0) cover both. Ghidra's parameter and plate were corrected to
    pOldOwnerFaction (fix round).
  - Scan order: the exe walks its exporter hash map; the model goes by faction id (INFERRED; only the event order
    differs).
  - Price update on a region loss (regionprice, CONFIRMED by disassembly; ours: `trade_routes_after_region_lost`
    and `change_region_owner`, tests `a_region_lost_mid_turn_updates_the_prices_keeping_the_history`,
    `a_port_slot_passing_updates_the_prices`). 0x00BBE1A0 has no early return: after the pair loop and the cache
    refreshes (0x00BAC0D0 for the region and the old owner, 0x00BBDBA0(old owner)) it calls 0x00BCB020 with flag 0
    (`PUSH 0x0` at 0x00BBE7C0, call 0x00BBE7C4) on every path. The second caller is SetSettlementOwner 0x00B2B810
    (this, pFaction, bSilent): after storing the holder (+0x88) it calls vtable +0x4C; when that returns non-null,
    the object's +0x20C (bCreatesPortObjects) is set and IsPortSlotType 0x00A8B810 (pSettlementInfo +0x1DC → slot-type
    record +0x44, byte +0x3D) holds, it runs 0x00BCB020 with flag 0 (`PUSH 0x0` 0x00B2B844, call 0x00B2B84F); then
    0x00AB49F0(0), UpdateRegionTownWealth(0) and, unless bSilent, the events. Vtable +0x4C is 0x004470B0 (returns
    this) in the region slot vtable g_apfnRegionSlotVtbl 0x0137695C and 0x00461180 (returns 0) in the settlement
    vtable 0x0137C9A4 (set by 0x00AF48D0 / 0x00AF4590): the settlement's own SetSettlementOwner calls in
    TransferSettlementOwnership 0x00B449F0 (0x00B44ACC, 0x00B44B2C) never update prices. The slot constructor
    0x00A4D4F0 builds the slot's recruitment manager (0x00AF3CC0 → 0x00AF4140, record "REGION_RECRUITMENT_MANAGER")
    and its port object under exactly the same two tests, so the slots that update are the model's
    `RegionSlot::port`. Order in 0x00B449F0 (when fDoTransfer, i.e. unless a passed force carries REBEL_DATA (+0x98)
    whose faction (+0x34) already owns the region, see "Ghidra pass (polish9)" below; a transfer to the same owner passes too, but 0x00A64AC0 then returns at 0x00A64D13 after its research stop, see "Same-owner transfer"): SetSettlementOwner(settlement, new, 1) →
    TransferRegionToFaction 0x00A64AC0 (route loss and its update when the old owner is not the rebels, then expel,
    policing tracker, taxes) → listeners → TransferRegionSlotOwners 0x00B1B300 (SetSettlementOwner(new, 0) for each
    slot with no garrison and no docked navy: one update per such port slot) → RebuildRegionRecruitablesAndEffects.
    Ours runs the update once after the taxes refresh when any port slot passed: the same prices (flag 0 moves no
    history and nothing it reads changes between the exe's calls). ORIGINAL BUG: none.
  - The other SetSettlementOwner callers (priceupdate, 2026-10-20, CONFIRMED by disassembly). SetSettlementOwner has no
    same-holder test, so every call on a port slot updates. Which residences can update: the vtable pair +0x44/+0x48
    and +0x4C/+0x50 returns (0, this) for a region slot (0x0137695C), (this, 0) for a fort (0x0137D3E4) and (0, 0)
    for a settlement (0x0137C9A4), so only region slots ever reach 0x00BCB020. The three residence kinds share their
    virtual layout: +0x90 leave residence, +0xA0 / +0xA4 / +0xB8 capture (settlement 0x00B58560 / 0x00B58890 /
    0x00B58C00, slot 0x00A92380 / 0x00A925C0 / 0x00A971B0, fort 0x00B584A0 / 0x00B587D0 / 0x00B58B90).
    - 0x00A8D859, RestorePortSlotHolderToRegionOwner 0x00A8D830 (this = the port object, +0x70 its slot; skipped when the
      slot's vfunc +0x1C, its garrison, is set): the slot goes back to the region owner, always a port, so always an
      update. Callers: the undock 0x00A8C0B0 (0x00A8C0E2) and the navy destructor 0x0088EF90 (0x0088F004, on the navy's
      port handle +0xE0, unless campaign +0xFC5). Ours: `restore_port_holder` (from `undock` and `remove_force`) now
      runs `update_commodity_prices_with(false)` for a port slot.
    - 0x00B1B01B, 0x00B1B010 = a fort's set-owner (then its +0x1D4 refresh 0x00B4D140 and the faction's 0x00B3F630).
      Callers are all fort paths: the fort constructor (0x00AEB4A0 → 0x00B4E6B0), the fort's leave residence 0x00B0D570,
      the fort capture virtuals, 0x00AB3DF0 (region +0x234 fort list, from TransferSettlementOwnership, handing forts
      back when 0x00898B80(holder, owner)), 0x00B794B0's fort branch, and the destruction loop at 0x008ABD3E (every
      listed faction's regions, campaign +0xF5C → +0x778 list → region +0x240 forts held by the dead faction → the
      region owner). A fort's +0x4C is 0: never a price update. Our forts hold no holder: nothing to do.
    - 0x00B79590, in 0x00B794B0, called only from DestroyFactionAndResetRelations (0x008ABB2D / 0x008ABB3D), for each
      character of a dead AI faction after 0x009DAB40 deleted its force: on the character's residence (0x009FB9A0,
      kept for a non-settlement only when 0x009D3A40 is nonzero) and then on its port's slot (0x00A17CF0 → +0x70). For a
      slot it recomputes the holder (each later read overrides the earlier, so the garrison's faction via vfunc +0x24 → +0x2A0 wins,
      then the docked navy's faction via port +0x1E4 → +0x60 → +0x7C, then the region owner) and calls SetSettlementOwner(f, 0) when that differs from
      +0x88: a port slot updates. A fort goes to 0x00B1B010; a settlement gets nothing. Ours: the slot reset in
      `destroy_faction` stands for this and the army destructor path below; it now runs the update once per port slot
      it hands back (INFERRED which slots go back, as slot garrisons are not modelled).
    - 0x00A645AB, in 0x00A64560: a slot whose holder is not the region owner and which has no port or no docked navy goes
      back to the owner (an update for a port). Callers: the slot's leave residence 0x00A64450 (vtable +0x90, then
      0x00B0D610 unlinks the garrison +0xC0) and the army destructor 0x0088CE30 (0x0088CEE0, on army +0x9C when its
      +0x4C is set, unless campaign +0xFC5). Event: an army garrisoned in a town or port slot leaves it or dies. Not
      modelled (slot garrisons, BACKLOG).
    - 0x00A92418 / 0x00A92658 / 0x00A97204, the slot capture virtuals 0x00A92380 (+0xA0, after an assault, also posts
      event 0xFD) / 0x00A925C0 (+0xA4, also an explicit 0x00BCB020(0) after) / 0x00A971B0 (+0xB8): a character takes a
      town or port slot; for a different faction the queues are cancelled (0x00A729E0, CancelPortNavalRecruitmentItems,
      ForwardSlotConstructionCancel), then RebuildRegionRecruitablesAndEffects, SetSettlementOwner(character faction
      +0x7C, 0) (an update for a port) and 0x00B44920. Not modelled: our slot holders come only from saves (BACKLOG).
    - 0x00A92564, in 0x00A924D0, called only from 0x0094B9B0 (a move-order step, vtable 0x01361504 slot 4, built by
      0x008FFEF0 ← 0x00917E20 ← 0x0095D6A0; its +0x48 is always a port slot; the order's class name is UNKNOWN): a port
      with no docked navy and no garrison gets the character placed (0x00A99A60 / 0x00963840) and 0x00A924D0 takes the
      slot for the force's faction (an update, then an explicit 0x00BCB020(0) again); otherwise 0x00A66F80 and result 8.
      Event: a navy docking in an empty port. Not modelled: ours docks by position (`docked_port`, PROVISIONAL) with no
      dock step (BACKLOG).
  - Not modelled (BACKLOG): the pair-path cache 0x00BBDBA0 / 0x00BAC0D0; the listener handlers 0x00BB9410 and 0x00BB9B90 (vtables
    0x0137FA44..0x0137FA74, set by ConstructCampaignTradeManagerFromEsf 0x00B89270 / 0x00B8E190 / 0x00B976B0; events not identified). 0x00BB9410 takes a
    faction F: it removes one route each way with every importer partner (RemoveFirstTradeRouteToImporter 0x00BC0120), and when F has a capital
    re-checks each pair with 0x00BA42E0, breaking (0x00B73430) or rebuilding (0x00BA0490) as above.
- **Agreement breaks remove one route each way** (tradebreak, CONFIRMED; code `break_trade`, `break_trade_both_ways`,
  both through `remove_first_trade_route`; tests `breaking_trade_*` in `trade_check.rs`):
  - `break_trade` = BreakTradeAgreementWithFaction 0x00B29BB0 (callers: 0x00B27070, EmbargoFactionTradeForRequester
    0x00B28DB0, 0x00C18DE5): a→b CancelTradeAgreementOnRelation 0x00B13C30, b→a EndTradeAgreementForBrokenPartner
    0x00B733B0 (as 0x00B13C30 but the record's trade_broken comes from the `trade_broken` event), each calling
    RemoveFirstTradeRouteToImporter 0x00BC0120 once; then the price update 0x00BCB020(0), 0x00A26E90, events +0x840 and +0x3A8.
    `break_trade_both_ways` = BreakTradeBothWays 0x00B73430: 0x00B13C30 on both records.
  - "First" route: 0x00BC0120 walks the exporter's vector (map +0x11C; count +8, data +0xC, 0x84 bytes) from index 0
    and erases the first route whose importer (0x00B27680(1)) matches, with the order-keeping erase 0x00BAC2A0; routes
    are appended by the loader 0x00B89270 (file order) and the builder 0x00BA0490. Ours: the pair's `Vec` in the same
    order, `remove(0)`. The erase then runs 0x00BD1020, the route-change event (+0x8C → +8 → +0x498, slot +8) and
    0x00BBDBA0(exporter) (pair cache, not kept in ours).
  - Routes left behind stay in the exporter's vector until a later break (oldest first), the route loss 0x00BBE1A0,
    or the listeners 0x00BB9410 / 0x00BB9B90. They keep earning in the exe: the split 0x00BC26D0 (via 0x00BBD870)
    shares supply only to partners with the agreement (+0x788), but 0x00BBD870 recomputes every route with 0x00B15B50
    (accumulated + commodity, 0 here, + resource + GDP part 0x00B3F8F0; 0 when blockaded), and the income 0x00BB3490
    sums every route of the exporter (0x00B79B20); the accumulation 0x00B05CC0 (from 0x00BCB020 at 0x00BCC5DA) walks
    every route too. Ours now does the same (leftroutes, below).
- **Routes left after a break keep earning** (leftroutes, CONFIRMED; code `economy::trade_route_importers`,
  `trade_routes_value`, `accumulate_trade_with`, `CampaignModel::trade_path_volumes`, `trade_home_value`; tests
  `a_route_left_after_a_broken_agreement_keeps_earning` (ntw_sim), `..._still_earns_after_a_load` (own save)):
  - GetFactionInternationalTradeIncome 0x00BB3490 (economics category 2, caller 0x00BBD7C0): finds the exporter's
    entry in the trade manager map +0x11C (0x00BB0980) and sums GetTradeRouteNetValue 0x00B79B20 over every route in
    it (count +0x14, data +0x18, 0x84 bytes); no agreement, war or importer check. 0x00B79B20: 0 when blockaded
    (0x00B12050), else route +0x28 (value) minus the second dword of each 8-byte entry of the vector +0x60/+0x64.
  - Home term, only for campaign `spa_napoleon` and faction `spa_france` (strings compared in 0x00BB3490): plus each
    domestic route ending in the capital (map +0x104, 0x68 bytes, filter 0x00A8B5A0): volume (0x00B1D1C0) × price
    (trade manager +0xBC, count +0xB8); minus Σ current volume (0x008AFAA0) × price over every international route of
    the exporter, blockaded or not (an SSE4 pmulld loop when g_nIsaAvailable > 1, same result). The whole term needs
    the faction's domestic map entry (polish41, CONFIRMED): when the find 0x00BB0A10 on map +0x100 returns the end
    (or a null entry), 0x00BB35C4 / 0x00BB35CC jump to the plain route sum, so nothing is subtracted either; an entry
    with no routes still subtracts. Ours: `trade_home_value` returns 0 when `World::domestic_trade` has no key for the
    faction (loaded from `DOMESTIC_TRADE_ROUTES`, one key per stored item); test `the_home_term_needs_a_domestic_map_entry`.
  - RecomputeAllInternationalTradeRoutes 0x00BBD870: the split 0x00BC26D0 (first ClearTradeRouteVolumes 0x00B1C1A0
    on every route, then shares only to partners with +0x788), 0x00B67300 on every route, then 0x00B15B50 on every
    route. So a leftover route has volume 0 and commodity part 0 but keeps its GDP part and its accumulated value;
    AccumulateTradeRouteValue 0x00B05CC0 adds trunc((effect 0x32 × 0.01 + 1) × var 4 × (+0x2C + +0x30 + +0x34)) to
    its +0x3C each round. Nothing in the split, the price update 0x00BCB020 or the turn pass prunes routes without
    an agreement.
  - Ours: `trade_route_importers(a)` = every importer with a stored route from `a`, plus the `trade_partners` (built
    route or test fallback), is the one list income, accumulation, the home term and the TradeInfo UI walk. A
    non-partner's route volumes are 0 also without a known supply (`trade_path_volumes` takes the exporter). The home
    term subtracts this turn's volumes over those routes (was: the loaded volumes of the stored routes).
  - **Accumulated value per route** (routeacc, CONFIRMED; code `TradePath::accumulated`, `accumulate_trade_with`,
    `trade_pair_value_with`, `remove_first_trade_route`, `ntw_campaign::trade::read_paths` / `write_accumulated`,
    `own_save::migrate_trade_accumulated`; tests `each_trade_route_keeps_its_own_accumulated_value` (ntw_sim),
    `trade_routes_keep_their_own_accumulated_value_and_a_version_6_save_moves_its_pair_value` (own save),
    `trade_accumulation_and_bankruptcy_are_written` (save_compat)):
    - Exe: each route holds its own value at +0x3C; 0x00B05CC0 adds to that route only, and the recompute 0x00B15B50
      copies it into +0x38, which goes into the route total +0x28. The erase 0x00BAC2A0 destroys the element
      (0x00B85030 → 0x00B981D0, which frees the route's +0x78, +0x64, +0x50, +0x24 and +0x10 vectors) and moves the
      later elements down one slot, so the erased route's value is gone and every other route keeps its own.
    - Ours: a stored route (`World::trade_paths`) keeps its value in `TradePath::accumulated`; `World::trade_accumulated`
      now holds only the value of a pair's built route (a pair with no stored route: `build_trade_route` makes one
      route per pair, and the test fallback stands for that one route). `remove_first_trade_route` drops the first
      route with its value, and drops the pair's built-route value once the pair has no stored route left. The .esf
      importer reads each record's +0x3C into its route; the writer matches each record to its model route (same
      pair, same waypoints) and writes that value. Own save format 7 stores the per-route value; an older save's
      pair value moves onto the pair's first stored route (or stays the built-route value when there is none).
    - Evidence saves (`trade_check`): every pair has one route, so per pair and per route agree on every file read
      so far; routes with a value above 0: 61 / 76 (`auto_after_c8`), 49 / 60 (`auto_nr1`), 57 / 72 (`auto_nr4_t4`),
      2 / 6 (`orig_fr_may1811`, `orig_fr_t1_b`), 69 / 84 (`orig_over_nr4_0252`), 0 in the others.
  - **Commerce-raid deductions** (the 0x00B79B20 vector, traced; modelled in `commerce_raid.rs` (traderaid), see
    "Commerce raids" below):
    - Route +0x58 base, +0x5C capacity, +0x60 count, +0x64 data: 8-byte entries {raiding faction, i32 amount}.
      GetTradeRouteNetValue 0x00B79B20 subtracts the amounts from +0x28.
    - Refilled at the end of every recompute: RecomputeInternationalTradeRouteValue 0x00B15B50 (blockaded or not) →
      RefreshTradeRouteRaidDeductions 0x00B61780 → 0x00BA7730 (finds the exporter and the route +4 path) →
      BuildTradeRouteRaidDeductions 0x00B83C30(route + 0x58); 0x00B61780 then stores the returned faction → share
      hash map at route +0x68 (0x00AF8170; buckets +0x74 count / +0x78 data, 0x14 bytes each).
      0x00B83C30 first clears the old entries (0x00AC8810, which books −amount back), then walks the route's
      waypoint segments (0x18-byte points, nodes at +0xC / +0x10) through the hash at trade manager +0x1AC count /
      +0x1B0 buckets, keyed by (lo ^ hi·lo ^ 0x4A545EED) over the (lower, higher) node pair, collects the entities
      standing on each segment (0x00BA7E90) and, per owner faction (entity +0x2A0 via 0x006649C0), sums a float share
      from the object at entity +0x2A8, virtual +0x54 (argument: the largest (float)(item +0x40) over the list
      0x00E31890 returns, at least 1.0: 0x00E31890 is units_table, +0x40 its `travel_speed`, CONFIRMED: the read is vtable slot +0x64, GetUnitRecordTravelSpeed 0x008B9F10 (record at unit +0x48), shared by the land and naval unit vtables 0x01355494 / 0x0135552C; see "Share" below). When the shares sum above 1 they are divided
      by the sum. Each faction's amount is ComputeTradeRouteRaidAmount 0x00B60A40(route, share): 0 when blockaded,
      else Σ_c trunc((float)(price_c × volume_c) × share) + trunc((float)(+0x30 resource + +0x34 GDP) × share) +
      trunc((float)+0x38 × share), each term truncated separately by 0x0126E2B0. Entries are RAII: each held entry
      books +amount into that faction's +0xAC economics record as group 1 (0x00BBE9A0), which is income category 9
      (`finances_commerce_raiding_income`; an earlier note read it as category 1) and its destructor 0x00AC8810 books
      −amount, so moves during vector growth (capacity 1, then doubling) net to zero. The domestic routes have the
      same pass (0x00BA76F0 → 0x00B83360; 0x00B84760 also calls 0x00B60A40).
    - Route value parts (0x00B15B50): +0x38 = +0x3C; commodities +0x2C = Σ volume × price (manager +0xB0);
      resources +0x30 = Σ over resources the exporter has (faction +0x720 byte[i], 0x008E1C10) and the importer
      (+0x54) lacks of manager +0xF0 value[i] / supplierTable[importerIndex × resourceCount + i] (importer index in
      the world faction list, 0x00B735F0), per-resource values kept at +0x48 / +0x4C / +0x50; GDP +0x34 =
      0x00B3F8F0; total +0x28 = +0x38 + +0x30 + +0x2C + +0x34. Blockaded: +0x38 = +0x34 = +0x28 = 0.
    - Route storage: EraseInternationalTradeRouteAt 0x00BAC2A0 destroys the element, moves each later one down
      (copy 0x00B96450, destroy 0x00B85030) and returns the position; DestroyInternationalTradeRoute 0x00B981D0
      frees the share buckets, the deductions (−amount each), then +0x50, +0x24, +0x10.
    - Ghidra types added: TradeRaidDeduction, TradeRaidDeductionVector, FactionShareHashMap, TradeRoutePathVector,
      TradeRoutePathPoint, TradeSegmentRecord, TradeUIntVector; CampaignTradeManager +0x1A1 / +0x1AC / +0x1B0.
    - Strings: `finances_commerce_raiding_income`, `value_lost_to_raiding`, `SeaTradeRouteRaided`, `COMMERCE_RAIDS`,
      `trade_route_tooltip_raid_value`.
    - Save: after the seven value u32s a route record holds a U32Array (+0x4C), the raid count and the entries
      (writer 0x00AFD490: i32 amount, then u32 saved faction id; reader 0x00AEC420 reads the list for record
      versions above 1). All 12 evidence saves have a raid count of 0 on every route, so no vanilla file holds a raid.
  - **Commerce raids** (traderaid, `crates/ntw_sim/src/campaign/commerce_raid.rs`; CONFIRMED unless tagged):
    - The order: `CMO_RAID_TRADE_ROUTE` (vtable slot 0x01361694 = 0x0094BA50) runs 0x009D3AC0 → 0x00BA7680 →
      0x00BA7600. Every trade segment nearer than 0.5 to the order's target (0x00BCA0F0 → 0x00BCA140 →
      0x00BCA060) that is raidable for his faction (0x00BA7DF0) gets one raid entry (0x00BA76A0 → 0x00B95440, then
      the price update 0x00BCB020(0)) on its target (0x00BAFB20). None of these ends his existing raids (an earlier
      note said 0x008BC580 ran here: wrong); the order's start does (the move below; INFERRED that the raid order
      starts through it, our order clears first). 0x009D3AC0 returns whether any segment is in range, raidable or
      not; with none the step cancels the order (0x00921CD0) and returns status 1. The character keeps his entries
      at +0x2D0, the segment in its `COMMERCE_RAIDS` list.
    - Candidates: 0x00BCA0F0 (this, position, out vector; returns any hit) asks the network's two segment grids
      (+0xD0, +0xD4) for the position's cell (0x00BB8910 / 0x00BA4B20), stops at the first grid with a hit, and
      tests only the segments that cell lists. Ours tests every segment: INFERRED equal when each cell lists every
      segment within 0.5 of it (the grid build is not traced).
    - The step's events (0x0094BA50, after a raid; not modelled, BACKLOG): a human raider gets message 0xBB
      (game +0x9C0, vt+8). With the first candidate segment, its +0xB8 flag (0x00BB5710) picks one of two notice
      classes (vtables 0x01362350 set, 0x0136240C clear), filled with the character and position and posted
      (0x00DF2F20 / 0x00E30700 / 0x00E00EE0). The victim faction (0x00950840: the highest route value at war) gets
      the position stored in its +0x8F8 block (flag set) or +0x8E8 block (clear), valid byte +0x904 / +0x8F4, and
      0x008A9250 when human; a human raider then notifies game +0xFA0 (0x00A0C750 set, 0x00A26FD0 clear).
    - Distance (0x00BA8E90): each cubic curve of the segment is split into 32 chords, chord i running from t = i/31
      to (i+1)/31 (the divisor is one less than the count, so the last chord overshoots to t = 32/31). f32 order: for
      step s, t = s × 1/31 (0x01381958) and t² = s² × 1/961 (0x01381954), each one product, and t³ = t² × t; the
      power-basis evaluation 0x007CD7E0 sums [t³, t², t, 1] times each row of the basis 0x01453790 (rows -1 3 -3 1 /
      3 -6 3 0 / -3 3 0 0 / 1 0 0 0) left to right, then each axis's weighted control points left to right. The chord
      distance takes the projection on the unit direction ((1/len × px) × (dx × 1/len) + the same for z), clamps it
      to 0..1 (0 for a zero-length chord) and returns the length to the projected point; the start value is FLT_MAX
      (0x01325DB4).
    - Raidable (0x00BA7DF0): one of the segment's routes belongs to another faction at war with the raider.
      Target (0x00BAFB20): of those owners, the one whose routes on the segment sum highest (domestic: Σ price ×
      volume; international: the gross total, 0 when blockaded); a tie keeps the lower faction id.
    - The segment's route lists (`TRADE_SEGMENTS[]` #18 domestic owners, #19 international (exporter, last region)):
      a route is listed on the segments of each waypoint's leg (from, to) and, for a sea waypoint, also of the legs
      from its start node to its region's settlement node and from its end node to the next waypoint region's
      settlement node; a leg's spline id s names segment s >> 1 (`TRADE_ROUTES` legs, loader 0x00B89270 at
      0x00B8CF2E). CONFIRMED by test (`commerce_raid_install`): every saved list of auto_nr4_t4 (280 segments),
      auto_after_c8 (287), orig_fr_t1, orig_fr_may1811 and auto_orig_spa_0245 matches, 0 differ. The code that fills
      the lists is not traced. The take pass itself walks only the path legs (from, to).
    - Ending: EndCharacterCommerceRaids (0x009DA140, the character in ECX) clears his raid list +0x2D0
      (0x008BC580), then runs the price update 0x00BCB020(0) on the game's +0xF90 and notifies the +0xF7C listener
      (event 0x268). The raiding test before it is the generic list-empty test IsListEmpty 0x008BC550 (through its
      thunk 0x008BC560). Its callers, all CONFIRMED by xref:
      - a move: the start slot (index 10, 0x0095D940) of the order class at vtable 0x0135F498 (constructors 0x0090B830
        save, 0x0090B980 runtime, built from 0x0091DCC0) paths the character to the order's target (+0xC8); when the
        path starts (0x00974700) a raiding character loses his raids, with no re-raid. That this class is the plain
        move order is INFERRED.
      - a change of commander (PromoteLandUnitCommanderInField 0x008E1C20, calls at 0x008E2170 / 0x008E2190; 0x008E2260, 0x008D5F10 twice, 0x008D47B0
        twice, 0x008B9030, 0x00A16110, 0x00A164C0): if the old commander raids, his raids end and the force's new
        commander (0x00898E80 on force +0x74) raids at the old one's position (0x009D3AC0 with 0x00A17D50's
        position). 0x009C99A0 and 0x00A1D5E0 end the raids of a character who is no longer the force's commander.
        The death path is the force observer 0x008DB3E0 (slot +4 of the force observer vtables 0x013553E4,
        0x01355644 and 0x01355730, installed by the force constructors 0x00881030 / 0x00881120, 0x00870E60 /
        0x00870FD0 and 0x00882180 / 0x008822F0): DestroyCampaignCharacter 0x0099D2D0 notifies the observers with the
        character before it ends his raids (EndCharacterCommerceRaids at 0x0099D62E when his list +0x2D0 is not empty; the member clear 0x0099D681 is the list teardown); when he commands the force, 0x008DB3E0 notes whether he
        raids (IsListEmpty +0x2D0), ends his raids, picks the new commander (0x008B8E50, which itself hands no raid
        on), places him at the old position and, if the old one raided, has him raid there (0x009D3AC0 at
        0x00A17D50's position). Ours: `CampaignModel::hand_over_raids`, called from `command_vacated` (death
        `character_dies` and the escape `character_falls`), CONFIRMED. The three other changes of commander our
        model has hand the raids on in the exe too (CONFIRMED, polish59 trace): each notes the force's commander
        (0x00898E80 on force +0x74) and, when his list +0x2D0 is not empty, remembers him as the raider; after the
        change, if the force's commander is someone else, the raider's raids end (0x009DA140) and the new commander
        raids at the raider's position (0x009D3AC0 with 0x00A17D50 on the old one). A General hired into an army
        (0x00A164C0: calls 0x00A165D0 / 0x00A165E9, position read at 0x00A165E1 with ECX = the raider); the field
        promotion (land 0x008E1C20: 0x008E2170 / 0x008E2190; naval 0x008E2260: 0x008E2650 / 0x008E2670; dead in
        vanilla, slot +0x40 constant false); an admiral hired into a fleet (0x00A16110: 0x00A163CC / 0x00A163DF, at
        the position noted on entry, 0x00A16150). In the admiral hire, the captain the new admiral replaces is
        destroyed either way (0x008B8E50 picks the new commander, then 0x00A0C6F0 with 1 destroys the old captain
        through his vtable +4, 0x00A16217); only the raids differ: when that captain was the raiding commander
        (flag set at 0x00A161CB, the noted raider cleared), his raids end with him and the new commander raids at
        the noted position all the same; with no captain aboard (0x00A162F3, the admiral boards the empty ship
        0x008EDE20) the plain rule applies. The field promotion also places the unit's own character where the
        commander stood (0x00963840 at 0x008E1E4F, position noted at 0x008E1CA4) before he takes command. Ours
        (raidhand): each notes `raiding_commander` on entry and calls `hand_over_raids_on_command_change` after
        the change (`hire_general`, `hire_admiral` before the replaced captain's `character_dies`, which leaves
        the same raids as the exe's destroy-first order, and `promote_unit`, which also moves the unit's
        character to the commander's position), CONFIRMED, tests in `commerce_raid_tests.rs`. The force split
        0x008B9030 (sole caller 0x008F04C0, run by the order executors; traced in raidhand) notes the old
        commander and his position, moves the units to a new force (0x008D31A0; a navy 0x00870E60 or an army
        0x00882180), then picks a commander for the old force and for the new one (0x008ED2C0, set through
        0x008B7EA0; its out-flag is set only when the first unit had no character and a new officer was made for
        it, otherwise that unit's character is moved to the position, 0x00963840). It hands the raids on only
        when the old force's new commander differs from the old one, that flag is set and the old one raids:
        EndCharacterCommerceRaids on him (0x008B922F) and 0x009D3AC0 for the new commander at the noted position
        (0x008B923B). Our model has no force split yet (BACKLOG); 0x008D5F10 and 0x008D47B0 are paths it does not
        have either.
      - the character vtable 0x0136A62C slot 8 (0x00A285E0): returns at once for argument 2 and ends the raids
        unless the argument is 1 (argument 1 is the "moving" flag clear, see below); the modes are UNKNOWN.
      - 0x008DB3E0 (above), DestroyCampaignCharacter, ClearCommerceRaidsBetweenFactions and the validation end 0x00BA4A60.
      Also his death (0x0099D62E); a peace (0x00B262C0 → ClearCommerceRaidsBetweenFactions 0x00A25570: every
      commander of either side with a raid on the other side loses all his raids); the trade update's rebuild at the round start and end (0x00BCB020 with either flag, call 0x00BCC5F8; 0x00BD1020
      → 0x00BA4A70 → 0x008B3690: a raider no longer raidable on one of his segments loses all his raids; skipped
      while game +0xFC6 is set, meaning UNKNOWN). Ours: a move (`move_character`) ends the raids; the price update
      at a raid end is not run at once (the prices are recomputed at the round start and end).
    - Share (army 0x008B6B80, navy 0x008B6C50, per unit 0x008B6C20): Σ over the force's units of (f32)moves × 0.05
      (0x01325C88) / max, max = the largest unit `travel_speed` over every unit record (0x00E31890), at least 1.
      Moves (0x008A8670 → 0x008A86D0): base `travel_speed`; bonus = effect 137 `mod_land_movement_campaign` (land)
      or 72 `mod_ship_movement_campaign` (ship) plus the unit category's `movement_mod`; with a bonus,
      round((bonus + 100) × base × 0.01) ties to even; then × the force action point factor, rounded ties to even.
      PROVISIONAL: a ship whose naval record has byte +0x1D4 set also adds effect 73 once effect 70 is not 0; the
      record column behind the byte is not found, so no ship has it.
    - Domestic amount (0x00B609B0): 0 when blockaded, else Σ_c price_c × trunc(volume_c × share). The domestic
      vector (route +0x3C) is refilled only by the rebuild 0x00BD1020 → 0x00B61720 → 0x00BA76F0 → 0x00B83360 and at a
      raid change; ours computes every take from the current state when the income is read (INFERRED same result).
      What the domestic route's owner loses is UNKNOWN (no net-value reader traced); ours deducts nothing.
    - The economics +0x418 modifier's virtual +4, applied to the booked amount, is UNKNOWN (treated as identity).
    - Model: `World::commerce_raids` (one entry per character and segment, no cap; own save format 8; an older save
      takes the network course (leg spline ids, segment curves) from the campaign's start). The take runs through
      the seam `trade.raid_take`. Income category 9 (`Income::raiding`, history record [9]); the victim's
      international route earns its net value. Command `RaidTradeRoute` (PROVISIONAL: an order not finished this
      turn is dropped). Not modelled: the ESF `COMMERCE_RAIDS` save layout (no vanilla save holds a raid), the UI
      raid order, AI raiding, the raid events (0xBB, `LandTradeRouteRaided` / `SeaTradeRouteRaided`).
    - No ORIGINAL BUG found.
  - Checked: `ECON_TRADE` over the 12 evidence saves unchanged line for line (336 OK, 20 DIFF in our own nr* saves as
    before); `trade_check` finds no saved pair with more than one route, so no save holds a leftover route yet.
  - No ORIGINAL BUG: earning on leftover routes is the exe's consistent behaviour, not a contradiction of its data.
  - The empty exporter entry 0x00BC0120's lookup (operator[] 0x00B9E480) can create needs no state in ours: route counts
    and income read it as no routes, and the only reader that tells "empty" from "none", `trade_port_full`, already
    gives every registry faction an entry from the campaign's construction on (see "Port full" above; the exporter of
    a stored route is never the rebels).

## Capture: occupy, loot, liberate (round 9; code `ntw_sim::campaign::capture`)
CONFIRMED from the exe unless tagged. The module docs hold the formulas; this is the map of the code.

**Capture variants and the queues (worker capture-refund, 2026-10-10; CONFIRMED by disassembly).** Four settlement
methods take a settlement by force; all four first empty the queues the same way, then call
`TransferSettlementOwnership` `0x00B449F0(new owner, force, report flag 1, 1, 0)` (the variant's military force;
`0x00B58A30` passes none):
| Variant | Reached from | Own steps |
|---|---|---|
| `0x00B58560` | settlement vtable slot at `0x0137CA44`, argument a military force (commander handle +0x74, faction handle +0x7C, siege object +0x94) | destroys the garrison (virtual +0x1C), a local slot list it frees unread (`0x00AAA790`), capture report (`0x00885F90`, surrender 0), loc event id `0xFD` |
| `0x00B58890` | slot at `0x0137CA48`, a force | no garrison step, no report |
| `0x00B58C00` | slot at `0x0137CA5C`, two stack arguments: the first never read, then a force | `0x009CB580` on the commander's main attribute, report with surrender 1 |
| `0x00B58A30` | not virtual; only caller `0x00BCA430`, the campaign director's (`CDIR_INTERFACE`, campaign +0xF80) applier of a `CDIR_INTENTION_TRANSFER_REGION_OWNERSHIP` (ctor `0x00B93840`, vtable `0x013801BC`; slot 12 `0x00BAF410` queues (faction, region, faction) in the director's list +0x3C via `0x00BA33A0`). The list is walked at each turn handover (`0x008A9920` → `0x00BA90E0`): an entry for the starting faction whose region still has its queued owner and is still transferable (`0x00BB52E0`) is applied; every such entry is dropped | argument a faction, NULL force to `0x00B449F0`: destroys the garrison, no report. Code: `director::director_transfer` (§Campaign director below) |

**SettlementOccupied and the event order (worker occupyorder, 2026-10-20; CONFIRMED by disassembly).** The
`SettlementOccupied` script event (ctor `InitializeSettlementOccupiedEvent` `0x00889EE0`: event base `0x00DF2F20`, settlement at +0x10, vtable
`0x01355EC0`, whose name getter `0x008BDFC0` returns the string at `0x0134FEFC`) is built only by the three
virtual variants (its sole callers `0x00B5874F`, `0x00B589A6`, `0x00B58DA0`; the vtable is written only by the
ctor), each right after its owner change returns, and posted at once through `0x00E30700` (synchronous script
dispatch). Per variant: `0x00B58560` destroys the garrison (~`0x00B585A9`), builds a local slot list it frees unread (`0x00AAA790`), clears the
queues, changes the owner (`0x00B449F0` at `0x00B586D9`), runs `0x008BD550` (force +0x94 set), `0x00A6AE40`,
`0x00B44920`, pushes the capture report on the context's +0xF70 queue (`0x008E2B80`; the push calls report slot 1
`0x00447880`, which only returns 1; the choice, slot 9, runs later), posts `SettlementOccupied` (`0x00B58755`),
then broadcasts the address of its argument slot on context +0x2A0 (`0x00B5877D`; by then the slot's low byte
holds the `0x00B4D9E0` result) and queues loc event id `0xFD` (with the force's commander) on context +0x9A8
(`0x00B587AE`). `0x00B58890`: owner change `0x00B5897A`, the same three calls, no report, event `0x00B589AC`,
broadcast `0x00B589D4`, loc id `0xFC` (`0x00B58A05`). `0x00B58C00`: owner change `0x00B58D2B`, the three calls,
report (surrender 1) `0x00B58D96`, event `0x00B58DA6`, no broadcast and no loc id. A capture that ends the old
owner does so inside the owner change (`0x00B449F0` → `0x00A64AC0` → `DestroyFactionAndResetRelations`
`0x008AB6C0` at `0x00A652A2`), which posts model event +0x108 (`FactionDestroyed`; human `0x008AB86A`,
multiplayer `0x008ABA4D`, AI without a killer `0x008ABDC2`); its broadcaster (vtable `0x01354CB0` slot +8,
`0x008DBAC0`) calls every listener at once. So the exe's order is: garrison destroyed, `FactionDestroyed`,
`SettlementOccupied`, then the capture choice. Ours: `CampaignModel::occupy` (battles.rs) is the one place that
orders them, and `settle_capture` follows it; tests `a_capture_that_ends_the_owner_reports_its_destruction_before_the_occupation`,
`a_capture_that_leaves_the_owner_reports_the_occupation_then_the_choice`. The director's transfer
`0x00B58A30` and the liberation's owner change `0x00B58A10` (a bare forward to `0x00B449F0`) post no
`SettlementOccupied`; ours posted one for the liberated faction, now removed. The liberation `0x00B4F090` posts
its own script event `FactionBecomesLiberationProtectorate` (vtable `0x0137D064`, name getter `0x008BDE20` →
string `0x01350274`; built at `0x00B4F0E8` / `0x00B50887`, posted at `0x00B4F0FE` / `0x00B5089D`), and the loot
choice `0x00B541E0` posts `CharacterLootedSettlement` (vtable `0x0137CCB8`, at `0x00B54468`; modelled below).
**FactionBecomesLiberationProtectorate (libevent, CONFIRMED by disassembly):** the apply-choice `0x008C0310` calls
`ApplySettlementLiberation` `0x00B4F090` (thiscall, the report's settlement, argument the target faction, `RET 4`)
for choice 2. It tests the campaign key against `spa_napoleon` (`0x00B4F0CC`) and both branches start the same way:
the event is built on the stack (base `0x00DF2F20`, vtable `0x0137D064`), its payload at +0x10 is the settlement's
owner (settlement vtable +0x14 `0x00B2B7D0`: region +0xF4), still the capturer since nothing has changed yet, and it
is posted synchronously (`0x00E30700`) before anything else. Then the capturer's armies are put out of the region
(`0x008D9DC0(capturer, region)` at `0x00B508C5`), then the owner change `0x00B58A10(target, 0, 1)` (the target's
return inside it), then the new army for the target. So it fires before the owner change, the re-emergence and the
capture report; a liberation posts no `SettlementOccupied` and no `FactionDestroyed` (the change's first flag is
clear). The event's vtable has one game accessor, the faction (+0x38 `0x0044A500` returns +0x10; read by the
faction conditions, e.g. `FactionTreasury` `0x008A0860`, registered at `0x0041B8F0`); character (+0x1C), settlement
(+0x40) and the region slots are null, so a script sees only the liberator. Ours:
`CampaignEvent::FactionBecomesLiberationProtectorate { faction }`, pushed by `resolve_capture` (capture.rs) with the
region's owner, then `expel_from_region(liberator, region)` (0x00B508C5), before `liberate_region`; the script host gives it a faction-only context. Tests
`a_liberation_posts_the_protectorate_event_before_the_owner_change` (ntw_sim),
`the_liberation_protectorate_event_gives_handlers_the_liberator`, `a_liberate_choice_fires_the_protectorate_event_with_the_liberator` (ntw_script); the no-capital expulsion in `ai_capture_occupies_and_liberation_hands_the_region_over` (ntw_sim). Side finding for a trace:
`SettlementOccupied`'s vtable `0x01355EC0` has no faction accessor (+0x38 null; its +0x40 is the settlement, +0x2C
the region slot, +0x34 the fort) while our host gives it one. **Ghidra (V5, occupyorder fix round 1; completeness):** `CaptureSettlementAfterAssault` `0x00B58560` 85.3,
`CaptureSettlementWithoutReport` `0x00B58890` 85.3, `CaptureSettlementWithSurrenderReport` `0x00B58C00` 85.3 (all
`CampaignSettlement::` members, argument typed `CampaignMilitaryForce *`).

**`CharacterLootedSettlement` (looting branch, CONFIRMED by disassembly).** The capture report is built by
`SettlementCaptureReport::InitializeSettlementCaptureReport` `0x00885F90` (vtable `0x013574DC`), called only by
`CaptureSettlementAfterAssault` `0x00B58560` and `CaptureSettlementWithSurrenderReport` `0x00B58C00` (at
`0x00B58D8A`); its first argument, kept at report +0x8, is the capturing force's commander (force +0x74, resolved by
`0x00898E80`). The report holds three `CaptureOption` blocks: loot +0x14, occupy +0x40, liberate +0x6C (its
liberation target faction at +0x94). The choice is applied by `ApplySettlementCaptureReportChoice` `0x008C0310`
(callers: report vtable slot 9 `0x008F8D00`, the AI path, at `0x008F8EBB`; the human command at `0x00936043`):
choice 0 runs `ApplyCaptureOptionToRegion` `0x00B541E0` (settlement, loot option, report +0x8); 1 tail-jumps to
`0x00B582E0` (occupy option); 2 tail-jumps to `ApplySettlementLiberation` `0x00B4F090` when +0x94 is set, else
falls into the loot path; any other value does nothing. `0x00B541E0` is reached only from there, so only a loot
(or a liberate with no target) posts the event: after the buildings, money, wealth, the spa_napoleon resentment
and the effects rebuild, it builds the event (`0x00DF2F20`, +0x10 = report +0x8) and posts it synchronously
(`0x00E30700` at `0x00B54468`). The event's name getter `0x008BDDA0` returns `0x0134FAF0` "CharacterLootedSettlement"
("A character loots a settlement"); its vtable answers only the character getter (+0x1C `0x0044A500`, event +0x10):
the faction (+0x38), settlement and region getters are the null stub `0x00461180`. No loot amount is carried. The
vanilla handler is `export_triggers.lua`'s `C_NTW_General_Looter_Good_Trigger` (a General rolls
`C_NTW_General_Looter_Good`, 1 point at 12 then 1 point at 25). With no commander (report +0x8 null, e.g. a
scripted capture) `0x00B541E0` still builds and posts the event (no null check), but the poster `0x00E30700` hands
an event to Lua (`0x00E142B0`) only when its vtable +0xB8 passes; for this event (vtable `0x0137CCB8`, +0xB8 =
`0x008F9EE0`, +0x1C = `0x0044A500`) that check returns 0 at once when the character getter is null, so no script
ever sees a looter-less loot (CONFIRMED). For a non-null character `0x008F9EE0` also requires the event name to be
a `trigger_events_table` record (`0x00E30810`) and that record to be absent from the character's list at +0x1AC
(`0x00EC5360`); ours does not model those two gates yet (open trace). Ours: `CapturePreview::looter` keeps the
capturing force's commander from `capture_preview`; `resolve_capture`'s loot arm pushes
`CampaignEvent::CharacterLootedSettlement { character }` (a plain `CharacterId`) only when there is a looter,
after `apply_outcome` and `loot_resentment`, before `CaptureResolved`; `ScriptHost::context_for` gives it the character only (no faction, so `FactionIsLocal` answers
false as in the exe). Tests: `looting_posts_character_looted_settlement_with_the_capturing_commander` (ntw_sim),
`character_looted_settlement_gives_the_character_and_no_faction` (ntw_script host),
`eur_napoleon_looting_reaches_the_looter_trait_trigger` (real install: the vanilla trigger runs and the trait is
gained). Open: the AI's choice `0x00AAABA0` is not decoded (ours: PROVISIONAL occupy), so the AI never loots in
ours yet. **Ghidra (V5, completeness):** `ApplySettlementCaptureReportChoice` `0x008C0310` 100,
`InitializeSettlementCaptureReport` `0x00885F90` 92 (the vtable global's bare pointer type), the name getter
`GetCharacterLootedSettlementEventName` `0x008BDDA0` (function created) 95, `ApplyCaptureOptionToRegion` plate
updated 98; struct `SettlementCaptureReport` created; `g_apfnCharacterLootedSettlementEventVtable`,
`g_apfnSettlementCaptureReportVtable`, `g_szCharacterLootedSettlementEventName` named.

The shared queue step: the land queue (region +0x124) `0x00B1A760(0)`; for each slot of the region's slot manager
(+0x120, vector +0x18 / count +0x14) with a port recruitment manager (slot +0x1E8) `0x00B1A760(1)`, then the slot's
construction `0x00A6CBE0(0)`; then the two single slots +0x24 / +0x20 (walls / road) `0x00A6CBE0(0)`.
- `0x00B1A760(flag)` cancels every item through `0x00B1A820(item, flag)`: the item's virtual +0x1C with the flag
  (`0x00B5C060` land, `0x00B5C0A0` naval: with the flag, item +0x20, the stored cost, goes to the faction through
  `0x00BB3810(cost, 3)`; that faction is the queue's region's owner, item +0x10 → manager +0x54 → region +0xF4 via
  `0x00B2B7C0`, still the old owner here), then the population credit `0x00A620A0` **whatever the flag**.
- `0x00A6CBE0(0)` → `0x00B1A790(0)`: a construction is refunded only with the flag (and a slot item, cost ≠ 0), so
  never on an owner change.
- So a capture gives the old owner back its queued ships' cost and nothing for its land units or buildings.
- `0x00B449F0` → `0x00A64AC0(new owner, report flag, 0, 0, a, b)` drains again (constructions `0x00A6CC10` with 0;
  land queue and each port's queue `0x00A6CC40` → `0x00A6CBF0` with `flag = (old owner +0x6E0 clear) or report`),
  after reading the old owner and before the region joins the new one. After a capture the queues are already
  empty. A transfer without a capture, `0x00B58A10(owner, a, b)` = `0x00B449F0(owner, 0, 0, a, b)` (the deal's
  region item `0x00C18BF0` with (1, 0), liberation `0x00B4F090` with (0, 1), `grant_faction_handover` `0x00978950`
  → `0x008AB6C0`), has report 0: an AI old owner (+0x6E0 = 0, the human flag, SAVE_COMPAT.md) gets every queued
  unit's cost back, land and naval; a human old owner gets nothing. Not an original bug by the rule (a deliberate
  branch on the human flag; no data contradicts it).
- Model: `battles.rs` `occupy` runs the capture variants' drain (`clear_region_queues`, land no / naval yes) before
  `change_region_owner`, whose own drain refunds both kinds when the old owner is not human (`turn.humans`). Land and
  naval items share `Region::recruitment_queue`; a ship's item is the port's (`CampaignRules::is_naval_unit`), checked
  on 478 queued items of the 12 top-level evidence saves against the record they were loaded from (a port's
  manager or the region's). Tests: `a_capture_refunds_the_old_owners_queued_ships_only`,
  `a_transfer_refunds_an_ai_old_owners_queued_units_and_a_humans_none` (ntw_sim),
  `a_capture_in_the_vanilla_saves_refunds_the_queued_ships` (ntw_campaign: 36 captures of regions with queued ships).
- Ghidra pass (polish9, 2026-10-11; disassembly; the +0x98 reading corrected by occupyorder fix round 4,
  2026-10-20): `TransferSettlementOwnership` reads the passed force's +0x98 and, when it and its +0x34 are both
  non-NULL, compares that +0x34 with the region's current owner (region +0xF4, `0x006649C0`) at
  `0x00B44A3A`–`0x00B44A67`, never the new-owner argument. Force +0x98 is not the force's faction (that is the
  handle +0x7C the capture variants pass as the new owner) but the army's `REBEL_DATA` (0x74 bytes; set only on
  a risen rebel army, PATHFINDING.md "REBEL_DATA"; built by `0x00886BA0`, called by the risings `0x00870E60` /
  `0x008AF5D0`). Its +0x34 is a faction pointer or NULL: `0x00886D30` stores there the rising record's +0x28
  (`0x00886BA0`'s fifth argument), the save reader `0x00886C90` zeroes it and fills it only when the record's flag
  is set, and when set the rebels take their name from that faction (`0x008C73A0`, a faction method,
  called with it at `0x00886DF6`). When they match it skips `0x00B44AC7`–`0x00B44B7F`
  (`0x00B2B810`, the drain `0x00A64AC0`, the owner-change signals, `0x00B1B300`) but does not return: the +0x730 step
  at `0x00B44B84` still runs. With no force (`0x00B58A10`: deals, liberation, handover; `0x00B58A30`: the
  director), or a force that is not a risen rebel army, there is no skip. Whether a rebel army can capture a
  settlement whose owner is its `REBEL_DATA` faction (the one case that skips) is not traced (UNKNOWN). A deal
  can hand a region to its own owner; the drain
  `0x00A64AC0` then returns early but `0x00B1B300` still runs (corrected 2026-10-20, see "Same-owner transfer" below:
  the model no longer returns early in `transfer_region`). `CancelRecruitmentItem` ends by firing the manager's multicast signal (+0x24, vtable
  `0x0137CBD8`, slot +8 `0x008DBAC0`: each subscriber's vtable +4 with an item pointer) after the refund, population
  credit, delete and count; the pointer is the item now in the cancelled one's place, or the freed item when it
  was the last. Subscribers register at runtime: UNKNOWN (a debugger breakpoint at `0x008DBB59` while cancelling
  settles whether any reads it; an ORIGINAL BUG only if one does). Completeness of the nine capture-refund
  functions 7–66 → 76–97 (new structs `RecruitmentQueueItem`, `RecruitmentManager`, `RegionSlotManager`,
  `CampaignSettlement`, `SlotConstructionItem`, `SlotSnapshotVector`; faction +0x72C labelled `pCapitalRegion`). Reconciled (polish11, 2026-10-11): the rules that read +0x72C are
  CONFIRMED (`0x00A8B5A0` region == its owner's +0x72C, used by the deal's region list `0x00C5C040`; the bankruptcy hit
  `0x008AE710`; `AddRegionToFaction` `0x008AE270` stores it when the faction has none, when the region is its +0x730
  home region, or on its make-capital flag), so +0x72C is the capital. **Save → +0x72C (CONFIRMED, polish13,
  2026-10-12, disassembly):** the faction loader `0x0087A190` (called after `0x0090BEB0` allocates the faction at
  `0x0090C0A4`) checks for the optional `FORT_UPGRADE_MANAGER` block (`0x00B02530` returns that block name,
  tested at `0x0087B1D3`; the manager goes to +0x728 at `0x0087B1F7`), then reads one i32 (−1 when the read
  fails) at `0x0087B20C` and stores it to +0x72C at `0x0087B217`; the next i32 goes to +0x730 (home region,
  `0x0087B23C`). After loading, `0x008E07F0` (`0x008E0816` / `0x008E0829`) turns the saved id into the region
  through the saved-id map `0x0105AC60`. **Loader order around it (CONFIRMED, payment-capital, 2026-10-12,
  disassembly):** a bool (`0x0087B19A`); when set, a 0x9C object holding a `REGION_KEYS` array (`0x00903DA0`) →
  +0x724; the optional `FORT_UPGRADE_MANAGER` (`0x0087B1DA` skips it when absent); the capital i32 (`0x0087B20C`);
  the home-region i32 (`0x0087B23C`); then the `EXPOSED_CHARACTERS` array, opened unconditionally at `0x0087B254`.
  So the capital is always the second i32 before `EXPOSED_CHARACTERS` and the home region the one right before
  it, with or without the manager block. Our importer (`crates/ntw_campaign/src/details.rs`, `faction_details`)
  now anchors on `EXPOSED_CHARACTERS` the same way (it used to read after the last `FORT_UPGRADE_MANAGER` and set
  none without one); 0 and negative ids (the loader's default is −1) mean none. Test
  `faction_capital_read_with_or_without_fort_upgrade_manager`; the real-save capital checks in
  `tests/real_install.rs` still hold.
- **Same-owner transfer** (sameowner, 2026-10-20, CONFIRMED by disassembly; code `battles.rs` `transfer_region`,
  `change_region_owner`, `pass_region_slots`; tests `a_same_owner_deal_handover_stops_research_and_passes_slots_only`,
  `a_same_owner_handover_updates_the_prices_only_through_a_port`). The regionprice review's premise (the taxes, the
  policing tracker and the port price update all refresh for a transfer to the same owner) was only partly right.
  - `TransferSettlementOwnership` `0x00B449F0` (new owner, force or NULL, report flag, a, b; `RET 0x14`) never compares
    the new owner with the old one; only a passed force whose `REBEL_DATA` (+0x98) faction (+0x34) owns the region skips the transfer
    (above). Its five callers:
    - `TransferSettlementWithoutCharacter` `0x00B58A10` (call at `0x00B58A20`: new, NULL force, report 0, a, b).
      Its callers: the deal's `ApplyRegionsRecordOnAccept` `0x00C18BF0` (`0x00C18C26` demanded list → proposer,
      `0x00C18C9C` offered list → recipient; a 1, b 0); the liberation `0x00B4F090` (`0x00B50202`, `0x00B511DF`; a 0,
      b 1; called only from the capture choice 2 at `0x008C0310`); `DestroyFactionAndResetRelations` `0x008AB6C0`
      (`0x008ABC30`, `0x008ABC98`; a 0, b 0).
    - `CaptureSettlementForDirectorTransfer` `0x00B58A30` (`0x00B58B4D`: new, NULL force, report 1, a 1, b 0),
      only from `ApplyDirectorRegionTransferIntention` `0x00BCA430` ← `0x00BA90E0` ← `AdvanceFactionTurn` (`0x008A9BAA`).
    - The capture virtuals `0x00B58560` (`0x00B586D9`), `0x00B58890` (`0x00B5897A`) and `0x00B58C00` (`0x00B58D2B`),
      settlement vtable `0x0137CA44` / `0x0137CA48` / `0x0137CA5C`: they pass the capturing military force.
  - Which can pass the region's own owner:
    - Deal: yes. Nothing checks a deal's region owners (`propose_regions`); the panel's and the AI's lists hold only
      each side's own regions, so it takes a command naming a region its receiver already owns (a peer's
      `CCQ_DIPLOMACY_PROPOSE_REGIONS`, our commands) or a region listed on both sides.
    - Liberation: no. The target is out of the game and not the capturer, who owns the region after the occupation
      (ours: `liberation_target`; `liberate_region` also refuses the owner).
    - Director: no. An entry is applied only while the region's owner is the queued non-major owner
      (`IsSameComponentId` `0x00CF0D30`) and `0x00BB52E0` passes; the receiver is a major other than it (`0x00BA2B90`).
    - Captures: every capture path takes a region its capturer (force +0x7C) does not own. The skip in
      `0x00B449F0` is not about the capturer: it needs a risen rebel army whose `REBEL_DATA` faction owns the region
      (above, UNKNOWN whether that happens).
    - Destruction: without a killer each region goes to the best other faction. With one (the script's
      `grant_faction_handover` `0x00978950`, a mission reward `0x00A06050`) every region goes to the killer and nothing
      checks killer ≠ target: with the same faction `0x00A64AC0` returns early, the region never leaves the faction's
      list (+0x76C) and the loop never ends (a hang). No vanilla data does it (mp_eur hands bavaria and wurttemberg to
      france). Ours (handover, 2026-10-20): both paths call `destroy_faction_by`, which refuses a killer that is the
      target ("Destruction by a killer" under §Destruction and re-emergence).
  - What runs for the same owner. In `0x00A64AC0`, before its return (`CMP` of the region's owner, region +0xF4, with
    the new one at `0x00A64D11`, `JZ` to the epilogue at `0x00A64D13`): `0x008B37B0(new)` (faction +0x950 and MP
    stats, only when the new owner is human, the third argument is 0 and the old owner is human; not modelled), the
    region's UI notifier (region +0xC → `0x009C7660`, vtable +0x6C), `0x008E63B0` when the report flag is set, the
    region +0x21C emergent-faction step for b = 0 (not modelled), and the research stop `0x008B4AB0` at `0x00A64CE5`.
    Then in `0x00B449F0`: the second `SetSettlementOwner` (`0x00B44B2C`), the settlement +0x24 event and the context
    +0x420 listeners, `TransferRegionSlotOwners` `0x00B1B300` (`0x00B44B69`; a third faction's slots come back, each
    port slot passed runs the price update), then `0x00A6AE40` and `0x00AB3DF0`. The +0x730 step at `0x00B44B84`
    does nothing (it needs the old owner other than the holder, `0x00898B80`).
  - Not run for the same owner (all after `0x00A64D13`): the queue clears, the diplomacy event `0x00B62120`, the
    capitals, the routes `0x00BBE1A0` (`0x00A64E87`), the expulsion (`0x00A650E4`), the policing tracker `0x00AA16E0`
    (`0x00A65103`), the taxes `0x00BBD7C0` (`0x00A6512D`, `0x00A6513E`), the end on a lost capital (`0x00A652A2`).
  - Ours: `transfer_region` no longer returns early; `change_region_owner` runs the research stop and the slot pass
    (`pass_region_slots`, shared with the different-owner path), then the price update when a port slot passed, and
    returns. `transfer_region` returns true only when the owner changed, so a same-owner deal item ends no giver; it
    still counts as a region received. Not ported here, as for the different-owner path: the UI notifier, the
    listeners, `0x008B37B0`, `0x00A6AE40` / `0x00AB3DF0`. ORIGINAL BUG: the destruction hang above, fixed in
    `destroy_faction_by` (logged refusal).

**Flow.** A capture (after a won assault, `0x00B58560`; an undefended settlement, PROVISIONAL same variant) changes the
owner and clears the queues (round 8), then queues a report object (`0x00885F90`, vtable `0x013574DC`; flags +0x98
pending, +0x99 surrender, +0x9A / +0x9B skip the choice). Its slot 9 `0x008F8D00`:
- computes the preview `0x00B14930(report+0x14, surrender, looting multiplier 0x008D20F0, faction)`;
- AI faction (faction +0x6E0 = 0) without the skip flags: choice = `0x00AAABA0` (AI virtual +0x2F8);
- human without the skip flags and the campaign flag (+0x114): a UI event (+0xC78) with the three option structs;
  the screen fills `SettlementLootingOptions` (`0x00A13DB0` → `0x009AB1B0`);
- otherwise choice 1; then `0x008C0310(choice)`.

**Option structs** (report +0x14, +0x40, +0x6C; 0x2C bytes each): {+0 vector header, +4 capacity, +8 count, +0xC
(building, value) list, +0x10 money, +0x14 town wealth after, +0x18 upper-class / +0x1C lower-class public order
preview (below), +0x20 public-order reduction, +0x24 flag, +0x28 liberation
faction}. The third struct's flag (+0x90) and faction (+0x94) mark
a liberation; `0x008C0310(2)` loots when +0x94 is 0.

**Public-order previews** (capturepreview, CONFIRMED from the listing of `0x00B14930`, `0x00A61400`, `0x00A610C0`,
`0x009AB1B0`):
- Per option (loot, then occupy), on a copy of the region's population factors (`CopyRegionFactors` `0x00A4C1D0`, region
  +0x4C): loot only, `0x00AAA5B0(reduction)` on the copy (each class +0x74 −= reduction, `0x008EF780`; then the spa
  alignment shift); the region's effect set (`0x00A67530`) is adjusted by `0x00B0D3B0(capturer, region, set)` and has
  the full local (level +0xE0) and faction-wide (level +0x100) effects of every building in the option's damage list
  (fortification included) subtracted; `RefreshRegionPopulationFactors` `0x00AA9C10(copy, set, 0, -1)` recomputes the
  classes. The copy's region pointer is the real region, already owned by the capturer (the owner changes before the
  report is queued).
- +0x18 = `GetUpperClassNetPublicOrder` `0x00A61400(copy)`: the net public order (`GetPopulationClassNetPublicOrder` `0x008E2AB0`: class +0x38 + +0x3C + +0x58, an int) of the first
  class for which `IsUpperClassOfRegionGovernment` `0x008CE320` holds, i.e. the class record equals the upper class (+0x18) of the government (+0xB0, `CompareGovernmentUpperClassRecord` `0x008CE2D0`) of
  the region's faction (region +0xF4, faction +0x70C). For the rebel faction (`0x008CEEF0`), CONFIRMED from the listing
  `0x008CE341`–`0x008CE368` (`0x008CE281`–`0x008CE2A8` in the lower test): region +0x220 is the region's set of risen
  rebel armies (count +0xC, `0x006A3110`); with at least one, the class is compared with the first army's
  (`0x008CFC20(0)`) REBEL_DATA (+0x98) → government record (+0x18, the GOVERNMENT_TYPE record per `0x00886F10`) →
  upper class +0x18 (lower +0x1C); with none the test is false, so both previews are 0. +0x1C = `GetLowerClassNetPublicOrder` `0x00A610C0`, the same with `IsLowerClassOfRegionGovernment` `0x008CE260` and the lower class (+0x1C, `CompareGovernmentLowerClassRecord` `0x008CE210`). Each is
  **0 when no class matches** (unlike `FindUpperPopulationClassEntry` `0x00A61320` / `0x00A60FE0`, which fall back to
  the first class).
- The screen (`0x009AB1B0`, ESI = option − 4): `Loot` ← +0x10, `TownWealth` ← +0x14, `UpperOrder` ← +0x18, `LowerOrder`
  ← +0x1C (string pushes at `0x009AB4A4`–`0x009AB4D6` and `0x009AB831`–`0x009AB863`).
- The preview runs for every capturer (AI included); it draws nothing.

Ours: `CaptureOutcome::public_order_after` = (lower, upper) of `economy::public_order` on a clone with the option
applied (buildings, fortification, town wealth, class reduction, spa shift): the same governed-class rule
(`governed_classes`, 0 for a missing class) and the same damaged-building removal (`Region::effect_buildings` skips a
damaged building); the UI maps lower → `LowerOrder`, upper → `UpperOrder` (test
`capture_previews_are_the_governments_upper_and_lower_classes`). Computed for human capturers only (display data).
Rebels (rebelgov 2026-10-20, CONFIRMED by decompile and data): `economy::region_government_classes` is the region
tests' one port, read by `governed_classes` (so the previews, `public_order` and the order attitude): for a
rebel-owned region (region +0xF4) the first id of `World::region_risen_armies` (the set +0x220 in file order:
`0x00A45950` → `0x008CFB60` → `AppendOwnedLinkToOwnerList` `0x008CF7D0`; an army's destructor `0x0088CE30` →
`0x0088F310` → `0x0088CA00` → `0x008B0150` erases its link and shifts the rest down, ours `remove_force`), whose
government is `World::rebel_army_governments` (the army's own ARMY #7 `REBEL_DATA` → `REBEL_DATA_INNARDS` →
`REBEL_SETUP` #3, read by `0x00870FD0` → `0x00886950` → `0x00886C90` / `0x00886F10`; v<4 blocks hold `REBEL_SETUP`
directly; written back by the ARMY writer `0x008FA810` via `0x008971C0` at `0x008FAA09`); no risen army or no
government → no class (both previews 0). Data: orig_fr_may1811's 8 rebel ARMYs all name `gov_absolute_monarchy`
(test `vanilla_save_risen_armies_keep_their_government`; model test
`a_rebel_regions_classes_are_its_first_risen_armys_government`). Our save writer aimed a rebel army's navy u32 at #7, the
REBEL_DATA record, so the type check skipped it and the navy link was silently not saved (our bug, fixed:
`army_navy_slot`, #8 behind the block, test `a_rebel_armys_navy_goes_behind_its_rebel_data`; a new army drops a
rebel template's block). The faction-level tests (`0x008CE230` / `0x008CE2F0`) stay `economy::government_classes`.
Left, PROVISIONAL: `RefreshRegionPopulationFactors`' per-class pass `0x008EDF70` picks classes with the faction tests,
which match none for the rebels, so it refreshes nothing and the stored factors stay (ours computes default
`upper` / `lower` factors: `public_order_factors_with`); and the region panel's `ActiveUpperClass` /
`ActiveLowerClass` read `0x00A975F0`, which for a rebel owner takes the region's REBEL_DATA template (+0x21C →
+0x18 government; `0x00A91EB0` frees the template when the region's risen list drops below 2 entries and the
owner is not the rebels) and with no template the first `government_types` record (db +0x4D8, `0x00E1F150`), not
the risen army's (ours: default classes; region_info.rs).
Not traced: what `0x00B0D3B0` adds (an effect set built by `0x00BC74B0` from `lower_classes;` / `upper_classes;`
tax-level keys, when the capturer has a record and `0x008C8D90` finds its entry for the region) and subtracts (the
region's governorship container, region +0x248 → `0x00BA2170`); ours takes the tax bundles of the governing faction
(`region_effect_set`), equivalence INFERRED.

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
  (the out-of-game flag, CONFIRMED: set by the destruction `0x008AB6C0`, cleared by the re-emergence
  `0x008AE6E0`; ours `FactionDetails::out_of_game`, see §Destruction and re-emergence) or (+0x72C = this region and a count of 1: not decoded,
  not offered); finally the capturer's relationship #28 `allows_region_return`. Campaign flag +0x124 is the scripts'
  `set_liberation_options_disabled` (episodic restrictions +0x124, tested on both paths at `0x00B15134` / `0x00B15214`;
  CONFIRMED, CAMPAIGN_DATA.md §4.2): ours `World::liberation_options_disabled`, read first in `liberation_target`.
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
  every population class's +0x74 (the war-results base, our `PopulationClassState::resistance`) −= reduction (`0x008EF780`), region
  +0xD4 := TW after, then `0x00AB4410(0)` copies +0xD4 into +0xCC and refreshes the region economy without growth;
  effects recomputed.
- occupy `0x00B582E0`: the occupy list's healths only.
- liberate `0x00B4F090` (`ApplySettlementLiberation`): posts `FactionBecomesLiberationProtectorate` (the liberator),
  puts the capturer's armies out of the region (`0x008D9DC0` at `0x00B508C5`, CONFIRMED; ours: `expel_from_region(liberator, region)` right after the event, so a capturer without a
  capital, which the owner change's own expulsion skips, is moved out of the settlement too and stays in the region on the map: the owner is still the liberator at expel time, so it may stay), then `0x00B58A10(target, 0, 1)` → `0x00B449F0` hands the region over; then an army for the target
  (units filtered by `0x00B69BA0` from what the region's `sArmy` / `rHorse` / `tGuns` allow, a named general,
  `0x008D24B0`): not modelled. spa_napoleon has its own branch (guerrilla units).

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
  faction +0x3F4 minus the amount, plain 32-bit (wraps; `pay` saturates instead, ORIGINAL BUG in §Treasury changes; the category converter
  `0x00BA8CD0` taken as identity, INFERRED). Construction: `can_pay_construction` = the unsigned card test and
  treasury ≥ cost signed. Repair: `0x00B16430` (the panel's `can_afford_repair`, and the AI's `0x00AA4910`) is
  repair cost ≤ treasury, signed, on the cost that `0x00B66410` caps at the treasury for an AI faction, so an AI
  always passes; the repair command (`CCQ_BUILDING_REPAIR` handler `0x00931DA0` → `0x00B66260`) tests nothing and
  charges only a cost above 0, but stores the cost in the item either way (`0x00AE7010` → item +0x14). So a repair
  costing 0x80000000, or an AI repair in debt, is queued for free.
- **Cancel refund (CONFIRMED).** `CCQ_BUILDING_CANCEL_CONSTRUCTION` (registered by `0x0041F500`, handler
  `0x00931C30`) calls `0x00B1A790` with the command's flag (the panel's `CancelConstruction` `0x009E0F10` sends 1):
  when the slot's item stores a cost other than 0 (item +0x14), `0x00BB3810(cost, 3)` credits it, faction +0x3F4
  plus the amount, plain 32-bit (`refund`; ours saturates, ORIGINAL BUG in §Treasury changes; category 3 converter `0x00BA8CE0` taken as identity,
  INFERRED). The stored cost is credited whatever was charged: cancelling a free 0x80000000 repair credits
  0x80000000, an AI debt repair's −20 credits −20. Saves keep the cost bit for bit (`BUILDING_CONSTRUCTION_ITEM`
  #4, u32). Tests: `treasury` module, `a_repair_costing_i32_min_is_queued_and_charges_nothing`,
  `an_ai_repair_is_capped_at_the_treasury_and_free_in_debt`, `cancelling_a_repair_credits_its_stored_cost`,
  `walls_round_trip_in_both_slot_shapes`, `a_treasury_below_zero_shows_every_card_affordable_but_the_command_refuses`,
  `a_negative_cost_shows_unaffordable_and_is_paid_out_only_from_a_negative_treasury`.
- Every other treasury change (the unit pool, treaties, the round settle, the script) is in §Treasury changes.
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
  in entry[4]. Data: `economy_check` `ECON_RECRUITCOST` matches **394 of 406** queued items in the saves the original
  wrote (`auto_after_c8` 90/97, `auto_nr1` 46/47, `auto_nr4_t4` 98/99, `orig_fr_may1811` and `orig_fr_t1_b` 14/14 each,
  `orig_over_nr4_0252` 132/135; `nr2`/`nr3` are our own saves, charged #4 by the old code).
- **The 12 misses are effects that changed after the AI queued the item (worker recruit-cost 2026-10-10; Spain's two
  were our own key gap, fixed by recruit-cost2 2026-10-14, below).**
  - The price is taken once (CONFIRMED). `0x00AF3F80` copies entry[0] to item +0x20 (and +0x28) at queue time.
    The count-down (land item vtable `0x0137CB1C` +0x18 → `0x006009F0`) touches only +0x1C. Nothing re-prices an
    item: the Naples items stay at 551 from `auto_after_c8` to `orig_over_nr4_0252` while the faction reads +1.
  - What it is priced from (CONFIRMED): the region effect set `0x00A67530`: the region's cached +0x188, its
    +0x1DC, and `0x008AFB50` over the faction's cached sum +0x6FC. That sum is rebuilt only by `0x008B16C0` (when, below).
    It is a copy of +0x8D4 (saved base + difficulty), plus the owned regions'
    faction-wide building sets (+0x198), plus the technology set (+0x7C4 → +0x18), plus the government part
    `0x008AFAB0`. That part is the government set, and for each non-governor post with a holder, the post level's
    `ministerial_positions_to_effects` set (`0x008D9D40`) and the holder's own set (+0x48C).
  - Every miss lies in the army or navy post's term, or in the saved bonus. The formula and the region part
    match (a temporary diagnostic broke each faction's sum down by source):
    - Naples (7 items, `auto_after_c8`, `orig_over_nr4_0252`): priced at −5. The save has the army minister at
      level 1 (+6 cost, +5 % land upkeep); priced at level 2 (0, 0 %). The round-end land upkeep in
      `FACTION_ECONOMICS` goes 990 → 1039 (+4.9 %) in the round the 2/2 items were queued. So the level dropped
      after Naples' turn and before the round end; the model's upkeep at the save, 1039, matches.
    - Britain (3 items, `auto_after_c8`): priced at −6 (iron mine −3, stables −3 on the dragoons, the infantry's own
      −3), i.e. the faction part 0 (army level 2). The save has army level 0 (+12 cost, +10 % upkeep). Land
      upkeep goes 3870 → 4268 (+10.3 %) at the end of the round the items were queued in.
    - Saxony (2 items, `auto_nr1`, `auto_nr4_t4`): priced at −5 (the government's). The save adds −5 from a trait
      of the navy minister (his +0x48C), gained after the item was queued.
    - Spain (2 copies of one merchantman, `orig_fr_may1811` = `orig_fr_t1_b`): priced at +49 = the basic
      `recruitment_mod_cost_naval_all` +24 (saved +25, navy level 3 −1) + a unit-category `cost_mod` +25 on
      `naval_merchant`. CONFIRMED (recruit-cost2 2026-10-14): every `spa_napoleon` faction's saved base #54 (and #55)
      holds a `CAMPAIGN_BONUS_VALUE` {type 0xE, bonus 0, +25, `naval_merchant`}; `0x00B0D220` adds the type-0xE id-0
      sum keyed by the unit's category (unit +0x24, thunk `0x00E1F0F0` → `0x00E23E10`) and the type-0xD id-0 sum
      keyed by its class (+0x28, `0x00E1F110`). The ids are the indices of `g_apszUnitCategoryBonusNames`
      `0x0145B75C` (7: cost_mod, upkeep_mod, melee_attack_mod, movement_mod, training_mod, land_command, sea_command;
      `FindUnitCategoryBonusId` `0x00E22570`, from the junction key builder `0x00F74450`) and
      `g_apszUnitClassBonusNames` `0x0145B778` (4: cost_mod, upkeep_mod, melee_attack_mod, experience_mod;
      `FindUnitClassBonusId` `0x00E225B0`, builder `0x00F744E0`). Ours kept saved types 0xE / 0xD under raw
      `Saved(n)` keys the DB junction keys never met; `SavedBonus::key` now maps them to `UnitCategory` / `UnitClass`
      by those tables (and `add_saved` back), test `the_spanish_merchantmen_price_takes_the_saved_unit_category_cost_mod`.
      Our own mapping gap, not an exe bug. Saved type 6 (population class) and type 10 (religion) now map the same way, and type 7
      (population class × religion) stays a raw key but now reads and writes both its qualifiers (class, then
      religion; reader `0x00DF0D10`, writer `0x00E02F80`, CONFIRMED, polish20); type map and name slices in
      EFFECTS_FIDELITY.md §1 (polish18).
  - So the other misses are not a pricing error, and no save can be matched without the state at queue time. The
    model prices from its state at its own queue time, as the exe does.
  - **When `RebuildFactionEffectSum` `0x008B16C0` runs** (its 15 callers, CONFIRMED by xrefs, recruit-cost2
    2026-10-14): each faction's turn start (`AdvanceFactionTurn` `0x008F4120` → `0x0096C050` → `0x008F1F60`, skipped
    while faction +0x824 is set); a unit restriction added or lifted, over every faction (`AddRestrictedUnitRecord`
    `0x009C9700` and `RemoveRestrictedUnitRecord` `0x00A1D3B0`, called by their script bindings `0x00977500` /
    `0x00977570`; the remove also by the mission reward `ApplyMissionSuccessReward` `0x00A06050`, so a mission
    reward lifts a restriction; CONFIRMED by xrefs, unitrestrict 2026-10-20); a region or settlement changing hands (`TransferRegionToFaction` `0x00A64AC0` via `0x008AE6E0` / `0x008E4420`,
    `TransferSettlementOwnership` `0x00B449F0` via `0x008BEAA0`; the capture options `0x00B541E0`, `0x00B582E0`);
    a technology granted (`GrantFactionTechnology` `0x008CDCB0` unless its param_2 is set; `0x00948A40`, `0x00948AD0`);
    `0x0095FF00` (from `0x00872550`); and three
    command handlers near `0x00936530` (`0x008F3760`, `0x008C9C50`, `0x008EB9D0`; which commands is UNKNOWN). None is
    on a trait or minister-level change, so a sum reader can see a stale +0x6FC until the next of these. Ours
    sums on demand. Whether any reader observably lags is a question for the effects system (every reader of the
    sum, not only recruitment); the remaining 12 misses are explained without it.
- **`units_to_groupings_military_permissions` is read** (CONFIRMED, recruit-cost2 2026-10-14, BACKLOG §0-B): the
  table constructor `0x00E86820` loads it through `0x00DC11A0` keyed by unit + grouping (analysis/mods/MOD_LOADING.md),
  and the faction unit-set builder `0x00F74CB0` (`FACTION_RECORD` +0x1AC) starts each faction's list from its grouping's
  (`factions` #34) units before the `units_to_exclusive_faction_permissions` rows. Ours: `faction_unit_set`
  (ntw_sim campaign rules) fed from the merged `unit_groupings` table in `rules_from_db`
  (ntw_campaign rules.rs), mods included; no `db/...` path read.
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
  The card's `reasons_unavailable` list (no slot, unaffordable, population, damaged, occupied, siege, limit,
  technology) matches all eight bits (INFERRED: the generator `0x009FE7B0` was not traced that far); the UI shows the
  model's flags.
- **The building flags (CONFIRMED, worker recruit-cost 2026-10-10; code `CampaignModel::recruitable_units`).** The
  priced list is a copy of the region's unpriced list (region +0x1A8). `RebuildRegionRecruitablesAndEffects`
  `0x00A6AE40` builds that list per slot through `0x00A62380` → `BuildSlotBuildingRecruitableList` `0x00B43CA0`,
  which flags each entry on its own:
  - 0x08 when the building's health (+0x18) is below 100.
  - 0x10 when `0x00A91FC0` holds: the slot's faction is not the region's owner. The same test keeps such a slot's
    building effects out (`0x00A62380`).
  - 0x20 when the settlement (region +0xFC) answers its virtual +0x98. The script function `IsUnderSiege`
    (`0x008A1FF0`, strings `0x01352184` / `0x013521B4`) calls the same virtual. The model has no campaign sieges
    yet, so this flag is never set (BACKLOG "Sieges on the campaign map").
  - 0x80 when `0x008AAB60` fails: one of the unit's technologies (unit +0xCC/+0xD0, linked from
    `unit_required_technology_junctions` by `0x00EA9B10`) is not in state 0 (`0x008F3DB0` reads state 5 for one
    the tree lacks). So a unit whose technology is missing is listed and flagged, not left out. Our tech gate was
    INFERRED before and is now CONFIRMED.
  - Some units get no entry at all. These are the ones that fail the faction's permission mask or unit set, or one
    of three gates:
    - `0x00EA9810` (CONFIRMED, ported as `CampaignRules::government_may_recruit`, test
      `a_unit_enabled_for_other_governments_is_not_recruitable`). The unit's list +0xDC holds the government types
      of its `enabled` rows in `units_to_gov_type_permissions` (`0x00E91320` keeps rows whose status is 0; the
      status strings `0x00ED5710`: enabled 0, destroyed 1, converted_to 2, non_reinforceable 3). When that list is
      not empty, the faction's government type (faction +0x70C → +0xB0, `0x008C8CF0`) must be in it. Vanilla has
      51 rows over 17 guard units: the Republican Guards are enabled only for `gov_republic`, the others only for
      the two monarchies. So France (`gov_empire`) gets none of them. What `destroyed` / `converted_to` do on a
      change of government is not traced.
    - `0x00AA1660` `CheckUnitRegionResourceAvailable(pUnitRecord, pRegion)` (CONFIRMED, traced 2026-10-13;
      ported in `recruitable_units` as `UnitRules::unit_resource` ∈ `Region::unit_resources`). True when `UNIT_RECORD` +0x98 is null; else that pointer must be in
      the region's list +0x274 (count +0x270). Sources, both CONFIRMED: +0x98 is set by the units row builder
      `0x00EDC000` from units column #14 (builder +0x78, our `UnitRecord::recruitment_scope`, e.g. `global`): a
      non-empty value is looked up in `region_unit_resources_table` (`0x00E29830`), an unknown key drops the row
      (returns 0), and the record ctor `0x00E91320` stores the found record at +0x98 (its param 7). The region's
      list +0x274 is filled by the region ESF loader `0x00A45950` at `0x00A467BE` from `RESOURCES_ARRAY` (each
      string resolved against `region_unit_resources_table`; unknown → logged, stored as null, never matches),
      and the region writer `0x00A51E30` writes `RESOURCES_ARRAY` back from it (string = record +0, the key).
      So the gate is: a unit with a `recruitment_scope` is listed only in a region whose `RESOURCES_ARRAY` names
      it (vanilla values per WORKER3_REPORT: `global`, `horses`, ...; 37 `region_unit_resources` rows).
      Mod data (CONFIRMED, recruit-followups 2026-10-14): every `units` read through `ntw_data::load_table`
      (the `GameDatabase`, `ScriptSource::typed_table` for army setup, tools) runs the table's row check
      (`DbRecord::checked`, `checked_by = drop_units_with_unknown_resource`), which loads `region_unit_resources`
      (merged, mods included) and drops a `units` row whose non-empty #14 is not one of its keys, one warning
      per dropped row, as `0x00EDC000` does (tests `a_unit_with_an_unknown_unit_resource_is_dropped_with_a_warning`,
      `load_table_drops_units_naming_an_unknown_unit_resource`). Its caller `0x00E9B700` (units load loop,
      `__thiscall`, RET 0x10) rewinds the record slot (−0xFC) and normally goes on with the next row
      (INFERRED): for each table the builder blamed (`0x00EDC000` pushes the `region_unit_resources` table into
      its error list on a miss), it reads that table's pack-name list (DATABASE_TABLE +0x38, size +0x40, data
      +0x44, through vfunc 0 `0x00542E90`) and returns at once (`0x00E9BD95`, returning the current slot − 0xFC,
      an interior pointer, so the remaining rows are never built) when a name is already in the shared
      bad-pack list (its param 4, DB manager +0x6FC). Otherwise it adds the row's own pack name (or "Invalid
      key"), reports the list through the global at `0x010A40E0` vfunc +0x6C and appends the newest name to
      the bad-pack list. Both table constructors traced (`0x00E6DF60`, `0x00F24BB0`) leave +0x38 empty and no
      writer is known, which would make the early return unreachable; left to trace (BACKLOG line). `0x00EDC000` also drops
      a row whose on-screen name text comes back null (`0x00EDC298`) or whose `unit_class` / `category` is unknown; those are not modelled
      (BACKLOG line). Vanilla is unaffected (CONFIRMED, recruit-gates 2026-10-14,
      test `unit_resources_with_vanilla_data` reading `region_unit_resources` through the merged view, layout
      `ss`): its 37 rows hold all 33 distinct units #14 values and all 12 eur_napoleon `RESOURCES_ARRAY` values,
      so the exe drops no vanilla unit and stores no null resource. Own saves are format version 2 (they write
      the list); a version-1 save takes its regions' unit resources from the campaign's start
      (`own_save::read_with_start`, called by `source::open`) and errors without it (tests `a_version_1_save_*`).
      Vanilla `eur_napoleon` (test `unit_resources_with_vanilla_data`): every one of the 72 regions holds
      `global` (375 units need it), plus `horses` 11, `cossacks` 4, `balkans` 3, `ottoman_elite` 5 and one
      each of `hungary`, `norway`, `poland`, `scotland`, `silesia`, `switzerland`, `tyrol` (102 items). Units
      needing `spain`, `portugal`, `france`, `bedouin`, `guerilla_*`, `lombardy`, ... match no region there, so
      the gate now leaves them out of that campaign (before, any building allowing them offered them).
    - `0x00A27960` `CheckUnitNotScriptRestricted(this = model +0xFA8, pUnitRecord)` (CONFIRMED, traced 2026-10-13;
      ported as `World::restricted_units`, filled by the loader from `UNIT_RESTRICTIONS` and by the script
      calls, saved with the world; the old `ScriptState` / AI-hint copies are gone). False when the unit is in the list +0x24 (count +0x20) = the scripts' `UNIT_RESTRICTIONS`
      (`EPISODIC_RESTRICTIONS`, loaded by the ctor `0x009968A0`; `add_restricted_unit_record` /
      `remove_restricted_unit_record`). Its sibling +0x10/+0x14 is `BUILDING_RESTRICTIONS` (gate `0x009CDA90`,
      modelled as `World::restricted_buildings`): restricted units get the same home, `World::restricted_units`.
    - The restricted-unit list (unitrestrict, 2026-10-20, CONFIRMED). It is a list of UNIT_RECORD pointers, not a
      set. `AddRestrictedUnitRecord` `0x009C9700` (Lua binding `LUA_AddRestrictedUnitRecord` `0x00977500`, name
      string 0x01369B40) looks the key up in the units table and appends the record with no duplicate check, then
      rebuilds every faction's effect sums (`RebuildFactionEffectSum` per faction, which rebuilds the region
      recruitable lists). `RemoveRestrictedUnitRecord` `0x00A1D3B0` (binding `0x00977570`, string 0x01369B5C; also
      called by `ApplyMissionSuccessReward` `0x00A06050`, so a mission reward can lift a restriction) erases the
      first matching entry only and rebuilds only when it erased one. So a unit added twice stays restricted until
      removed twice. The ESF loader `0x009968A0` reads each `UNIT_RESTRICTIONS` item as one type-0x0E (UTF16) key
      and appends it in file order (at 0x00996D3B); the writer `0x009A1070` writes each record's key as type 0x0E.
      Ported: `World::restricted_units` is a `Vec` in list order with duplicates kept; `CampaignModel::
      add_restricted_unit` / `remove_restricted_unit` are the one rule the script command calls;
      `World::unit_restricted` is the one test (recruit list, so the panel, the AI and the recruit command).
      Own saves keep the list as written (a RON list either way, format unchanged).
    - ORIGINAL BUG (unitrestrict): a key the units table lacks makes both the add and the loader append a NULL
      record (the loader logs it; its building list skips such keys). The writer `0x009A1070` has no null test for
      units (its building list has one, at 0x009A1170), so the next save dereferences the NULL record and crashes.
      A NULL entry restricts nothing. Ours logs the key and stores nothing: the script command logs "not a valid
      unit key, ignored", the loader one warning listing the dropped keys. A remove of an unknown key does nothing
      in both.
    - Third caller of `0x00A27960`: `CampaignUI.BuildingEffects` (handler 0x009DE150, call at 0x009DF476) sets
      each recruitment row's `ScriptRestricted` field (string 0x0136C798) from it. Our BuildingEffects is still a
      stub; its rows must read `World::unit_restricted` when they are built.
    - `0x00B77F90` `CheckQueuedUnitStillRecruitable(item, faction, region, pFaction)` (CONFIRMED; called from
      `0x00AAFD40` in the queue step `0x00B71FB0`'s removal): permission mask (unit +0x94 & faction +0x30 → +0x44),
      the faction's unit set (hash at faction +0x1B0), technology, government, `0x00AA1660`, `0x00A27960`; no
      building flags. `0x00B43FC0` (a per-slot list, called by `0x00A799A0` for each slot with a standing
      building) runs the same gates plus a land-stats test and no `0x00A27960` call. The test is unit +0x9C ≠ 0:
      +0x9C is `UnitRecord.pLandUnitRecord`, the unit_stats_land row, zeroed by the UNIT_RECORD ctor `0x00E91320` and
      set only by the land record ctor `0x00E8CFF0` (CONFIRMED, polish20), so that list holds land units only.
      `0x00A799A0` itself also appends the global units with +0x9C ≠ 0 that pass the permission, technology and
      resource tests. `BuildSlotBuildingRecruitableList` `0x00B43CA0` uses the same field to pick each entry's queue
      (land: the region's recruitment manager; otherwise the port queue).
    - `0x00A6AE40` `RebuildRegionRecruitablesAndEffects` reads the slot container's +0x20 and +0x24 besides the
      slot array: CONFIRMED from `LoadRegionSlotManager` `0x00A4DCB0`, +0x20 = `ROAD_SLOT` and +0x24 =
      `FORTIFICATION_SLOT`, both ordinary 0x254-byte slots built by `0x00A4D4F0` (`ConstructRegionSlotFromEsf`; the REGION_SLOT record version, reader +0x2C, gates two fields: below 3 one extra bool is read and dropped, 2 or more reads +0x220). So the fortification's building
      is a recruitable source like the road: `recruitable_units` now reads the slots, then `road`, then
      `fortification` (test `the_walls_building_is_a_recruitable_source`). The same function also merges the two
      slots' effects into region +0x188 (local) / +0x198 (faction-wide) through the same `0x00A62380`, after the
      slot array, road then walls, each only when the slot is not held by another faction (`0x00A91FC0`) and
      its building is at health > 99 (recruit-followups 2026-10-14, CONFIRMED at `0x00A6AEE3` / `0x00A6AEFA`).
      Readers of +0x188: `BuildRegionEffectSet` `0x00A67530` (construction options `0x00B43880`, repair cost
      `0x00B66410`, public order `0x008EDF70`), our `economy::region_effect_set` and the effect sets.
      `Region::effect_buildings` now yields the slots, the road, then the walls (the walls' effects counted
      for the first time; before, INFERRED not counted). The road and walls slots have holders like any slot
      (same ctor `0x00A4D4F0`): the ESF loader reads `Region::road_holder` / `fortification_holder` from their
      `GARRISON_RESIDENCE` #0, and `slot_held` uses them for effects, the recruitable flag 0x10 (now also on
      the road and walls), construction progress and repair. Evidence: `eur_bavaria` in `auto_after_c8` and
      `orig_over_nr4_0252` (original-written) is Austria's with Bavaria holding its road, walls and four of
      five slots; in every region of those saves the road / walls holder is the owner or holds another of its
      slots (test `the_road_and_walls_holders_load_from_the_vanilla_saves`). An owner change hands the road and
      walls to the new owner (CONFIRMED, `0x00B1B300`, see "Slot holders"). Own saves are format version 3; an
      older one takes the holders from the campaign's start, kept while that faction still holds one of the
      region's slots (or, holding none, still owns it) (test `a_version_2_save_takes_its_road_and_walls_holders_from_the_start`).
      The ESF save writer now writes every slot's holder (array slots, road, walls) to `GARRISON_RESIDENCE` #0,
      the owner when `None` (test `slot_holders_are_written_where_the_loader_reads_them`); the loader maps the
      owner's id back to `None`.
    - Construction vs the queue step (CONFIRMED): construction (`0x00AB3DA0` → `0x00B78800` per slot, including
      +0x20 and +0x24; a completion rebuilds the cache through `0x00A682F0`) runs in the round-end economy
      `0x008BC650`, before the next faction's turn-start region update `0x00AAE820` → queue step `0x00B71FB0`.
      So the removal and the held-back check read the post-construction list. The model runs it in the round-end
      economy (`construction_round_end`, `TurnStep::Economy`), so it is done before the next turn start's
      region update (test `a_building_finished_this_turn_counts_for_the_queue_step`).
    - Tests: `restricted_units_and_units_without_their_region_resource_are_not_recruitable` (ntw_sim),
      `unit_resources_with_vanilla_data` (ntw_campaign, real install: the units #14 values and the regions'
      `RESOURCES_ARRAY`), `a_legacy_restricted_unit_list_loads_into_the_model` (own save: the old top-level
      list is merged into the world on read). unitrestrict: `a_unit_restricted_twice_stays_restricted_until_lifted_twice`
      (ntw_sim), `restricted_unit_records_are_the_models_list` (ntw_script host: the script command),
      `the_snapshot_leaves_out_a_script_restricted_unit` (ntw_ai: the AI's list is the model's recruit list, the
      one the settlement panel reads), `a_made_up_campaign_round_trips` (own save keeps order and duplicates),
      `the_loader_fills_the_restricted_building_levels` (ESF: order, duplicates, unknown key dropped).
    - Ghidra (unitrestrict, V5): `AddRestrictedUnitRecord` / `RemoveRestrictedUnitRecord` raw 77 each (effective 90;
      the rest is register-only locals), methods of the partial struct `CampaignScriptRestrictions` (+0x20 count, +0x24 data;
      one stack argument, the key, RET 4); `LUA_AddRestrictedUnitRecord` / `LUA_RemoveRestrictedUnitRecord` effective 89 each.
    - Ghidra (V5, `analyze_function_completeness` 2026-10-13, effective scores): `0x00AA1660` 86,
      `0x00A27960` 88, `0x00B77F90` 64, `0x009FB640` / `0x009FB660` / `0x00B57F40` 88 each. What is left is
      struct typing (UNIT_RECORD, region, faction, recruitment item as `void *`, no struct defined yet) and
      register-only locals, which the API cannot type. 2026-10-14: `0x00A6AE40` RebuildRegionRecruitablesAndEffects
      91 (was 14) and `0x00B43CA0` BuildSlotBuildingRecruitableList 77 (was 31; polish17: the register-only
      locals typed, `pRegion` = `CampaignRegion *` (not `RegionSlotManager *`: it is the BuildingSlot's
      vtable +0x34 result, whose +0x124 is the RecruitmentManager), `pSlotObject` = `BuildingSlot *`
      (`0x00B6D260`), `pEntryQueue` = `RecruitmentManager *`. Left: every retype splits the register-only
      locals into new generic SSA instances (structural), and the unit record, faction, owner object and
      out-list stay `void *`, no struct defined yet); RegionSlotManager +0x20 `pRoadSlot`,
      +0x24 `pFortificationSlot` (CONFIRMED, LoadRegionSlotManager `0x00A4DCB0`).
    - Struct sizes and typing (polish19, 2026-10-14, STRUCT_RESIZE_WORKFLOW; sizes CONFIRMED from the
      allocations): `CampaignRegion` 492 → 0x3B0 bytes (`push 0x3B0` before the ctor `0x00A45950` at
      `0x00A4CF12`), fields +0x270 unit resource count / +0x274 unit resource list (the ctor resolves
      `RESOURCES_ARRAY`); `BuildingSlot` 492 → 0x254 bytes (three `push 0x254` in `0x00A4DCB0` at
      `0x00A4DDB1` / `0x00A4DECC` / `0x00A4DF62`; the region-slot class built by `0x00A4D4F0`, whose `this`
      is now `BuildingSlot *`), fields +0x1D8 parent, +0x1E4 port object (0x78 bytes, `0x00A45860`), +0x20C
      the byte that gates the two port objects. New structs `UnitRecord` (0xFC, the row size of
      `0x00E9B700`: +0x38 price, +0x94 permission bits, +0x98 required unit resource) and
      `RecruitableEntry` (0x20, the entry `0x00B43CA0` builds). `0x00AA1660` now takes
      `(UnitRecord *, CampaignRegion *)`; in `0x00B43CA0` the temporary list, the lookup key and the entry
      unit are typed. The raw +0x124 read in `0x00B43CA0` stays: the register holds the hash-set node, then
      the region (structural). Effective scores after: `0x00AA1660` 100, `0x00B43CA0` 81, `0x00E9B700` 54
      (register temporaries named; the rest is unnamed callees), `0x00A4D4F0` 46 (sub-object vtables and the
      intrusive lists between +0xC and +0x1D4 are untyped bytes).
    - The commander pool around the queue (recruit-gates, 2026-10-13, Ghidra functions created):
      `0x009FB640` `CheckNavyHasShipForAdmiral` (= `0x00A278D0(force) != 0`, pool vtable `0x0136AEC4` slot +0x40),
      `0x009FB660` `CheckForceHasFreeUnitSlotForGeneral` (vfunc +0x8 count < vfunc +0x48 max, unsigned; vtable
      `0x0136AE7C`), `0x00B57F40` `HandleRecruitmentItemTargetCharacterDestroyed` (this = item +0x5C; clears item
      +0x18 and +0x24 when the destroyed character matches). Pool code: the panel counts the commanders in service
      and the maximum once (`commander_row_flags_with`; the exe recounts per row at `0x00A1BB7B`, same result),
      and `hire_admiral`'s refusal without a ship is labelled a crash guard: the exe passes the null ship from
      `0x00A278B0` on and reads its +0xF4 (`0x00A161A1` → `0x006649C0`); its interface never offers that hire.
  - `MergeRecruitableEntrySorted` `0x00B08E30` joins the entries of one unit from several buildings. An unflagged
    entry replaces a flagged one; an unflagged one stays; two flagged ones OR their flags. So a unit is unflagged
    when any building allowing it is. Between two unflagged entries, `0x00B780E0` keeps the lower experience,
    then the lower cost. The model's entries carry neither, so this does not apply to them.
  - The list is kept sorted by unit key (`0x00B78140` → `CompareUniStrings` `0x004F1F30`: case-sensitive, by UTF-16
    code unit; the UNIT_RECORD starts with its key). `recruitable_units` sorts the same way, and the card list
    shows that order.
  - A port's naval queue uses its port building's own list (`0x00B61DA0`). The model's single naval queue uses the
    region's list, which is the same for a region with one port.
- **The recruitable population (CONFIRMED, worker recruit-pop 2026-10-10; code `population::RecruitmentPopulation`,
  used by `recruitable_entry_flags`, `recruit`, `cancel_recruitment`, `region_turn` and `occupy`).** It is the live
  population, `REGION_FACTORS` #2 (region +0x7C, the model's `Region::population`, §Population); there is no separate
  recruitable pool. The variables are read by `0x008B25F0` (FISTP of the f32 at model +0xDA0 + 4 × index): var 36
  `minimum_population_after_recruitment`, var 37 `recruitment_population_cost` (key objects `0x0164B670` /
  `0x0164B678`, built at `0x00432FE1` / `0x00432FF0` from the strings `0x0139EFDC` / `0x0139F004`; var 35 at
  `0x0164B668` is `minimum_population`, which fixes the indexing). No unit field takes part: every entry of a
  region's land and naval lists gets the same answer.
  - Gate `HasRecruitmentPopulationAvailable` `0x00A89550` (this = entry[7] +0x54 → region +0x28, from
    `FlagUnavailableRecruitableEntries` `0x00B69BA0` for every entry): passes when var 37 + var 36 ≤ population
    (signed); failing sets entry flag 4, which the queue command refuses (`CommandError::NotEnoughPopulation`) and
    the card shows as its "population" reason (bit 2, the mapping INFERRED with the other bits).
  - Charge `0x00AAF190` (via thunks `0x00AAF1E0` region +0x28 → `0x00AAF180` +0x24, so +0x30 = region +0x7C): at
    queue time, by `QueueRecruitmentItemForUnit` `0x00B58DD0` right after the money (its only caller): subtract var
    37 when var 37 + var 36 ≤ population, else set it to var 36 (unreachable from the command, which refused the
    flagged entry first). The AI and the console queue through the same command.
  - Credit `0x00A61AA0` (via thunk `0x00A620A0`, region +0x28 +0x54): add var 37, called only by
    `CancelRecruitmentItem` `0x00B1A820`, unconditionally (whether or not the money is refunded), once per item. Its
    callers: the cancel command, the turn start's removal of items the region can no longer recruit (`0x00B71FB0`),
    and the queue clearers `0x00B1A760` (capture variants `0x00B58560` / `0x00B58890` / `0x00B58C00` / `0x00B58A30`, the region
    transfer `0x00A64AC0`) and `0x00A6CBF0` (one port's naval queue; callers `0x0094D4F0`, `0x00A92380`,
    `0x00A924D0`, `0x00A925C0`, `0x00A971B0`: the slot captures and the docking step, see "The other
    SetSettlementOwner callers"). A trained item is destroyed by the queue step
    without the cancel path (no credit), and disbanding a unit has no credit.
  - Signedness: only these three rules read the population signed. The growth (`0x00AB4227`, CVTDQ2PD with the
    2^32 fix-up) and the factors (`0x00AA9D44`, `0x00AA9F91`) read it unsigned, and the cap compares it unsigned
    (CMOVA `0x00AB42CB`). So a mod's negative variables that take it below 0 leave the exe with those 32 bits;
    the model's `u32` holds the same bits (test `a_population_charged_below_zero_keeps_the_exes_bits`). The growth
    now rounds with FISTP (`commands::fistp`, the indefinite out of range) instead of a saturating cast.
  - With the shipped data both variables are 0 in every campaign (`campaign_variables` and its per-campaign
    overrides), so nothing changes; checked by `recruiting_takes_no_population_in_the_vanilla_saves`: the 28 regions
    that queued new recruits between `auto_nr4_t4` and `orig_over_nr4_0252` (factors and owner unchanged) reach
    their grown population exactly. Synthetic tests: `recruiting_takes_population_and_cancelling_gives_it_back`,
    `the_recruitment_population_rules_are_signed_32_bit`.
  - The money side of those queue clearers (capture: ships refunded, land not; a transfer: everything refunded
    to an AI old owner, nothing to a human) is ported: §Capture, "Capture variants and the queues".
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
  `the_recruitment_rule_is_unsigned_only`,
  `the_recruitable_list_flags_damaged_and_occupied_buildings_and_missing_technologies` (ntw_sim);
  `the_loader_takes_the_campaign_cost_from_units_7`,
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
  3. In queue order, each item that is not blocked and has its flag +0x54 set takes one turn (item method +0x18).
     The loop stops once `capacity` items have. A blocked item takes no turn and uses no capacity, so the items
     behind it train instead. It is not removed or refunded.
     - Blocked (`0x00B5AD90`, CONFIRMED, worker recruit-cost 2026-10-10): the item's unit has no entry in the
       queue's **unpriced** list (vtable +0x14: land `0x00B61E10` = a copy of region +0x1A8; naval `0x00B61DA0` =
       the port building's list), or that entry has any flag.
     - Only the building flags 8 / 0x10 / 0x20 / 0x80 can be set there (§Recruitment cost and money). The cost,
       population, cap and full-queue flags are added only when the list is priced (vtable +0xC), so they never hold
       back an item that is already queued.
     - Ported in `region_turn` (test `a_flagged_item_is_held_back_without_taking_a_recruitment_point`). The step
       reads the list the turn start's removal read: the cache is not rebuilt inside `0x00B71FB0`.
     - CONFIRMED (recruit-gates 2026-10-13): that list already holds the buildings finished by construction
       (`0x00A78670` from `0x00AB3DA0`, in the round-end economy `0x008BC650`; a finished slot rebuilds the cache
       through `0x00A682F0` from `0x00B78800`), which runs before the turn-start region update. The model steps
       construction in the round-end economy (§Recruitment cost and money, "Construction vs the queue step").
     - `RebuildRegionRecruitablesAndEffects` `0x00A6AE40` also reads the settlement container's two extra slots:
       CONFIRMED +0x20 = the road, +0x24 = the walls (fortification); both are recruitable sources in the model.
  4. Finished items are trained.
  - Flag +0x54 is set on every queued item by `0x00A78620`, called from the faction's round-end economy
    `0x008BC650`. So an item counts down only after a round end has passed since it was queued.
  - The queue holds at most 10 items (`0x00B62040`).
  - Force 1 (`0x00B489B0` → `0x00B71FB0(1)`, from `0x008CDD60`) ignores the flag and repeats until the queue stops
    changing: an instant-complete path (cheat or script).
- **Construction** (`0x00A78670`, a virtual): `0x00B78800` for every slot of the settlement container (the slot list
  plus the two extra slots). Each slot steps its own construction item (item method +0xC) when the slot's holder is
  the region's owner (or the item type is 2 repair / 3 demolish; `StepSlotConstructionItem` `0x00B78800`), so
  items progress side by side. Order: the slot list in index order, then the road (+0x20), then the walls (+0x24).
  It does not run in the region update: CONFIRMED in the round-end economy `RunFactionRoundEndEconomy` `0x008BC650`
  → `StepRegionConstructionAtRoundEnd` `0x00AB3DA0`, per region of the faction's region list (faction +0x778 count /
  +0x77C data, loop at `0x008BC873`), not for the rebels (`0x008CEEF0`), then the queue flag `0x00A78620`, before
  diplomacy `0x00B29100` and research `0x008F3C40`. Model: `construction_round_end` in `TurnStep::Economy` (tests
  `a_repair_counts_down_in_a_slot_another_faction_holds`, `the_rebels_construction_does_not_count_down`), through the
  seam `construction.step` (`turn::construction_step`). Regions go in the list's order (`World::regions_in_list_order`
  filtered by owner; accessloss2 2026-10-20, CONFIRMED, see "Faction region list order"; test
  `construction_steps_the_faction_regions_in_list_order`): each region's step reads and writes only that region's
  slots, its school's research entry and its owner's records (completion events, the recruitable cache), so the
  order changes no result, only the order of the events (CONFIRMED by the callees below).
- **Item types** (CONFIRMED; vtable +4 the type, +0xC the step, `0x00B781A0` the shared count: done + 1, finished
  at done ≥ total):
  - 0 upgrade (loader `0x00AE7040`, step `0x00B78560`) and 1 new building (loader `0x00AE6EF0`, step `0x00B78230`): the level at
    full health, `BuildingCompleted`.
  - 2 repair (loader `0x00AE6FE0`; made by `0x00AE7010` with +0x18 = (100 − health) / turns in u32 and +0x10 = max(1,
    floor((100 − health) × 0.01 × level turns))): `0x00B78490` raises the health by +0x18 each step that does not
    finish, through `SetBuildingHealth` `0x00B452A0`, which posts `CampaignBuildingDamaged` while it is below 100;
    the last step sets 100 (no `BuildingCompleted`; for the walls it also restores the fortification's damage
    object, vtable +0xC0, not modelled). Model: `ConstructionKind::Repair` (test above).
  - 3 demolish (loader `0x00AE6EB0`; made by `0x00AE6ED0`: one turn, cost 0), queued by `QueueSlotDemolition` `0x00B286C0`
    (the `DemolishBuilding` command): it first cancels the slot's item with its refund, then tests `0x00B16540`
    (not the road, the slot held by the owner, a building standing); an item in the slot does not block it. Its
    step `0x00B781B0` stops the slot's school (`0x00AA4BF0`, research progress kept), sets slot +0x1EC (UNKNOWN),
    clears the slot (`0x00B146A0`) and posts an event through +0xBA0 (UNKNOWN, not modelled). Model:
    `demolish_building`, `ConstructionKind::Demolish` (test `buildings_and_forts_are_demolished`). The panel's
    standing-building row (`0x009C8320`) sets `dismantling` from `0x00B4D5A0` (the slot's item is type 3) and its
    turns from the item (total − done).
- **Item record** (CONFIRMED, `WriteConstructionItemToEsf` `0x00AFBA90` / `ReadConstructionItemFromEsf`
  `0x00B256C0`): #0 type, #1 active, #2 done, #3 total, #4 cost (bit for bit), then the type's tail (vtable +0):
  types 0 / 1 #5 the utf16 level key, type 2 #5 the u32 health per turn (`0x00B7A4A0`), type 3 nothing. Model: read by
  type (`world::read_construction`; an item that does not fit its type is skipped with
  `UnreadableConstructionItem`), written by type (`save::write_building_manager`). #1 is the active byte (+0x8):
  `0x00B78800` steps only an active item (CONFIRMED, `0x00B7880E`), every item the exe makes is active
  (`0x00AE6E80`), and what clears it is UNKNOWN (a BACKLOG trace line). Model: `ConstructionItem::active`, read from #1
  and written back (a kept source record gets it too), kept by own saves (format 4; older ones read active), and an
  inactive item does not step (`turn::construction_step`; test `an_inactive_construction_item_is_not_stepped`). Older saves of ours stored a repair as a build of
  the standing level; `CampaignModel::convert_legacy_repairs` turns it into a repair at load (own saves before format
  4, and the ESF load). Construction options (`BuildSlotConstructionOptionsWithCost` `0x00B43300`, `0x00B4333B` →
  `0x00A91FC0`): a slot another faction holds offers nothing (test `a_slot_another_faction_holds_offers_no_construction`).
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
  find them: they are not there. The spawn is elsewhere (corrected 2026-10-20: in the manager's +0x20 itself, `0x00B1E500` / `0x00B1D7E0`, §Recruitment spawn and reinforcements; `0x008EF790` only links reinforcements).
- **Placement and the garrison join were already ported — by 0-E, not by us.**
  `CampaignModel::spawn_recruited_unit` (`commands.rs`) implements phase 3 of `0x00B1E500` and the
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

### Recruitment spawn and reinforcements (recruitmarch, 2026-10-20; static trace, CONFIRMED unless tagged; code `turn.rs` `region_turn`, `commands.rs` `spawn_reinforcement` / `march_reinforcements`)

Correction to round 15: the spawn is not in the `0x008EF790` walker. The queue tick `0x00B71FB0`
creates the units through the queue manager's vtable +0x20: land manager vtable `0x0137CBA0`
(+8 `0x00B62010` = the settlement position, region +0xFC; +0x20 `0x00B1E500`), naval vtable
`0x0137CBE4` (+0x20 `0x00B1D7E0`). `0x008EF790` only links a new army's units into the target's
incoming-reinforcement list (below).

- **Land spawn `0x00B1E500`, three phases.**
  1. Per queued item whose target (+0x18) or +0x50 is set, the distinct (target, +0x50) pairs in queue
     order. A finished item (`0x00B5C110`) whose target leads no army (`0x009D3A40` = 0) is retargeted
     to the character of the holder at target +0x2B0 when that holder's vfunc +0x38 says so (vfunc +0x14
     with holder(+0x34)+0x74), else its +0x18 is cleared.
  2. Per pair: the spawn spot is the settlement position (manager +8), moved by a path query, to the
     target army's building (`0x00B6D760`, mover 9) when it has +0x9C, else to its commander
     (`0x00B6D8D0`, mover 6); both run the pathfinder `0x00B6E6E0` and trim the path (`0x00B77820` →
     `0x00B67A30` / `0x00B77900`, `0x00B77790`; a one-node path becomes empty). Query true with nodes:
     the spot is node 0 (INFERRED meaning); with none: a free spot near the settlement (`0x00B39FE0`).
     Then `0x00B1D3F0(target, +0x50, out, &spot)`: every finished item of that pair becomes a unit
     (land `0x00B258C0`, naval `0x00B25D20`) and the group ONE new army, `0x008D25F0(owner, units, spot,
     1, 0, 0, 0)` (a navy `0x008D2DA0` when the group has no land unit), each unit placed (`0x00A27990`,
     `0x008F9040`) and named (`0x008EDE30`). Query false: the pair's items fall to phase 3.
  3. Every other finished item with +0x48 = 0: +0x18 cleared, unit `0x00B258C0`, then the garrison /
     existing army / new army branch `0x008D25F0(owner, units, spot, 1, 0, 1, 1)` (the one
     `spawn_recruited_unit` ports).
  Each new unit posts the `UnitTrained` script event (vtable `0x0137D120`, name string `0x0135056C`,
  posted by `0x00E30700`): in the group spawn at `0x00B1D74F`, the naval spawn `0x00B1D7E0` at
  `0x00B1E44F` and phase 3 at `0x00B1F1C9`.
  Which unit the new army's colonel is attached to: `0x008D25F0` → `0x008D24B0` (commander pick
  `0x008B8E50` / `0x008ED2C0`) or `0x00870E60` (army constructor), not traced (PROVISIONAL in ours).
- **+0x50 (`RECRUITMENT_ITEM` #11)** is `QueueRecruitmentItemForUnit`'s third argument: 0 from the
  player's command (`HandleRecruitUnitCommand` pushes 0 at `0x00936C55`, and passes the command's
  commander as the target) and from the console (`0x00924FE2`) and one AI site (`0x00A9F350`); the AI site
  `0x00A9F12C` passes an AI object (`0x004CE450` result at `0x00A9F10F`) with target 0. So the vanilla
  saves' nonzero #11 are AI items, and the player's targeted items group by target alone. The AI's
  (0, handle) groups (an army raised for an AI task) are not modelled.
- **Hand-over to the march.** The tail of `0x00B71FB0` takes the (item, new army) pairs of +0x20 and,
  per new army of an item with +0x18 (one per army), calls `0x00B0A270` on the faction's
  `ARMY_REINFORCEMENT_MANAGER` (faction +0x52C, vtable `0x0137DAEC`, getter `0x00B01F80`) with (new
  army's commander, settlement region +0xFC, target). An entry with the same source is retargeted
  (`0x00B69670`); else a new 0x54-byte entry `0x00AE8B10(source, settlement, target, 1)`: +4 source,
  +8 target, +0xC settlement, +0x10 done; `0x008EF790` links the source army's units into the target's
  incoming list. The entry listens to both characters (`0x0137DA8C` / `0x0137DA98` → `0x00B57DA0`: either
  destroyed, the entry is dead; `0x00B57E50`).
- **The march, at the faction turn start.** `0x008F2620` calls `0x00B71FA0(faction +0x52C)` at
  `0x008F2B35`, after the regions (`0x008F2B14`) and before research availability; only on the
  non-rebel branch (the rebels' `0x008F2B47` skips it; "The rebels' turn start"). An earlier version of
  the turn-start list's item (g) called this "the diplomacy manager": it is the reinforcement manager. `0x00B71FA0` → `0x00B5C710` (character
  entries) and `0x00B5C7A0` (unit entries). Per character entry: valid (`0x00B5C830`: source and target
  set, the source's `0x009D0D00(target, 1)` UNKNOWN, done flag clear) → `0x00B5C4A0`, else deleted. `0x00B5C4A0`:
  `0x00968B00(movers, target, 1)` (`0x008BE360`, else the move-to-character order `0x009195A0`, a
  multi-turn order object `0x00905460` of 0x120 bytes resumed by `0x009653D0`; events 0xEF / 0xE8); true
  → `0x00967070`, source army +0x9C +0x14 = 1, entry done and deleted; false → kept for next turn.
- **Saved.** `ARMY_REINFORCEMENT_MANAGER`: writer `0x00AFB6C0` (from `0x00892A80`),
  `CHARACTER_REINFORCEMENTS_ARRAY` items by `0x00AFC060` (i32 source, i32 target, i32 settlement, bool
  done), `UNIT_REINFORCEMENTS_ARRAY` by `0x00AFDEC0`; reader `0x00AE62F0` (from `0x0087A190`) →
  `0x00AE8C10`; post-load `0x00B5B740` (from `0x008E07F0`) → `0x00B5C130` / `0x00B5C330`. Not modelled
  in our saves (an entry lives only from its region step to the same turn start's march in ours).
- **`RECRUITMENT_ITEM` #2** (settled): `WriteRecruitmentItemToEsf` `0x00B5C900` writes #0 the item, #1
  raw +0x14, #2 raw +0x18; `LoadRecruitmentItemFromEsf` `0x00AF3D00` reads #1 / #2 raw and registers the
  item under #0 (`0x0105AEC0`); the item's post-load resolver `0x00B5C2D0` (item vtable `0x0137CAE8`
  slot +0x20, called from `0x00B5C1B3` / `0x00B5C243`) maps +0x14 and +0x18 through the id table
  (`0x0085F890` → `0x0105AC60`; an unknown id gives 0) and, for a target, registers the item as his
  listener (`0x005301C0`). So #2 is the target character's object id, 0 for none (CONFIRMED). No vanilla
  evidence save holds a nonzero #2.

**Ours.** `region_turn` spawns the finished land items whose target leads an army first, one new army
per target (`spawn_reinforcement`: units, then the colonel's army at the settlement position, not
garrisoned, the colonel on `commander_unit`'s pick, an entry in `World::reinforcements`), then the rest
(`spawn_recruited_unit`). The faction's `FactionReady` step runs `march_reinforcements` before research
availability: an entry whose source or target leads no force is dropped; else the merge order's walk and
merge (`march_and_merge`, shared with `MergeForces`). PROVISIONAL: the spawn spot (no path query; the
exe's failed query → settlement spawn; node 0 / `0x00B39FE0`); the holder retarget of phase 1; the
colonel's unit; the march is one walk this turn, then the entry is dropped (the exe's order lasts until
it arrives, and a refused order retries next turn); the entries are not saved; naval targeted items
(`0x00B1D7E0`) and the AI's (0, handle) groups spawn as ordinary recruits.

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
  - 3 = requirements met but a required building level missing (`state::BUILDING_MISSING`).
  - 4 = not yet available.
- **Links** (techchain worker 2026-10-20, static trace, CONFIRMED; code `ntw_campaign::rules`, test
  `the_loader_links_technologies_as_the_exe`, real-data test `shipped_technology_links_match_the_exe_builders`).
  - Chain record `BuildBuildingChainRecord` `0x00E55C30`: +0xC = thread from the chain's group
    (`GetTechnologyThreadFromChainGroup` `0x00F21E60`, case-sensitive: industry 1, military 0, enlightenment 2, else 3);
    +0x10 = 0 when the thread is not 3, else −1; +0x14 = the row's #2 number; +0x18 = category enum (military 0,
    government 1, money 2, agriculture 3, happiness 4, research 5, else 6). Loaded as `BuildingRules::chain_thread` /
    `chain_class`, `BuildingChain::group`.
  - `LinkTechnologyRequirements` `0x00F21A30` (called from `0x00E2125A`): per technology, +0x5C = its building level
    (unknown key: logged, +0x5C null), +0x8C = chain +0xC (the thread, `TechRules::thread`), +0x90 / +0x94 = chain
    +0x10 / +0x14, +0x98 = level +0x10. Rows of `technology_required_building_levels_junctions`
    (`GetTechnologyRequiredBuildingLevelsJunctionsTable` `0x00E2F220`, row reader `0x00DD2620` "ss") with a known level
    go to list +0x78 / +0x7C / +0x80 (`TechRules::required_buildings`). Then each
    `technology_required_technology_junctions` row (`GetTechnologyToTechnologyJunctionsTable` `0x00E2F2E0`) with a known required tech: same pair (+0x90 / +0x94)
    → +0x60 (`TechRules::parent`), else appended to +0x70. Unknown keys are logged ("In table %S: '%S' is not a valid key
    for this table") and dropped; a junction row whose technology is no record is never matched (silent). Ours drops
    the same rows and logs each once at load (`rules::invalid_key`), the silent ones too.
  - **ORIGINAL BUG** (`0x00F21A30`): +0x60 is overwritten per same-pair requirement, so only the last one is ever
    checked by `0x008F91F0`; ours checks every requirement (`TechRules::requires`), test
    `every_requirement_is_checked_even_two_of_the_same_tree`. Vanilla has at most one per technology, so it gives the
    same states.
  - **ORIGINAL BUG** (`0x00F21A30` / `0x008F91F0`): a technology with an unknown level keeps +0x5C null, and the
    availability walk dereferences it unconditionally (its chain record): a crash. Ours never opens it.
  - Shipped data: `technology_required_building_levels_junctions` has 0 rows (state 3 never occurs in vanilla); every
    vanilla thread equals the old key-prefix rule.
  - `LinkUnitRequiredTechnologies` `0x00EA9B10` (unittechs worker 2026-10-20, static trace, CONFIRMED; called once from
    the DB loader at `0x00E212A6`; this = the units table `0x00E31890`, args the junction table `0x00E31250` and the
    technologies table `0x00E2EFB0`). At load, for each built unit in table order, each
    `unit_required_technology_junctions` row in table order whose unit (row +0) equals the unit's key exactly
    (`0x004F0F40` → `CompareUniStrings`, case-sensitive) has its technology (row +0xC) looked up in the technologies key
    index (`0x0047AA40`). Unknown: logged ("In table %S: '%S' is not a valid key for this table", `0x01359008`) and the
    row dropped. Known: appended to unit +0xD0 (count +0xCC, capacity +0xC8), repeats kept. A row naming no built unit
    (including a unit the units builder dropped, `0x00EDC000`) matches nothing: dropped, no log. The drop is the
    exe's deliberate bad-key handling, not a bug, so ours is 1:1: `rules_from_db` drops the same rows and logs each
    once (the unit case too), test `the_loader_drops_unit_technology_rows_with_bad_keys`. Before, ours kept an unknown
    technology and so never let the unit be recruited; the exe recruits it with no such requirement. Shipped data:
    all 46 rows link (real-data test `unit_technology_rows_from_the_real_data`).
- **Availability** (CONFIRMED, `UpdateFactionTechnologyAvailability` `0x008F91F0`; code `update_tech_availability`,
  tests `availability_matches_the_chain_pair_at_the_level_or_higher`,
  `a_missing_required_building_level_holds_state_3_until_it_is_built`). For each tech in state 4 or 3:
  - its parent (+0x60) must be absent or researched;
  - the faction must own a building of the pair (`FactionOwnsTechTreeBuildingAtLevel` `0x008B13E0`): any slot of an
    owned region (region +0x120 slot list; any health, any holder; not the road) whose building's chain has the tech
    level's chain +0x10 / +0x14, at that level index or higher. So any group-less chain with the same #2 number
    serves (e.g. a Spanish variant), and a chain of another group does not;
  - every tech of +0x70 must be researched (else the entry is left as it is).

  Then every level of +0x80 must be owned by exact chain at that level or higher (`FactionOwnsBuildingLevelOrHigher`
  `0x008B13C0` → `FactionOwnsBuildingOfChainAtLevel` `0x008B1330`): state 2, else 3. The writer never sets 4, so a
  tech never goes back (Bavaria, Italy and Oldenburg keep state 2 after losing the building).
  - On state 2 the exe posts {technology manager +0x14 (its faction), entry} through `(game +8) +0x4E0` vtable slot +8: not identified (PROVISIONAL in
    `update_tech_availability`).
  - Callers: research step `0x008DD450`, grant `0x008CDCB0`, load `0x008CB8F0`, and `0x008CDD60` after its
    per-region pass. The model runs it at the faction's turn start (after its regions) and after each research step.
  - The rule reproduces every state of the 4 shipped start positions (test
    `tech_availability_rule_keeps_every_start_position_state`) and changes nothing in 11 vanilla saves
    (`ECON_TECH=update`, 0 changed each).
- **Grant** (`GrantFactionTechnology` `0x008CDCB0`, CONFIRMED; code `grant_technology`, test
  `a_grant_walks_the_parents_and_always_updates_availability`): while the tech is at state 1..4 it is completed
  (`0x008EED20`) and the walk follows its parent (+0x60) until there is none; then always
  `RebuildResearchedTechnologyEffects` `0x008E49C0` (clears the manager's set +0x18, then sums every state-0 record's effects +0xAC into it), the
  availability update, and the faction effect sum unless the second argument is set.
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
  - The thread effect is `research_points_military` / `_industry` / `_enlightenment` for thread 0 / 1 / 2, none for 3.
    The thread is technology record +0x8C, copied from its level's chain +0xC (the chain group, see Links; CONFIRMED,
    test `the_research_rate_reads_the_technology_thread`).
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
- **Gates** (0 failures over the queued items of the original saves):
  - `building_level_required_technology_junctions` (`building_tech_ok`, used by `can_build`; CONFIRMED:
    `LinkBuildingLevelRequiredTechnologies` `0x00DDF300` links each row with a known technology into level record
    +0x114 / +0x118 / +0x11C, unknown dropped; `BuildSlotConstructionOptionsWithCost` `0x00B43300` flags the option 8
    when any of them is not at state 0, and the construct command drops it): e.g. sAdmin3_court_justice needs
    admin1_public_schooling, so Paris cannot upgrade its magistrate at the start.
  - `unit_required_technology_junctions` (`unit_tech_ok`, used by `recruitable_units`; CONFIRMED `0x008AAB60`, links `0x00EA9B10`): e.g. rocket troops need
    military5_rockets.
  - `nr1`–`nr3` are saves written by our own port (0 public-order classes), not evidence; `nr3` holds a France
    sAdmin3 queued without the tech.
- **A building level's technologies** (techswitch worker 2026-10-20, static trace, CONFIRMED; code
  `CampaignModel::building_level_technologies` / `building_info_technologies` / `effects_table_technologies`, test
  `building_level_technologies_follow_the_tree_the_faction_list_and_the_switches`).
  - `0x008C7620` returns nothing for a faction without a technology manager (faction +0x7C4), else `0x008C7640` walks
    the manager's nested tree (+0xC/+0x10 stride 0x10 → +8/+0xC stride 0x20 → +0x18/+0x1C stride 0x14 → +0xC/+0x10
    node pointers) and keeps each technology whose building level (record +0x5C) is the given level itself (not
    "that level or higher") and that `0x00F1EFA0` allows for the faction's record (faction +0x514 via `0x008B9DE0`;
    the `britain` record when that is null). Any state counts. The `techs[]` saver `0x00894430` walks the same tree
    in the same order, so the list follows `FactionDetails::technologies`. `0x008C7620` has a third caller at
    `0x00AB8409`, code with no function defined yet next to the AI analyser `0x00AB8BC0`; it adds a score based on the
    building's technology list and is not gated by the technology switch (+0x118), so the AI port must call
    `building_level_technologies`, not `building_info_technologies`.
  - `0x00F1EFA0`: the technology record's faction set (+0xC0..+0xD0, count +0xD0) empty → allowed, else membership.
    The set is `technology_faction_junctions`: the technologies table load `0x00E2EFB0` gives the row builder
    `0x00F20F00` that junction table, and it inserts each matching row's faction into a hash set (once each). Loaded
    as `TechRules::factions`. The tree load `0x008CB8F0` applies the same filter (`0x008CBCC2`): every technology of
    every eur start-position tree passes its list (real-install test `technology_faction_lists_from_the_real_data`,
    1887 entries).
  - Readers: `CampaignUI.BuildingEffects` (handler `0x009DE150`, not a Ghidra function) builds the building info
    `0x009B7300` twice (`0x009DE598`, `0x009DE656`), which skips this list while the script switch +0x118
    (`World::technology_research_disabled`) is set (`0x009B73AF`) and still builds its other parts (`0x009B73F0`,
    `0x009B74E0`). The building effects table `0x009B2BB0` ("generic", "technology", "construction", "recruitment",
    "FactionColour", "UniformColour") fills "technology" (rows `0x009ABB50`, not traced) only when its switch
    argument is 0, the settlement has an owner faction and the campaign key does not start with `episodic`
    (`0x009B2FA1`..`0x009B2FE0`, prefix test `0x004F4480` → `0x004F1FE0`, an exact UTF-16 compare, so case-sensitive;
    `CampaignFeatures::effects_technologies_hidden`, no Napoleon key has it). `CampaignUI.FortEffects` `0x009E4AA0`
    passes the live +0x118; the building-completed messages (posted by `0x009CDA30` from the new-building and
    upgrade construction steps `0x00B78230` / `0x00B78560`, and by `0x009FB920` from `0x00B79970`; constructors
    `0x00985A70` / `0x00985CB0` store +0x118 at message +0xBC; vtable `0x0136FB34`) pass the posted copy: their
    details builder `0x00A096C0` reads +0xBC at `0x00A09D93` for the call at `0x00A09DD1`. These four are the only
    readers of +0x118 (every `+0xFA8` load scanned); no research, AI or tech-screen code reads it.
  - Not built yet: the construction-complete message (ours posts none; it must carry the +0xBC copy) and the Lua
    effect tables themselves (`ntw_script` ui/campaign/settlement.rs, PROVISIONAL empty), which must call these
    queries.
- **Open.** The AI's research choice (see AI_RESEARCH.md §7 item 8); the UI's tech availability bit in the construction
  panel (0-E); the save writer must write `techs[]` #1 / #2 / #3 (save-compat). `0x008DD450`'s callers
  (`0x008CDD40` / `0x008CDD50` / `0x008F3C40`) are not read.

## Treasury changes (worker treasury-rules 2026-10-10, static trace, CONFIRMED unless tagged; code `treasury.rs`)
- **Storage and the two movers.** The treasury is economics (faction +0xAC) +0x3F4, an i32. The charge
  `ChargeFactionTreasuryAmount` `0x00BAF500(amount, category)` is a plain wrapping `-=`, the credit
  `CreditFactionTreasuryAmount` `0x00BB3810(amount, category)` a plain wrapping `+=`; the category only feeds the
  spending/income statistics (its converters are identity). Neither tests the balance: every money test is the
  caller's. The model's one path: `treasury::pay` / `credit` (saturating, below), `script_credit`, `settle_round`; the
  bankruptcy reset to 0 is the one plain store.
- **Round end.** The cannot-pay flag `0x00BBC7D0` is `treasury + income < expenses`, 32-bit wrapping sum, signed
  compare; the settle `0x00BABE30` adds `income − expenses` (wrapping) at `0x00BAC0B3` when the flag is clear.
  Income `0x00BBE970` and expenses `0x00BBE910` are plain int sums of the economics lines. So a treasury near
  `i32::MAX` wraps negative in the flag test and the faction is flagged bankrupt.
- **ORIGINAL BUG: the 32-bit wrap (decided polish11, 2026-10-11, disassembly).** The movers wrap (`0x00BAF500` SUB,
  `0x00BB3810` ADD), the flag adds treasury + income at `0x00BBC817` and compares signed (SETL `0x00BBC81B`), the settle
  adds at `0x00BAC0B3`, none with an overflow check. The exe's own intent contradicts the wrap: when the flag is set,
  `0x00BBC7D0` passes the shortfall expenses − income − treasury (`0x00BBC85F`) to `0x00B0E580`, which cuts regular
  payments to cover a deficit, so the flag means "cannot cover expenses"; a rich treasury that wraps is flagged, its
  treasury zeroed and its payments cut. This is a 32-bit limit under CLAUDE.md "Better where the original is broken".
  Ours keeps the i32 treasury (saves unchanged) but `pay` / `credit` / the cancel refund saturate, `cannot_pay` sums
  in 64 bits and `settle_round` clamps to i32. Identical for every reachable value (vanilla treasuries stay far below
  2^31); tests `paying_saturates_where_the_exe_wraps`, `the_round_settles_without_overflow`. The income and expense
  sums (`0x00BBE970` / `0x00BBE910`, treaty money included) still wrap 1:1: each line is far below the limit.
- **Script `treasury_mod`** (`0x0097BAD0`): the amount is a Lua number read as f32, rounded by `0x010556D0` (FISTP,
  half to even); `0x00BB3810(amount, 3)` only when the faction is found and the rounded amount is > 0. A negative or
  zero change is ignored.
- **Capture money** (`0x00B541E0`): an option with money (+0x10 ≠ 0) credits it at `0x00B5431C` (category 0).
- **Unit pool.** The hire `0x00A1B8F0` charges `0x00BAF500(cost, 2)` only when the faction is human (+0x6E0); an AI
  hires free, and neither side is money-tested there. The panel's row flags `0x00A1BB20`: bit 1 = treasury < cost
  (signed), bit 0 = the faction's generals ≥ its maximum, bit 2 = no room; `IsRecruitable` is flags == 0
  (`0x009DD3AF`). **Bits 0 and 2 (treasury2, 2026-10-12, static trace, CONFIRMED; code `pool::commander_row_flags`):**
  bit 0 compares the commanders in service (`0x009D52A0`, the faction's characters of the pool's kind, candidates
  left out) with the maximum `0x009D1E00`: agents #11 (`+0x1A8`, vanilla General 4, admiral 3) plus the faction's
  `cap` bonus (bonus type 0, id 2; the agent key goes through `effects::agent_bonus_key`), rounded half to even
  (FISTP), then held to agents #10 (`+0x1A4`, the ceiling, 12 for the other agents, 0 = none) when it is nonzero.
  **ORIGINAL BUG:** a negative sum (a cap bonus below −#11) is compared unsigned at `0x009D1E21` and `0x00A1BB87`, so a
  faction with a negative maximum may hire without limit; ours clamps the maximum to 0 (test
  `the_most_commanders_a_faction_may_have_add_the_cap_bonus_up_to_the_ceiling`). Bit 2: a General needs a free unit
  slot in the force (`0x009FB660`); an admiral needs a ship with no character or a non-admiral one (`0x009FB640` →
  `0x00A278D0`). `hire_admiral` boards that ship (`pool::admiral_ship`, `0x00A278B0`); a captain it displaces dies
  (`character_dies`) and command is re-picked. Test `a_pool_row_flags_the_maximum_the_purse_and_the_room`.
  **The `CanRecruitCommander` gate** `0x009D1CD0` tests no money, turn or pool: false for a faction without a record
  (the rebels), else true when the force's commander (force +0x74) has byte +0x1B7 clear. That byte is the
  commander's "moving" flag: set at `0x00A28B50` when a move starts, cleared at `0x00A285E0` (argument 1) when it
  ends; the model's moves are instant, so it is always clear between commands and the gate is "commander present and
  not rebel" (`pool::can_recruit_commander`, test `the_interface_gate_for_hiring_a_commander_refuses_only_the_rebels`).
- **Promotion** in the field (`0x008E1C20`) charges every faction, human or AI, with no money test (the value it
  charges stays PROVISIONAL, §Unit pool in the code).
- **State gift** (deal component `0x00C4B440`, vtable entry `0x01381ECC`): charges the giver `0x00BAF500(amount, 3)`
  (a negative amount charges the other side |amount|), then `0x00B44590` raises the receiver's attitude. **The
  receiver is credited nothing**: the money leaves the game. No money test in the exe; the original Lua
  `InitialiseStateGift` (`diplomacy_panel.luac`) greys the values above `MaxPlayerPaymentAllowed`. The model refuses
  an amount ≤ 0 (the panel offers only `state_gift_values`).
- **Deal payment** (component `0x00C18A70`, vtable entry `0x01381F18`; amount +0x14, turns +0x18): turns == 1 is the
  one-off payment, payer charged `0x00BAF500(amount, 3)` and payee credited `0x00BB3810(amount, 1)` (a negative
  amount runs the other way); any other length adds a regular payment through `0x00B54FC0` (on the payer's
  relationship record towards the payee, via `0x00B55010(amount, turns)`; CONFIRMED by polish11's V5 pass). When both sides are human the exe also calls
  `0x008F56C0` on each with ±amount: FACTION +0x954 += n, a multiplayer results counter (CONFIRMED treasury2,
  2026-10-12). Its second caller (treasury3, 2026-10-14, CONFIRMED by decompilation) is `0x00B298F0` (from
  `0x00B29170` at `0x00B2971F`), a regular payment's per-turn tick: when both sides are human (+0x6E0 of each) it adds
  ±amount to the same counter, then lowers the turns left (+0x14) by 1; the money itself moves in the economy lines.
  Its one reader is the getter `0x008D9DB0`, which feeds the achievement / stat tracker
  `0x009C7660` (campaign +0xD9C, vfunc +0x50 = `0x004AFD40`; polish57: stat hooks, see "Allies called into a war") when a human faction of an MP campaign (+0x1034) is destroyed
  (`0x008AB6C0`) or the campaign ends (`0x0096CD10`); no rule reads it. It is saved as the FACTION i32 written at
  `0x00893E8B` (after +0x94C and the +0x950 bool), which our save keeps as loaded (template tree), so it is not
  modelled until our own multiplayer has a results report (BACKLOG line proposed). Model: the deal's payment
  record `negotiation::DealPayment` (amount, turns), applied by `accept_deal` as `treaties::one_off_payment` (turns 1)
  or a regular payment (other turns). The applier has no money test and no failure (`0x00BAF500` charges as is), so
  the model's only payment error is its own faction-pair check (a negotiation of a faction with itself); `accept_deal`
  makes it before any record is applied, so a refused deal applies nothing (deals-fix2, test
  `a_deal_with_a_failing_payment_applies_nothing`).
  **Sign and turns (CONFIRMED, deals-fix, 2026-10-14, disassembly `0x009BF64C`–`0x009BF701`; corrects polish11,
  which had offer and demand swapped):** the script `Propose` handler `0x009BF3C0`, action 9 (payments), is called
  `Propose(offer, amount, turns, regular, action)` and reads the action, the regular flag (bool reader `0x01055840`,
  Lua type 1 or a type error), the turns and the amount (number reader `0x010556D0` → `0x01055700`: Lua type 3, read
  as a float and rounded to the nearest, ties to even, by the x87 default), then the offer flag. It writes the
  deal's one payment pair (+0x08 amount, +0x0C turns): an offer through `0x00C43D70` (turns) / `0x00C43D90` (turns 1),
  which negate the amount; a demand through `0x00C1F6D0` / `0x00C1F6F0`, which store it. Turns are the panel's when
  the regular flag is set and the turns are above 1 (signed), else 1. So the record's amount is **negative for an
  offer** (the proposer pays) and **positive for a demand** (the recipient pays); the applier `0x00C18A70` charges
  the `0x00C4BC20` side (the proposer) for a negative amount and the `0x00C4CE90` side for a positive one. The panel
  note that the schedule was one of the deal's two optional items (+0x20 / +0x24) was wrong.
  **Deal → record copy (CONFIRMED, polish13, 2026-10-12, disassembly):** the payment record's vtable starts at
  `0x01381EDC` (constructor `0x00BF3C90` at `0x00BF3CA5`, amount and turns zeroed); slot +0x44 (`0x01381F20` →
  `0x00C4AB20`) stores the pair's amount to +0x14 at `0x00C4AB38` and the turns to +0x18 at `0x00C4AB42`; slot 0
  `0x00C11CA0` writes them back. **Payment cap (CONFIRMED, deals-fix, 2026-10-14, disassembly; the 2026-10-12
  reading called it a demand cap):** the same function (`0x00C4AB4E`–`0x00C4AB82`) caps an **offered** one-off
  payment: when the turns are 1 and the amount is negative it reads the treasury (`GetFactionEconomyTreasury`
  `0x00BCAFE0`) of the record's `0x00C4BC20` side, the proposer, and when the offer is above it (signed) stores minus
  the treasury (`0x00C4AB82`). Demands and regular payments are not capped (the panel limits a demand itself:
  `diplomacy_panel.luac` `CheckMaxPayment` line 1498 / `SetMaxPayment` line 1521, against
  `MaxOppositionPaymentAllowed`). Then (`0x00C4AB85`) it builds the row text from the capped record: turns 1
  `diplomacy_strings_string_offer_or_demand_single_payment` ("Single payment: %d"), else `…_regular_payment`
  ("Regular payment: %d. Turns: %d"), both with |amount|. **When:** the record copies the deal through
  `ApplyDealToNegotiationRecord` `0x00C49AA0`, from the propose commands (`0x00933CC0`, `0x009340A0`), the AI's
  evaluation (`EvaluateProposedNegotiationDealAI 0x00AA5ED0` → `0x00CC58C0`) and `0x00C1E240`; so the cap uses the proposer's treasury when the payment
  is proposed, and the panel row, the AI's value and the applied deal all read the capped record. The slot-0x14 offer
  text getter `0x00C123F0` returns the text for a negative amount, the slot-0x4 demand text getter `0x00C11F20` for a
  positive one; slot +0x2C `0x00C12640` reports items while the amount is not 0; slot +0x4C `0x00C5CBC0`
  (`RemoveAction`) posts the propose command (descriptor `0x013879FC`, via `0x00C4AD60`) with the clear flag and zeros.
  **ORIGINAL BUG (`0x00C4AB7E`–`0x00C4AB82`):** with a negative treasury the stored amount, minus the treasury, is
  positive, so the offer turns into a demand on the recipient, against the cap's own intent ("never more than the
  proposer has"). Ours: `treaties::proposed_payment` caps a one-off offer at max(treasury, 0), so an indebted
  proposer offers 0 and nothing moves (the applier skips 0). It is the rule seam `diplomacy.proposed_payment`
  (`CampaignModel::proposed_payment`, renamed from `diplomacy.demanded_payment`), called once by
  `CampaignModel::propose_payment` (command `ProposePayment`) when the record is set; the UI's `Propose` /
  `RemoveAction` post that command and the row text reads the record (tests
  `an_offered_one_off_payment_is_capped_at_the_proposers_treasury`,
  `a_proposed_payment_is_capped_when_proposed_and_paid_on_accept`,
  `a_proposed_payment_is_capped_once_and_its_row_shows_the_capped_amount`). The AI's value of the record:
  AI_RESEARCH.md §Deal evaluation, record 7. Ghidra names (deals-fix): `CopyDealPaymentAndBuildRowText` (`0x00C4AB20`),
  `PostProposeDealPaymentCommand` (`0x00C4AD60`), descriptor `g_pfnProposePaymentsCommandPost` (`0x013879FC`).
- **Regular payments and protectorate tribute** are booked into the running record (categories 8 / 10 / 22 / 24,
  "Running record" above) and paid by the round settle with the rest of it.
- **The other charge / credit callers (treasury2, 2026-10-12, static trace, CONFIRMED unless tagged).** Every xref
  of `0x00BAF500`, `0x00BB3810` and the refund mover `0x00BAF660` is now mapped:
  - `0x008EEB70` is the **ship repair in port** order (treasury3, 2026-10-14; it was read as a unit retraining):
    charges `0x008EA2F0`, category 2; cancelling refunds through `0x00BAF660`. Modelled, see §Ship repair in port.
  - `0x008E2260` naval promotion: already mapped (§Promotion), pays 0.
  - `0x00B4E4C0` map-fort repair (from `0x00AA49A0` ← `0x00CE9C40`; gates `0x00B4D500`, `0x00B16460`, `0x00B4D3F0`):
    charges round(level cost × (1 − health)), category 1. Not modelled: our map forts have no health, and no vanilla
    startpos or save holds a fort (the fort UI calls are PLACEHOLDER no-ops, `settlement.rs`). Its cancel
    `0x00B1A940` (CCQ_CANCEL_REPAIR_FORT, executor `0x00932270`) refunds item +4,
    category 1.
  - `0x00B1AAB0` CCQ_CANCEL_UPGRADE_FORT (via `0x008B3680`, executor `0x00932360`): refunds
    (int)(rec +0x24 / rec +0x20 × (rec +0x20 − item +0xC)), through the mover (category 1) when item +0xC is 0, else
    as a credit (category 3). Unreachable in vanilla: the fort upgrade is constant-false.
  - `0x00B0A300` CCQ_REPLENISH_UNIT (`0x00937080`) / CCQ_TOGGLE_REPLENISH_UNIT (`0x009387F0`): dead, its first test is
    `GetConstantFalseStdcallOneArg` (the cost would be `0x00B64920`, category 2); its cancel `0x00B1A9F0` (from the
    merge `0x008D93D0`) is never reached either.
  - `0x00A06050` mission rewards (caller `0x00A26600`): money (+0x64) credited, category 3; also the takeover
    (+0x68 → `0x008AB6C0`) and the unit, agent, experience and recruitment grants. Belongs to the missions item
    (BACKLOG §0 missions line). **treasury4 (2026-10-14): partial, not modelled.** The model has no mission
    completion, so nothing would call an applier; the credit itself is `treasury::credit` (category 3) when the
    missions item models `0x00A06050` whole (BACKLOG line).
  - `0x009DB270` / `0x009DB7C0` post-battle state 6: a unit that cannot be placed is credited (category 2) with
    `0x008E1BC0` (CalculateNavalUnitSaleValue, which only computes; `0x009DB7C0` credits) = trunc((1 − 0.25 × the sum of the four floats unit +0x130..+0x13C) × unit record +0x38). The four
    floats are the ship's damage d0..d3 (`SHIP_DAMAGE_INFO` #0..#3; treasury3, CONFIRMED: the repair share
    `0x008F6590` and the repair `0x00576890` read and zero the same words), so state 6 is a captured ship that finds no
    free spot (`0x00B38F10`) and is sold at its undamaged share of the `units` #7 price. Not modelled: ours puts a
    captured ship into the captor's force whatever its size (BACKLOG line proposed). **treasury4 (2026-10-14):
    still not modelled.** The sale is one branch of the state-6 force rebuild (`0x008EB330` → `0x008EB370` →
    `0x009DB270` per side, §Battles "After the battle"): a unit whose force is gone gets a new force on a free spot
    within 3 map units of the previous entry (`0x00B38F10`); only a ship that finds none is sold. The model has no
    such rebuild (its units always belong to a force, and a captured ship joins the captor's force), no
    commanderless navy and no free-spot search, so the sale needs that rebuild first (BACKLOG line).
  - `0x009318B3` in the undefined function `0x00931860`, CCQ_ADJUST_FACTION_TREASURY (registered at `0x0041F200`):
    category 3, sent only by the dev console's `adjust_treasury` (`0x008A9770`). Not modelled (a developer command).
  - `0x00BB33A0` from `0x00BAF200`, a campaign-director element (vtable `0x013800AC`, built by `0x00B930D0` /
    `0x00BAC310`, cdir config 0x28–0x2B): credits its +0x10C, category 4. **Modelled (treasury4, 2026-10-14, CONFIRMED):** the
    boost desire's step `0x00BAC310` builds one such intention per chosen faction (`0x00B930D0(weight 0.0, CAI faction, amount)`);
    its action `0x00BAF200` → `0x00BB33A0` credits the amount (`0x00BB3810(amount, 4)`), and the intention loop
    `0x00BBC880` runs it right after the step, so `director::boost_step` credits at once (same treasury changes,
    same order; test `the_boost_desire_pays_the_highest_rivals`).
  - **The MP results counter** (FACTION +0x954, `0x008F56C0` / `0x00B298F0`, above under Deal payment) moves no
    treasury; treasury4 found no other writer or rule reader (readers: the getter `0x008D9DB0` for the MP results
    listener, from `0x0096CD10` and the takeover `0x008AB6C0`). Not modelled until our multiplayer has a results
    report (BACKLOG line).
- **A queued item whose commander dies** (BACKLOG §0-B). The character destructor `0x0099D2D0` notifies its observer
  list (+0xD4); a recruitment item queued through a commander observes him, and its listener `0x00B57F40` clears
  the item's target (+0x18) and the bool +0x24. The item stays in its queue, keeps its cost and turns, and is not
  refunded; when it finishes, `0x00B71FB0` hands it to the reinforcements only while +0x18 is set, so it spawns in
  its settlement like an untargeted item. Model: `characters::drop_recruitment_target`, called from
  `character_dies` and from the admiral hire's captain removal. **The +0x24 bool (treasury2, 2026-10-12,
  CONFIRMED):** it is RECRUITMENT_ITEM field #5 (`WriteRecruitmentItemToEsf` `0x00B5C900`, written at `0x00B5CA99`,
  read back at `0x00AF3E66`), set to 1 by the constructor (`0x00AF4019`) and cleared only by the listener
  (`0x00B57F40`, reached through the thunks `0x00B57F29` / `0x00B57F31` with ECX = item +0x5C); nothing else reads it,
  so it has no gameplay effect. Modelled as `RecruitmentItem::target_lost` (saved as `!target_lost`), so a save
  written after a commander's death keeps the original's value (test in `ntw_campaign` save round-trip).
- **Plate corrections (polish11 V5 pass, 2026-10-11, disassembly).** Applied in Ghidra on 2026-10-12 with the
  V5 write-up of the 11 treasury functions. Completeness: minimum 78 and average 84 (effective: minimum 86,
  average 93). The corrections:
  `0x009D1CD0` CanForceRecruitCommander takes one argument, the force (stdcall, RET 4). The "is_navy"
  argument on its plate does not exist. `0x00BBC7D0` returns void: it stores the cannot-pay flag at
  faction +0x464 and, when the flag is set, cuts the regular payments (`0x00B0E580`), so its old name
  `IsFactionUnableToPayExpenses` was wrong (now `ComputeFactionCannotPayFlag`). In `0x00BAF500` the decompiler shows the amount as the
  bucket index, but the disassembly at `0x00BAF5F2` indexes by the category. `0x008F56C0` does
  faction +0x954 += n. `0x00B54FC0` books the payment on the payer's relationship record toward the
  payee, which is CONFIRMED.

## Ship repair in port (treasury3, 2026-10-14, static trace, CONFIRMED unless tagged; code `ship_repair.rs`)

- **The flag:** `NAVAL_UNIT` #3 (ESF `NAVAL_UNIT` v1 = {record key, `UNIT`, u32, bool #3, bool #4,
  `SHIP_DAMAGE_INFO`}) is unit +0x168, "ordered to repair" (written by the saver `0x00895640`). Model:
  `ShipState::repairing`, loaded by `read_ship_states`, hashed in the world hash.
- **Ship memory (naval unit, vtable `0x0135552C`):** `SHIP_DAMAGE_INFO` at +0x130: damage f32 ×5 (+0x130..+0x140),
  crews i32 ×3 (+0x144), max crews (+0x150), guns (+0x15C), sunk (+0x160), max guns (+0x164). The ESF writes the
  three crews in the reverse order of memory; the sums below do not care, a per-index use would (BACKLOG check line).
  Record at +0x48 (`units` #7 price at record +0x38), force link +0x11C. Vtable: +0x38 `0x008C92F0` = in a force
  (+0x11C non-null), +0x40 `GetConstantFalseCdecl`, +0x4C `0x008E2260` (promotion).
- **Needs a repair** `0x0055ACC0`: any crew below its maximum, else any of damage d0..d3 above 0 (d4 not tested).
- **Repair share** `0x008F6590`: ((Σmax − Σcrews) IDIV Σmax) as a float + ((d3 + d2) + (d1 + d0)) × 0.25 (that
  summing order, SSE). The crew term is an **integer** divide, so it is 0 unless every man is gone (then 1) or the
  crew reaches twice the max (then −1, one less per further max crew; the cost clamps a negative total to 0): a
  half-manned ship costs only its hull damage. `ECX` is the naval unit itself (the plate said +0x130; fixed). **Settled 1:1 (treasury3-merge, 2026-10-14, disassembly):** the sums
  are `0x0057E1E0` ([+8] + [+4] + [+0], wrapping ADDs; Σcrews from +0x144, Σmax from +0x150), the quotient is
  `SUB EDI,ESI; CDQ; IDIV ECX` (`0x008F65E4`–`0x008F65F0`: signed, truncating toward 0, the remainder dropped), then
  `CVTDQ2PS` + `ADDSS` with the damage term (constant `0x01325CAC` = 0.25). Ours (`ShipState::repair_fraction`) uses
  `wrapping_add` / `wrapping_div` / `as f32`, the same results bit for bit. No traced contradiction of the exe's
  intent: the share's only other consumer, the navy vtable slot +0x58 `0x008F66A0` (vtable entry `0x0135570C`: the
  mean of 1 − share over the navy's ships, divided by its vtable +0x8 count), uses the same integer quotient, so the
  exe is self-consistent and the rule stays 1:1 (not an ORIGINAL BUG). **ORIGINAL BUG:** Σmax = 0 divides by zero
  (an integer fault); ours takes the crew term as 0.
- **Cost** `0x008EA2F0`: share clamped with `if 0 > f {0} else MINSS(f, 0.5)` (a NaN gives 0.5), f32(price) × it,
  rounded by FISTP (half to even). Model `ship_repair_cost`.
- **Order** `0x008EEB70(on)`: on: needs a repair, cost < treasury (signed, strict), the force is a navy docked in a
  port (navy +0xE0) whose slot can repair (`0x00A894D0` → the port manager's slot +0x18 `0x00B44E50`: a building in
  the slot and its recruitable list `0x00B43CA0` not empty); then `0x00BAF500(cost, 2)` and flag = 1. The flag is
  not tested here. Off: the cost is computed **again**; when the flag is set and the cost > 0, `0x00BAF660(cost, 2)`;
  then flag = 0. **Settled (treasury3-merge, 2026-10-14, disassembly):** the off branch (`0x008EEC4E`) calls
  `0x008EA2F0` first and passes its result (ESI) to `0x00BAF660` at `0x008EEC7A`; the on branch (`0x008EEC2F`–
  `0x008EEC42`) charges and stores only the flag byte (+0x168), and `NAVAL_UNIT` saves no cost, so no paid cost
  exists to refund: the refund is the cost recomputed from the ship's state at cancel time, which can differ from
  what was paid (more after new damage, nothing when the recomputed cost is 0). Ours does the same (tests
  `ordering_charges_once_and_cancelling_refunds_the_cost_now`, `a_cancel_refunds_nothing_when_the_cost_now_is_zero`).
  `0x008E1BC0` (the state-6 sale value) is not on this path. Model `set_ship_repair`.
- **The panel's and AI's test** `0x008B3120` (callers `0x008DF820` UI, `0x00AA90F0` AI): flag clear, in a force,
  the order's tests, and the port slot's building health > 99. Model `can_repair_ship`.
- **Commands:** `CCQ_REPAIR_NAVAL_UNIT` (executor `0x00937040`) orders; `CCQ_TOGGLE_REPAIR_NAVAL_UNIT`
  (`0x009387A0`) cancels a flagged ship or orders. The executors only test that the unit is a ship; ours also run
  `can_repair_ship` (what every issuer tests first), so a repeated command does not charge twice.
- **The UI (shiprepair, 2026-10-16, static trace, CONFIRMED unless tagged):** the button is `navy_repair` in
  `ui/campaign ui/layout`; its script `layout.navy_repair.luac` calls `Army.ReplenishUnits()` on a click unless the
  state is "inactive" (army.lua:591: `CampaignUI.ToggleReplenishUnit(card.Address)` for every selected card). Shown by
  `ShowNavyButtons` (army.lua:1172) when the player controls the force; "inactive" when nothing is selected, or when
  no selected card needs it (army.lua:937: some card `Replenishable` and not `Replenished`) and not all selected are
  already ordered (army.lua:907: every card `Replenished`); the tooltip (army.lua:1269) sums `RepairCost` and asks
  `CampaignUI.PlayerHasFunds`. Handler `0x009F9D60` (`HandleCampaignUIToggleReplenishUnit`, registered `0x0042A950`):
  a ship (the unit's own type, CONFIRMED: vtable slot +0x20 returns the unit in the land vtable `0x01355494` and null in
  the naval one `0x0135552C`) appends `CCQ_TOGGLE_REPAIR_NAVAL_UNIT` (id `g_dwCcqToggleRepairNavalUnitId` `0x015C3744`) and returns
  unit +0x168 == 0; a land unit appends `CCQ_TOGGLE_REPLENISH_UNIT` (`0x015C3750`) and returns the replenish query
  `0x00B78090` == 0. The card fields come from the ship snapshot `0x008DF820` (+0xE5 = unit +0x168, +0xE6 =
  `0x008B3120`, +0xE8 = cost `0x008EA2F0`, +0xEC = `0x008CE990` → `0x0055ACC0`); the land snapshot `0x008DF190`
  zeroes all four; the card writer `WriteUnitCardSnapshotToLuaTable` `0x009AA5E0` names them `Replenished`, `Replenishable`, `RepairCost`,
  `NeedsRepair`. Ours: `unit_entry` (ntw_script `ui/campaign/army.rs`) fills the four from `ship_state`,
  `can_repair_ship` and `ship_repair_cost` for a navy, false/0 for an army; `CampaignUI.ToggleReplenishUnit` pushes
  `ToggleShipRepair` for a unit whose own record is naval (`CampaignRules::is_naval_unit`) and returns the flag's inverse (test
  `the_repair_button_reads_the_models_repair_state_and_toggles_the_order`). The land branch (land replenishment) is
  not modelled: it logs once and returns false (PROVISIONAL; vanilla never calls it, the land card's fields being 0).
  `layout.navy_repair`'s OnDestroyed calls `army_manager.OnRepairButtonDestroyed`, which army.lua never defines (an
  original script error; our host does not dispatch OnDestroyed yet).
- **Cancel-all** `0x008B36D0`: the order off for every flagged ship of a navy. Callers: after a unit transfer
  (`0x008D5F10`, `0x008D47B0`, `0x008F04C0`) for each of the two forces that is a navy outside a port; the undock
  `0x00A8C0B0` (from a move, the post-battle placement `0x008F7880` and the war kick-out `0x008DA179` inside
  `ExpelFactionForcesFromRegion` `0x008D9DC0`). The undock is a method of the port (`SlotPortObject`, 0x78 bytes,
  vtable `0x01376950`, hanging off `BuildingSlot` +0x1E4) and takes the character as its argument (CONFIRMED,
  polish20). It cancels only when that character is the navy docked there (`GetPortDockedNavyCharacter`
  `0x00A84990`, the port's +0x60 handle); then it clears that handle (`UnlinkHandle` `0x008B02E0`), hands the port
  slot back to the region's owner when the slot's vfunc +0x1C test returns 0 (`RestorePortSlotHolderToRegionOwner`
  `0x00A8D830`: SetSettlementOwner(slot, region owner +0xF4, false); the test returns the slot's garrison handle
  +0xC0, CONFIRMED polish21) and sends three notifications. The navy's destructor `0x0088EF90` runs the same restore
  on its port handle (navy +0xE0) unless the campaign is being torn down (campaign +0xFC5), without a repair refund.
  Model: `undock` cancels and restores (`restore_port_holder`; no garrison in the model, so the slot always passes
  back), every removal of a force goes through `remove_force` (destroyed, emptied when its commander goes, merged
  away), which restores a docked navy's port (test `a_navy_leaving_its_port_hands_the_slot_back_to_the_owner`). The
  notifications are not modelled (BACKLOG).
  Model: `undock` (docked navies only), on
  every navy move through `move_character` (walk, the fleet sailing to land an army, a force taking a settlement)
  and once per battle result through `undock_after_battle` (after the casualties for `apply_battle_result`; the
  naval autoresolve runs it after the ships' states are written, before captures: PROVISIONAL order; a ship lost
  in the battle drops its flag unrefunded); `merge_units` (both forces).
- **Turn end:** the navy's turn-end slot (vtable `0x013556B4` +0x2C = `0x008BD430`, from the faction turn end
  `0x008BD0F0`) first repairs each flagged ship through `0x008EA2A0` = `0x00576890` (damage 0, crews and guns to
  max, sunk cleared) + men = max men + flag 0 (+ notifications), then runs the units' `UnitTurnEnd`. Model
  `repair_ships`, before the `UnitTurnEnd` events.
- **The AI's repair (treasury4, 2026-10-14, static trace, CONFIRMED unless tagged):** the intention
  `CAI_BDIM_REPAIR_NAVY` (vtable `0x01389E60`) acts through slot 12 `0x00CD24A0` → `0x00AA90F0`. That function
  first asks `0x00A8B2D0` whether the AI may shop at the navy's position; when it may, it walks the CAI force's
  unit list (+0x164) and, for every ship that needs a repair (`0x008CE990` → `0x0055ACC0`) and passes
  `0x008B3120`, calls the order `0x008EEB70(1)` directly (no command is queued), so each order is charged before
  the next ship is tested. **The shopping gate:** CAI interface +0x618 holds a shroud pointer, set only by the
  script's `disable_shopping_for_ai_under_shroud(on)` (binding `0x00979910` → `0x00AA9B40`): off stores 0; on
  stores the shroud (faction +0x6F8) of the one human faction, or 0 when there is not exactly one. `0x00A8B2D0`
  is false with no pointer, else the shroud's hidden test `0x00B11E70` (hidden set +0x34 while the shroud is
  active, +0x50), the same test as our map labels. The pointer is saved as a bool (CAI_INTERFACE v6 and up, read
  at `0x00A3E505`) and re-applied through `0x00AA9B40` on load (`0x00A3EF07`). Only the tutorial scripts call it.
  Model: `World::ai_shopping_shroud`, `CampaignModel::set_ai_shopping_shroud` / `ai_shopping_hidden`,
  `ai_repair_navy` (tests `the_ai_orders_each_ship_it_can_pay_for_in_unit_order`,
  `the_ai_does_not_repair_under_the_scripted_shroud`; script test
  `disable_shopping_for_ai_under_shroud_stores_the_one_humans_shroud`). **INFERRED:** the CAI unit list is taken
  in the force's unit order (its fill order is not traced). **Not modelled (BACKLOG):** the goal that creates the
  intention (code at `0x00CDD540`, no function defined there): our AI has no naval goals, so nothing calls
  `ai_repair_navy` yet; the CAI_INTERFACE bool is not read from or written to an ESF save (ours keeps it in our
  own save); the same gate guards 11 other AI purchase functions (callers of `0x00A8B2D0`, the construction twin
  `0x00A67890` among them). **The AI's building repair** (`TryAiRepairSlotBuilding` `0x00AA4910`, our
  `ai_repairs`) has the gate, CONFIRMED at `0x00AA491D..0x00AA4945`: when the CAI slot's +0x104 object exists
  (getter `0x0047B090`), its position (vtable +0x40) is tested and a hidden one is refused (refusal counter
  +0x81C); the CAI slot's +0xFC is the game building slot (getter `0x00A63FD0`). Modelled with the slot's own
  position (none = no gate) and the settlement's for the walls (test
  `the_ai_does_not_repair_a_building_under_the_scripted_shroud`); **INFERRED:** that position, since the +0x104
  object's type is not traced (BACKLOG). **Ghidra (V5, treasury4 fix round; completeness / effective):**
  IsPositionUnderAiShoppingShroud `0x00A8B2D0` 97/100 (structural deductions only), SetAiShoppingShroud
  `0x00AA9B40` 88/98, OrderAiNavyShipRepairs `0x00AA90F0` 80/93, BuildDirectorBoostIntention `0x00B930D0` (CAI
  faction +0x108, amount +0x10C) 91/94, ExecuteRepairNavyIntention `0x00CD24A0` 87/91,
  ExecuteDirectorBoostIntention `0x00BAF200` 88/91, CreditDirectorBoostToFactionTreasury `0x00BB33A0` (stdcall,
  returns true) 100/100. New types `CaiFaction` (+0x168 its campaign faction, read through the shared getter
  `0x00C3C330`) and `CaiRepairNavyIntention` (+0xFC status, +0x120 CAI force); boost vtables typed. What
  remains: register-only temporaries the API cannot rename (three in `0x00AA90F0`, two in
  `0x00AA9B40`); the phantom stack argument of `0x00AA9B40`; the unit's vtable calls
  +0x28/+0x2C in `0x00AA90F0` (unit class not typed); the attribute temporaries of `0x00B930D0` and the
  intention loop's `pContext` in the two Execute functions (layouts not traced); the +0x6F8 shroud read on the
  result of the unnamed getter `0x00C3C330`.
- **PROVISIONAL (BACKLOG line):** the model has no dock link (navy +0xE0); a navy is docked at the port slot within 1
  map unit of it (the same test the turns-at-sea counter uses, now `port_at`).
- **Ghidra (V5 plates, completeness 42-93; the rest is struct typing of the unit, navy and faction):**
  SetNavalUnitRepairOrder `0x008EEB70`, CalculateNavalUnitRepairCost `0x008EA2F0`, CalculateShipRepairShare
  `0x008F6590`, CheckShipDamageNeedsRepair `0x0055ACC0`, ApplyNavalUnitRepair `0x008EA2A0`, ResetShipDamageInfoToFull
  `0x00576890`, CanOrderNavalUnitRepair `0x008B3120`, CancelNavyShipRepairOrders `0x008B36D0`,
  ProcessNavyTurnEndRepairsAndUnits `0x008BD430`, CalculateNavalUnitSaleValue `0x008E1BC0`,
  ProcessRegularPaymentTurnTick `0x00B298F0`, ApplyFactionTreasuryRefund `0x00BAF660`, CanPortSlotRepairShips
  `0x00A894D0`. Added by polish19 (2026-10-14; functions created where none existed): ExecuteRepairNavalUnitCommand
  `0x00937040` and FlipNavalUnitRepairOrderCommand `0x009387A0`, the two command executors;
  CheckPortSlotHasRecruitableUnits `0x00B44E50`; IsNavalUnitInForce `0x008C92F0` (91); UndockCharacterFromPort
  `0x00A8C0B0`; PlacePostBattleShipsOrSellUnplaced `0x009DB7C0` (walks the 0x4C-byte naval battle result
  entries, copies each entry's ship state into the unit at +0x130, and for a ship that changes hands (state 2) places
  it in a force of the given side or sells it). Polished by polish20 (2026-10-15; scores now): the executors 85 / 85
  (the unit handle and naval unit have no struct yet); CheckPortSlotHasRecruitableUnits 85 (struct
  RecruitableEntryList, 16 bytes, also the return and out type of BuildSlotBuildingRecruitableList `0x00B43CA0`, 71);
  UndockCharacterFromPort 81 (this/argument swap fixed: the port is `this`); PlacePostBattleShipsOrSellUnplaced 44
  (effective 62; structs NavalBattleResult and PostBattleSideForce {side id, force}; the context `this` and the
  12-byte position stay untyped). Named by polish20: GetPortDockedNavyCharacter `0x00A84990` (77), UnlinkHandle
  `0x008B02E0` (89), RestorePortSlotHolderToRegionOwner `0x00A8D830` (73), ExpelFactionForcesFromRegion `0x008D9DC0`
  (64, effective 83: plate, locals and inline comments; the force vfuncs +0x54 / +0x90 not traced; EndProtectorateTie
  `0x00B64B10` 77, effective 87), ConstructCampaignRegionFromEsf `0x00A45950` (6, prototype and plate only); struct
  EsfRecordReader grown to 0x48 (+0x44 an owned child: `0x00DE8BD0` returns it, `0x00DE8C60` frees it, size 0x48); struct SlotPortObject
  (0x78 bytes, `BuildingSlot.pPortObject` +0x1E4), the BuildingSlot sub-object vtable fields (+0x0C, +0x24, +0xE0,
  +0xEC, +0xF0 vbtable, +0x154, list vtables +0x160 / +0x178 / +0x190 / +0x1A8) and `UnitRecord.pLandUnitRecord` +0x9C.
  polish21 (2026-10-15): 0x009DB7C0's third argument is the campaign model (its +0xF64 free-spot finder 0x00B38F10,
  its +0x8E8 placed-unit listener), not an unused value, so `NavalBattleResult` shrinks to the 0x14 bytes it reads
  (+0 side count, +0xC entry count, +0x10 entries); 0x00A51E30 is `SaveRegionRecord` (V5: write order in its plate);
  `this` typed `DealPaymentItem` on 0x00C4AB20 / 0x00C4AD60, locals named in 0x00C12EE0 (the deal and context
  arguments stay untyped: their classes are not traced). Fix round: struct `CampaignModel` (0xF68: +0x5E8 / +0x8E8
  embedded listeners, +0xF64 free-spot finder) types 0x009DB7C0's model argument (whether it is the object
  `CampaignWorld` describes is not traced); `DealValue` {gain, cost, given} types 0x00C12EE0's out value, whose last
  argument is the proposing side (an offer resolves its payer from it at 0x00C12FE8), not unused; `g_flPointEight`
  0x0131A7B4; `EsfWriter` (0x10: stream, name, flags +0xC block open / +0xD skip errors / +0xE writable) and
  `EsfOutputStream` (+0x54 error reported, +0x56 block refused) type `SaveRegionRecord`, whose fort array holds handles
  (two loads before the vfunc call at 0x00A527AE). Completeness: 0x00C4AB20 80, 0x00C4AD60 74, 0x00C12EE0 73,
  0x009DB7C0 67, 0x00A51E30 78; the rest of each is register-only variables and untraced struct pointers.

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
  - level = range class A − B (resolver unit `+0x9C`, not the `UnitRecord` +0x9C land stats row: table `0x0070D370` by
    category code copied in `0x007CA330` via `0x0070D4E0` / `0x0070D580` → `0x0070D740`; artillery 2, line of battle 3,
    frigate and galley 2, others 1; CONFIRMED polish21, `unit_kind::Category::range_class`). Then −1 if adv ≥ 0.5, +1 if adv ≤ −0.5, and
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
  `0x00AB3FF0` → `0x00AB4070`: population growth (§Population), then the conversion
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

## Population (round 17, worker 0e-panel 2026-10-10; code `ntw_sim::campaign::population`)
CONFIRMED from the exe unless tagged; the Ghidra names below are in the program.
- **Layout.** The population object sits at region +0x28 (`POPULATION`: #1..#3 = object +8/+0xC/+0x10, #4 an i32;
  reader `0x00A456D0`, writer `0x00A51C30`); its `REGION_FACTORS` part at object +0x24 = region +0x4C (reader
  `0x00A4C340`): #0 f32[7] growth factors (+0x04..+0x1C), #1 classes, #2 the **live population** (+0x30 = region
  +0x7C, what `0x0047B7E0` returns), #3 capacity (+0x34), #4 base capacity (+0x38), #5 growth % (+0x3C), #6 trend
  (+0x40: 1 up, 2 unchanged, 3 down; a new object holds 2), #7 overcrowded (+0x44), #8 migrants (+0x58), #9 policing
  cost (+0x5C), #10 the worst class's public order colour (+0x60, an ARGB u32, 0xFF000000 by default; `UpdateWorstClassOrderColour` `0x00AAA0F0`, below), #11 the religion
  breakdown (a vector: +0x4C capacity, +0x50 count, +0x54 8-byte {religion, share} entries; #1's classes likewise at
  +0x24 / +0x28 / +0x2C, 0x78-byte entries). Ghidra structs (2026-10-20):
  `RegionPopulation` (0x88: +0x0 the region's +0xC link, +0x4 region, +0x8..+0x10 #1..#3, +0x14 `PolicingTracker`
  {+0x0 link, +0x4 owner, +0x8 amount}, +0x20 previous amount, +0x24 `RegionFactors` of 0x64), typed as
  `CampaignRegion::Population`; the copy constructor `CopyRegionFactors` `0x00A4C1D0` gives each copy capacity = the
  source's count. A new region's object (`ConstructNewRegionPopulation` `0x00A457A0`, from `0x00A46A70`, not from a
  save) stores its first argument in #1 and #3 and the base capacity in #2 (1,000,000 when 0). `POPULATION` #1 (the model's old
  `population`) is read by none of growth, conversion, garrison repression (`0x008B18F0` reads region +0x28 → +0x54)
  or the panel; it equals #2 only until the population grows (`orig_fr_may1811`). The model now loads #2 as
  `Region::population` and the rest as `Region::population_state`, and the save writer writes them back with the
  religion shares (`save::write_region_population`; the shares the conversion changes were lost on save before).
- **Factors** (`RefreshRegionPopulationFactors` `0x00AA9C10`; keys `0x00A88F20` → table `0x01458B04`:
  population_growth_base / _buildings / _taxes / _military / _food_shortages / _migration / _ports, rows of
  `public_order_factors`). Each is rounded to hundredths (`0x00AA6830` = `0x00A98AD0` (x / 0.01f stored as a float,
  then FISTP) × 0.01f), in single precision except the x87 sums noted:
  - capacity = FISTP(f32(base) × (`maxpop_modifier` × 0.01 + f32(0.01 × `pop_maxpop_modifier_tech_mod`) + 1));
    ratio = pop / capacity (f32); overcrowded = ratio > 0.9;
  - base = var 14 `baseline_pop_growth` (0.3); military = −0.05 × the units (force vt+8) of the armies standing in
    the region's area whose faction is not the owner and is the rebels or at war with it (`0x00AAAFD0`, the area
    query `0x00BA6840`, `0x008CE9B0`); buildings = `pop_growth_tech` + `pop_growth_farm`; ports =
    `pop_growth_port_fishing`;
  - s = buildings + base + ports (`0x00A6ABC0`, x87, stored as a float); taxes = `pop_growth_tax_modifier` +
    f32(`pop_growth_tax_modifier_multiplier` × s);
  - food shortages when overcrowded: (1 − ratio) × 20 − s at or above the capacity, else (0.9 − ratio) × 10 × s;
  - migration 0 (set only on the panel's copy); the growth % is their sum (`0x00A6B8C0`). Then the classes' public
    order (`0x008EDF70`), the policing cost (+0x5C: var 18 × Σ class policing × √pop × max(1 + `policing_cost_mod`%, 0)
    when the faction's +0x6E0 flag is set; `population::policing_cost`), the religion normalisation (`NormaliseRegionReligionShares` `0x00AA4860`, shares of 8 bytes at +0x54, count +0x50: shares below
    0.0005 become 0, the rest are scaled to sum 1 unless they already do, with nothing left the first takes 1) and
    the worst class's order colour (+0x60, `0x00AAA0F0`).
  - A rebel owner (`0x008CEEF0`, faction +0x514 = its `factions` record, null for the rebels: writers `0x0087AA92` /
    `0x0087BEE3`): every factor 0, no normalisation, capacity and growth kept.
  - **When**: at the round start (`0x0096C050` → `0x008F1F60` → `0x00AAE710`, every region of a listed faction in the
    game, not the rebels') and in the round end's first pass (`0x00948CF0` → `0x008E1030` → `0x00AB3E70`, before
    any economy), which walks the same regions (worker polish54 2026-10-19, disassembly, CONFIRMED): `0x00948CF0`
    calls `CampaignFaction::PrepareFactionRoundEnd` `0x008E1030` with ECX = each entry of the turn list (+0x30, count +0x2C),
    which never holds the rebels (+0x1C); `0x008E1030` walks the faction's region list (+0x77C, count +0x778) with
    `0x00AB3E70` (`BuildRegionEffectSet` `0x00A67530`, then `0x00AA9BF0` → `0x00AA9C10`) only
    when +0x824 (out of the game) is clear. Then, for a faction that is not the rebels and is in the game: the land
    and naval force sums (`0x008B2150(…, 1)`) into +0x738 / +0x744 (`0x0088F950` / `0x0088F990`, each posting the
    change to `0x00BBE930` with category 1 / 2 when its +0x4 is set), `UpdateRegionPolicingTracker` `0x00AB4400`
    per region (the policing tracker, next item) and a tail jump to `ComputeFactionCannotPayFlag` `0x00BBC7D0` on
    +0xAC. So the rebels' and out-of-game factions'
    regions keep their stored factors through both passes; the model refreshed every region at the round end,
    which zeroed the rebels' factors (polish54 fix). Model: `CampaignModel::refresh_population_factors`, called by
    `TurnStep::RoundStart` and `TurnStep::RoundEndRefresh` (test
    `the_round_start_and_end_refresh_only_listed_in_game_factions_regions`).
  - **The policing tracker at the round end** (worker popnotes 2026-10-20, disassembly, CONFIRMED): it changes
    game state (the human's expense category 21, which the round end pays), not UI. `0x00AB4400` is a thunk
    (ECX += 0x28, the region's population object; jump to `UpdatePopulationPolicingTracker` `0x00AB43A0`), whose
    only caller is the per-region loop of `0x008E1030` above. `0x00AB43A0`: (1) stores the tracker's current amount
    at object +0x20 (the amount before this update; not saved, the population loader `0x00A456D0` zeroes it, and a
    scan for reads of +0x20 through the +0x28 object found none, so no rule reads it); (2) refreshes a copy of the
    region's factors (`CopyRefreshedRegionPopulation` `0x00A72990`: the region's effect set `0x00A67530`, then
    `0x00AA9C10` with garrison −1); (3) `SetPolicingTrackerAmount` `0x00A51230` on the tracker at object +0x14
    ({+0x4 owner faction, +0x8 amount}) with the copy's policing cost (+0x5C): when the tracker has an owner it posts
    new − old into the owner's economics (+0xAC) through `0x00BBE930(change, 3)` (expense group 3 = category 21),
    then stores the new amount. The copy's cost equals the stored one the same pass just wrote: `0x00AB3E70`
    refreshes with the same effect set and garrison (−1), and `0x00AA9C10`'s second argument (1 there, 0 here) is
    never read (its stack slot is only scratch, `0x00AA9F6E`). So the rule is "set each listed in-game faction's
    regions' trackers to their stored policing cost" (0 for an AI owner: the cost needs faction +0x6E0). Ours
    (recordpay, merged 2026-10-20) ports it
    (`economy::set_policing_tracker` from `CampaignModel::prepare_round_end`, with the stored
    `PopulationState::policing_cost`), which matches this trace.
- **Growth** (`GrowRegionPopulation` `0x00AB4070`, from `0x008BC650` → `0x00AB42F0` → `0x00AB3FF0`), with the stored
  factors: n = FISTP(f32(f32(pop) × growth) × 0.01f); new = pop + migrants + n; below var 35 `minimum_population`
  (1000) it becomes var 35; the trend from new − pop; capped at 200,000,000 (unsigned); then the conversion
  `0x00A63FE0` on the new population and `0x00AAA0F0`. A rebel owner: migrants and growth 0, only the conversion.
  After it `0x00AB3FF0` checks town emergence (below) and `0x00AB42F0` sets #18 and runs the town wealth
  `0x00AB4410(1)`. The three public order passes inside `0x00AB4070` (`0x008F9690`, results 5 / 2 / 1 →
  `0x008B6A40`) are the riot / rebellion checks, not part of the growth.
- **Migration** (`BuildFactionMigrationTable` `0x00A44DB0` per faction before its regions; entries `0x00A44D70`:
  capacity `0x00A89830` = round(var 17 × pop %) unless overcrowded at home, weight `0x00A89770` (0 at home or
  overcrowded), emigrants `0x00A8CC00` = round(pop × min(var 16 × unrest × 1e-4 + `0x00A6BDE0` + `0x00A6B860`, 0.05))
  at home, keeping var 35). With no weight in the faction nobody moves, and every region of a shipped map is in its
  owner's home theatre (`0x00A8B5C0`), so migration is 0 in every vanilla campaign. The model (no theatres) does not
  compute it (BACKLOG §0-E).
- **Checked** (`tests/economy_fidelity.rs` `population_factors_and_growth_match_the_vanilla_saves`; `economy_check`
  ECON_POP with ECON_MAP=1): the factors, capacity, overcrowded flag and growth of all 422 regions of 7 vanilla saves
  (12 with a military factor; −0.39999998 for 8 units: 0.01f × 40 lies exactly between two floats and rounds to even).
  `auto_nr4_t4` → `orig_over_nr4_0252`: every one of the 70 regions whose factors did not change reaches the next
  population and trend exactly, and `eur_northern_italy` (an army left during the AI turns) grew by the **refreshed**
  −0.07 % rather than its stored −0.37 %, which confirms the round end's refresh. `orig_fr_t1_b` differs only where
  rebel bands spawned after its round start. Without the map the area test falls back to a radius and misses
  `eur_northern_italy` and `eur_tyrol`.
- **ORIGINAL BUG** (`0x00AA4860`): with every share below 0.0005 it writes 1.0 to the first item even when the
  breakdown is empty (a write through a null list); the model leaves an empty breakdown alone (test
  `religion_normalisation_drops_dust_and_leaves_an_empty_breakdown_alone`).
- **Town emergence** (not modelled, BACKLOG §0-E): `0x00AB3FF0`, unless model +0xFA8 +0xA9, spawns a town
  (`0x00AADB00`) when the population reaches round(`POPULATION` #3 × (1 + var 21 `pop_growth_for_spawn`%))
  (`0x00A8D900`), and #3 becomes that threshold. The panel's next town (`0x00A673A0`) is the region's town slot (region
  +0x120 list) not yet emerged with the lowest order (+0x214); its turns (`0x00AAFB40`) are ceil((threshold − pop) /
  (projected pop − pop) − 0.001) while the population grows, else −1.
- **Projection** (`ProjectRegionPopulationRound` `0x00A727D0`, for the region info table and the region details
  panel; `this` is region +0x28): copy A = the object refreshed with the region's set and the garrison as it stands
  (`0x00A72990`); copy B = A grown one step with the faction's migration table (`0x00AB4070`, the region's set), then
  refreshed with the **predicted set** (`0x00A9B930`) and the garrison count of `0x00A190A0`; both get migration =
  B's migrants × 100 / A's population (`0x00AAA010`) and their totals (`0x00AB3EA0`); the result holds A at +0 and B
  at +0x64 (`0x00A4CE00`). Readers: PopulationChange = B's trend (+0xA4); the population growth table (`0x00A99000`:
  A's and B's factors); the public order tables (`0x00A98F50` / `0x00A98EA0` → `0x008E02E0`: the governed class of A
  and the same index of B); the religion breakdown (`0x00A99510`: Change = (B share − A share) × 100). The garrison
  factor reads the region's own population, not the copy's (`0x008B18F0` → region +0x28). Model:
  `population::project`.
- **Predicted set** (`0x00A9B930`): the region's set, then for a non-rebel owner its technologies under research
  (`0x008AFCF0` over the tree: the nodes whose researching school, node +0xC, is set by `0x008EEC90` and cleared on
  completion by `0x008EED20`; the techs[] loader `0x0087CDA0` keeps (record, researcher id) for that link), the
  faction-wide change of every construction in the owner's regions (`0x008BF110` → `0x00A798F0`: the level's +0x100 set
  added, the slot building's taken away, `0x00E05090`) and this region's local change (`0x00A67490`, the +0xE0 sets).
- **Panel garrison** (PROVISIONAL in `region_info.rs`): `0x00A190A0` adds the units of an army the player has
  selected to move into the settlement (`0x00B148B0`) or removes those selected to leave (`0x00B14900`); the model's UI
  uses the garrison as it stands (`0x00B14880`).

- **The other population users** (worker popghidra 2026-10-20, disassembly, CONFIRMED; Ghidra names, plates and
  types applied, structs `RegionPopulationProjection` {`RegionFactors` A +0, B +0x64} and `FactionMigrationTable`
  {+0xC entries}, `PopulationClass` (0x78-byte class entry: +0 `pClassRecord` = its population class row, opaque `PopulationClassRecord`; +0x38 `nPositiveOrder`, +0x3C `nNegativeOrder`, +0x58 `nRepressionOrder`), `ReligionCountVector` / `ReligionCountEntry` (the religion
  counts passed to a new region), `EsfBlockKey` (static save block name: length, name, hash), `RegionFactors` +0x60
  named `dwOrderColour`):
  - **Order colour** (`UpdateWorstClassOrderColour` `0x00AAA0F0`, from `GrowRegionPopulation` and
    `RefreshRegionPopulationFactors`): net = class +0x38 positive + +0x3C negative + +0x58 repression (`0x008E2AB0`) of the lower class
    (`0x00A60FE0` / `0x008CE260`) and the upper class (`0x00A61320` / `0x008CE320`); the lower one if its net is
    smaller, else the upper (with no matching class both finds return the first class entry, `MOV ESI,EDX` at 0x00A6101A / 0x00A6135A; only an empty class list gives the end pointer); `0x008E2AC0` maps net > 0 to 0x8000FF00, 0 to 0x80FFFF00, < 0 to 0x80FF0000. No
    reader of +0x60 was found outside the save writer, so it is display state. Ours copies `REGION_FACTORS` #10
    from the source save instead of recomputing it (BACKLOG).
  - **Writer** (`WritePopulationEsfBlock` `0x00A51C30`, from `SaveRegionRecord` `0x00A51E30`): opens the
    `POPULATION` child (`0x00DE77D0`), writes `REGION_FACTORS` first (`0x00A52A90`), then #1..#3 (u32, object
    +8 / +0xC / +0x10) and #4 (i32, the policing tracker's amount), then closes the child.
  - **Loader** (`LoadRegionFactors` `0x00A4C340`, from `0x00A456D0`): defaults (trend 2, colour 0xFF000000), then
    the fields in the order of the layout above; the classes are rebuilt from the population class table
    (`0x008861E0`); a `RELIGION_BREAKDOWN` key that is not a `religions` row logs "In table %S: '%S' is not a valid
    key" and keeps the entry with a null religion.
  - **New region** (`ConstructNewRegionFactors` `0x00A4C9B0`, from `ConstructNewRegionPopulation` `0x00A457A0`):
    +0x30 = the population, +0x38 = the base capacity; a population of 0 becomes min(base, max(100000,
    base / 10)); one class per population class row (`0x008864B0`); each religion's share = the count of the
    first matching entry of the passed religion counts (else 0), then `NormaliseRegionReligionShares`.
  - **Owner change** (`HandleRegionPopulationOwnerChange` `0x00AA16E0`, from `0x00A45950` and
    `TransferRegionToFaction` `0x00A64AC0` when its 4th argument is 0 and there was an owner): sets the policing
    tracker's owner (`0x00AAF220`); with an old owner, `ResetPopulationClassesForNewOwner` `0x00AA1730` clears
    class +0x28 (`0x00A6FBB0`) and runs `0x008E6400(region, new owner, capture)` per class: +0x64 / +0x68 / +0x20
    cleared; the class's +0x74 base becomes 0 without a capture, else 0 → −8 and negative → −3; −18 when
    `0x008CEDB0` holds or the region is the capital of ita_french_republic / egy_french_republic / france; 0 for
    the new owner's capital; otherwise a negative value with the new owner's +0x6E0 clear is scaled by
    (1 + var 0x8a / 100), rounded up; −2 when `0x009DAE00` holds; 0 for spa_napoleon. Not modelled:
    `Region::class_bases` is untouched by our owner changes (BACKLOG).
  - **The class war factor** (classfactor 2026-10-20, CONFIRMED by disassembly and decompilation; code
    `population::{class_war_change, class_round, class_round_step}`, `CampaignModel::move_class_war`, state
    `PopulationClassState::war`): class +0x20, happiness factor [8] (our `ClassPublicOrder::happiness[7]`, the UI's
    `happiness_war`). The fill `0x008EDF70` never writes it (nor +0x28, bankruptcy); public order sums the stored
    value.
    - *Load / save*: `0x008861E0` reads `POPULATION_CLASS` #1 (i32[13]) into class[1..13], so +0x20 is #1 element 7;
      #11 (+0x70) when the record's version > 1, #12 (+0x74) when > 2. Ours reads all three per class
      with a key (#0) and writes them back by position.
    - *Events*: `0x008A94D0(class, kind, owner, other)` returns when either faction is the rebels
      (`0x008CEEF0`). Kind 6 (a war declaration): category = the owner's attitude category to the other
      (`0x00B0DBA0`); lower = the class is the owner's government's lower class (`0x008CE230`), upper = its upper
      class (`0x008CE2F0`); the ranks are the governments' record +0x14 = `government_types` #3 (copied by the
      record ctor `0x00F76A70`; 3/2/1/0 shipped). Category 0: +4 if (lower and own ≤ theirs) or (upper and theirs
      ≤ own); 3: −3, 4: −6, if (upper and own ≤ theirs) or (lower and theirs ≤ own); else 0. Kinds 0..5 add the
      i32 table `0x014579F0` = −1, −2, +1, +2, −4, +4. Every change is then clamped to [−15, 6].
    - *Who calls it*: `0x00A63490` (thunk, ECX += 0x28) → `0x00A633F0`, which runs only when the region is its
      owner's capital (`0x008B3790`: faction +0x72C → +0xFC, or 0) and calls `0x00A63430` per class →
      `0x008A94D0`. Callers: `ApplyWarToDeclarerRecord` `0x00B27070` with kind 6 (no ally, no skip flag; only
      the setup `0x00B45C20` passes the flag) and the battle results `0x008ACCF0` (from `0x008F7880` at
      `0x008F7B7C`) with kinds 0..5 on each side's capital: battle state +0x60 ∈ {6, 7, 8} → winner kind 5, loser
      kind 4; else winner codes 8/9 → 2, 10/11 → 3; loser 13/14 → 0, 15/16 → 1. Ours: the war declaration
      (`war_on`); the battle kinds are ready (`ClassWarEvent::Battle`) but not called, the battle results lack
      the victory type (BACKLOG).
    - *ORIGINAL BUG*: `0x00B27070` reads the capital's vtable at `0x00B270D0` without a null test after
      `0x008B3790` returns 0 (a declarer with no capital) and crashes; ours skips the step. The battle path shows
      the intent: `0x008ACCF0` tests `0x008C9200` (faction +0x72C ≠ 0, has a capital) for both sides before each
      of its calls. *How it is reached* (traced read-only, classfactor fix round): every path into `0x00B27070`
      with no ally and no skip flag, none of which tests the declarer's capital: the declare-war command
      (`CCQ_DECLARE_WAR` handler `0x00932B70`, registered by `0x00420400`) → `DeclareWarAndCallAllies`
      `0x00B268B0` (ally 0, skip 0) → `0x00B26700` → `0x00B27070`; a deal's war record applied (`0x00C4B990`,
      vtable ref `0x0138201C`) → `0x00B268B0`; and `0x008BFA80` / `0x008BFCA0` (vtable refs `0x01357314` /
      `0x0135735C`) → `0x00B268B0` / `0x00B26700`. (The setup `0x00B45C20` passes the skip flag and the join-war
      accept `0x00C188F0` an ally: both safe.) A declarer has +0x72C = 0 when it was loaded with no capital (the
      loader `0x0087A190` stores 0 at `0x0087B20C` / `0x0087B217` for a missing region) or when `0x008BA580`
      cleared it on an owner change, `FindNearestCapitalRegion` `0x008DABC0` found no region in its home theatre
      (+0x734) and the change did not end the faction (`0x00A651D5` ends a capital-less faction only when the
      transfer's first flag is set). Not seen in vanilla play.
    - *Ghidra names* (classfactor): `0x008A94D0` PopulationClass::ApplyPopulationClassWarEvent (field +0x20
      `nWarFactor`), `0x00A63490` ApplyRegionPopulationWarEvent, `0x00A633F0` ApplyWarEventIfCapitalPopulation,
      `0x00A63430` ApplyWarEventToPopulationClasses, `0x008CE230` IsLowerClassOfFactionGovernment, `0x008CE2F0`
      IsUpperClassOfFactionGovernment, table `0x014579F0` g_anClassWarBattleChange; `0x008ACCF0`, `0x008F9690`
      and the class finders keep their FUN_ names until the battle and riot items trace them.
    - *Battle path details*: both sides must not be the rebels and both must have a capital (`0x008C9200`); the
      first block also posts a UI event (0xC1..0xC4) by which side is human (`+0x6E0`) and `0x008CECD0`
      (not traced).
    - *Round decay*: `0x008F9690`'s first part (before its riot checks, not modelled), per pass: +0x20 one step
      toward 0; a negative +0x28 +1; a negative +0x74 +1; a positive +0x54 (repression [21]) −1; a positive +0x70
      becomes 0 below 3, else −2; +0x64 == 5 clears +0x64/+0x68/+0x60. `GrowRegionPopulation` `0x00AB4070` runs
      the passes before the growth: a rebel owner, one per class; otherwise one on the first lower class
      (`0x00A60FE0`), one on the first upper (`0x00A61320`), one on the first of neither (`0x00A89C90`: not
      `0x008CE320` upper, not `0x008CE260` lower); a find with no match takes the first class (vanilla regions
      have three classes, so each once). Ours keeps +0x20, +0x70 and +0x74 (+0x28 and +0x54 are not stored:
      BACKLOG).
    - *Seams*: `population.class_round` (a region's classes after the passes) and `population.class_war` (one
      class's factor after an event).
    - *Projection* (classfactor fix round, CONFIRMED by disassembly): `ProjectRegionPopulationRound`
      `0x00A727D0` calls `GrowRegionPopulation` with ECX = the copy (`CopyRegionFactors` `0x00A4C1D0` filled it)
      at `0x00A72844`, so all three class passes run on the copy's classes; then `RefreshRegionPopulationFactors`
      `0x00AA9C10` refreshes the copy with the predicted set, calling `0x008EDF70(region, set, copy, garrison)`
      per class, so the predicted public order and policing read the copy's classes and its converted religions
      (before `0x00AA4860` normalises them). Ours: `population::project` runs the seam `population.class_round`
      on the copy (`Projection::classes`), and `FactorInputs` carries the classes and religions the refresh
      reads.
    - Owner change: `0x008E6400` clears +0x20 too (not modelled, with the rest of that step).
  - **Projection seed**: `ProjectRegionPopulationRound` passes a local holding 0x61266 by pointer to
    `GrowRegionPopulation`, which reaches `0x008B37D0` through the riot passes `0x008F9690` (class +0x6C). The
    director's RNG is seeded with the same constant (RNG section), but this use is not traced: UNKNOWN.

## Turn order (round 9 item 4; code `turn.rs`)
CONFIRMED from the exe (the calls of each function in order; event posters found by their name getters
`0x008BDxxx` and vtables):
- **Faction turn start `0x008F2620`** (called from `0x0096C160` / `0x008F4120`): (a) campaign-script hooks on +0x724
  (`0x00A072D0` / `0x00A07440`); (b) for a human: achievement counters; (c) +0x860 countdown → flag +0x85C;
  (d) `0x008ED860`; (e) **every character `0x00A24EB0`** (uses the campaign RNG); (f) **every region `0x00AAE820`**:
  slots (`SlotTurnStart` each, `0x00AAE8D0`, a port's naval queue `0x00B71FB0` right after its slot), the land queue
  `0x00B71FB0`, `0x00A249A0` (per slot: a chance roll on the campaign RNG from two building effects; INFERRED
  building-driven spawns), the settlement's virtual +0x98 → `0x00A22480`, then **`RegionTurnStart`** (vtable
  `0x01376DB8`); (g) the reinforcement manager `0x00B71FA0` (faction +0x52C, §Recruitment spawn and reinforcements); (h) **research availability `0x008F91F0`**
  (thunk `0x008AE3B0`); (i) the message manager `0x00A18A70` (model +0xFA0); (j) the character recruitment pools
  `0x00A24C90` (+0x6F4); (k) every region `0x00A6F720`; (l) `0x008F2E10` (the government +0x70C's popularity, see
  "The rebels' turn start"); (m) `0x008DA210` (characters with skill 5 or flag +0x4E0); (n) +0x800: AI `0x008B4B70` / human
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
  **Who skips what** (rebelroundend 2026-10-20, CONFIRMED by listing). The function tests the rebels
  (`IsFactionWithoutRecord` `0x008CEEF0`) and faction +0x824 (out of the game) together, four times: before the settle
  (`0x008BC65D` / `0x008BC666`), before the family year `0x008BC9C0` (`0x008BC6C7` / `0x008BC6D0`, which also needs the
  +0x794 object), before the block of construction + queue flag + relationships + research (`0x008BC862` /
  `0x008BC86B`, both jumping to `0x008BC8C4`) and before `0x008AB200` / `0x008ABF50` (`0x008BC8C6` / `0x008BC8CF`).
  The region loop of `0x00AB42F0` (population, discontent growth, town wealth `0x00AB4410(1)` at `0x00AB437E`) tests only
  +0x824 (`0x008BC685`), so the rebels' regions grow and their town wealth moves; the migration table `0x00A44DB0`, the
  character refresh (`0x009DA190` → `0x00A0C6F0(3)`) and the +0x964 list clean-up run for every faction. Ours before:
  the rebels and factions out of the game were settled (treasury paid, record ring rolled, cannot-pay and desertion
  reachable), and the rebels' relationships and research stepped (only construction skipped them). Now one gate,
  `CampaignModel::in_the_game_with_record`, used by `economy::settle_round` (`Settlement::Skipped`), `TurnStep::Economy`
  (construction, relationships, research) and the family year; the population step runs while the faction is in the
  game. `0x008AB200` / `0x008ABF50` walk the faction's force list (+0x7BC count / +0x7C0 data) and are not modelled
  (UNKNOWN purpose). Test `the_round_end_economy_skips_the_rebels_settle`.

The model now follows (a)–(r) for what it models: characters (`CharacterTurnStart`, action points) → regions (slots,
land queue, `RegionTurnStart`) → research availability → `FactionTurnStart`; and at the end
`CharacterTurnEnd` → `RegionTurnEnd` (+ offset) → `UnitTurnEnd` → `FactionTurnEnd`; construction runs in the round-end economy, before diplomacy and research, as the exe's. Before, `FactionTurnStart` came
first and `RegionTurnEnd` closed each region update. Not modelled: (a)–(d), (g), (i)–(q) and `0x00A249A0`.

**Region walk order** (regionwalk 2026-10-20, CONFIRMED by listing). Every per-region loop of the turn start and
turn end walks the faction's owned-region list (faction +0x778 count, +0x77C data; see "Faction region list
order"), front to back, re-reading the count each step, each entry an owned link whose first field is the region:
(f) at `0x008F2AF6`–`0x008F2B2D` for every faction with a record, and the same walk at `0x008F2B7F`–`0x008F2BAF`
in the branch for a faction without a record (`IsFactionWithoutRecord` `0x008CEEF0` true, the rebels: the `JNZ` at
`0x008F28E1` goes to `0x008F2B47`, their character loop, then `0x008F2B7F`), which skips the tax-rate log, the
reinforcement manager `0x00B71FA0` and research availability (see "The rebels' turn start" below); (k) `0x00A6F720` at `0x008F2BED`–`0x008F2C1D`; and the turn end's `0x00A786C0` loop at
`0x008BD1E1`–`0x008BD211`. None walks the region manager's array. Model: the turn start queues its
`TurnStep::Region` steps from `World::faction_regions_in_list_order` (test
`the_turn_start_updates_the_regions_in_the_faction_list_order`); the order matters beyond the events, since a
finished land-queue item makes its unit then (new ids are handed out in that order). The turn end's `RegionTurnEnd`
walk (`TurnStep::FactionEnd` in `turn.rs`) uses the same list (test
`the_turn_end_walks_the_faction_list_with_gains_last_and_losses_dropped`: a region gained in play goes to the
list's end, a lost one leaves it and the rest keep their order). The vanilla-save tests
(save_compat, economy_fidelity, own_save_install, campaign_play) pass unchanged. Ghidra: pre comments at the three
loops.

**The rebels' turn start** (rebelresearch 2026-10-20, CONFIRMED by listing of `0x008F2620`). Both paths share
everything up to `0x008F28DA` ((a)–(d)) and from `0x008F2BB1` on, except two more `IsFactionWithoutRecord`
(`0x008CEEF0`) tests there. For the rebels the `JNZ` at `0x008F28E1` jumps to `0x008F2B47`: the character loop
(`0x00A24EB0`) and the region loop (`0x00AAE820`) only, then the join at `0x008F2BB1`. So the rebels skip:
1. the `FactionTaxRates` log (`0x008F28E7`–`0x008F2AC4`; runs only while the byte `0x01767039`, read by
   `0x0106A690`, is set; it formats strings and writes no game state);
2. the reinforcement manager `0x00B71FA0` (faction +0x52C, at `0x008F2B35`);
3. research availability (`0x008AE3B0`, a `JMP` to `0x008F91F0`, on the technology manager faction +0x7C4, at
   `0x008F2B40`);
4. the message manager's step for the faction (`0x00A18A70` on model +0xFA0, at `0x008F2BCC`, behind the test at
   `0x008F2BB3`: it takes the faction's entry of its map at +0xB0, drops the items whose vfunc +8 is false, and for
   the rest finds a `MovieEvent` row by the faction's culture and posts it through model +0x270's vfunc +8; the
   meaning, queued movies for the faction, is INFERRED);
5. `0x008F2E10` on the government (faction +0x70C, at `0x008F2C30`, behind the test at `0x008F2C21`): it stores the
   popularity `0x008DEA60` (capital's two class public-order terms clamped to ±20, plus a rounded term `0x00A975C0`,
   plus 10, clamped 0..100) at government +0xE0, reports it to model +0xD9C's vfunc +0x2C only in `eur_napoleon` /
   `mp_eur_napoleon`, then tail-calls the vfunc +0xC of government +0xCC (not traced).
Steps both paths run: (j) the pools `0x00A24C90` (own +0x814 / `0x008CA0B0` test), (k), (m)–(r).
Model: (2) and (3) are modelled at the turn start (`march_reinforcements`, then `update_tech_availability`), and
`TurnStep::FactionReady` skips both for the rebels behind one test (tests
`the_rebels_turn_start_skips_research_availability`: A's technology opens at its turn start, the rebels' stays
unavailable through theirs; `the_rebels_turn_start_skips_reinforcement_march`: a rebel entry neither marches nor is
dropped at their turn start). (1) is a log; (4) is display-side in ours (messages are events); (5) is not modelled (the
government screen computes the popularity itself, PROVISIONAL in `ntw_script` government.rs). Also from the xrefs:
`AddRegionToFaction` `0x008AE270` calls `0x008AE3B0` twice (`0x008AE2D1`, `0x008AE2E3`); ours does not update
availability on a region gain (BACKLOG trace line). The round end's rebel test (`0x008BC650`) is a separate item.
Ghidra: EOL comments at `0x008F28E1`, `0x008F28E7`, `0x008F2B35`, `0x008F2B40`, `0x008F2BBA`, `0x008F2C28`; plates of
`0x008AE3B0` and `0x00B71FA0` corrected (callers, our name).

### Turn-step event delivery (BACKLOG §5 "eventdrop", 2026-10-20; code `ntw_script` `host::fire_all`)
- `CharacterMoved` and `ForceDestroyed` are project events: the exe declares no script event of either name
  (CONFIRMED, the declared event list in `analysis/worker3/lua_api.txt`), so they never go to Lua, by design.
- Two drops found and fixed, each pinned by a test:
  - The script host returned a turn step's or an AI turn's project events only when they were diagnostics.
    So the app never got the AI's `CharacterMoved` (no move animation) or an `OutcomeMovie` posted during the
    turn loop (the defeat movie of a human faction destroyed in an AI turn). Now every event
    goes through `fire_all`, the one path for commands, turn steps and AI turns: script events fire in the model's
    order, and every event is returned in that order (test `turn_steps_return_every_model_event_in_order`). The app
    joins a character's successive moves into one path (`play::record_move`).
  - Bankrupt desertion (`economy::bankrupt_desertion`, round-end step) destroyed the emptied forces into a local
    list, losing their `ForceDestroyed`. It now goes to the step's events (test
    `an_army_emptied_by_desertion_is_reported_by_the_economy_step`).
- Still open (UNKNOWN, not traced): an AI turn applies all its orders and only then fires their script events
  (`ntw_ai` `driver::play_faction` through the host's AI hook). A human's command fires its events right after it.
  Whether the exe dispatches each AI order's events before its next order (event dispatch from model +0x120 /
  +0x870) is untraced.
- Events a script command queues (`grant_faction_handover`, `ScriptState::script_events`) are fired by
  `ScriptHost::fire_script_events` through the same `fire_all` after each batch, so they are returned too.
  Still dropped (not this branch's): `force_make_peace` / `force_declare_war` (`game.rs`) apply their model command
  and discard its events.

**Character walk order** (charwalk 2026-10-20, CONFIRMED by listing). The per-character loops of the turn start and
turn end walk the faction's character list (faction +0x764 count, +0x768 data; see `World::character_order`),
front to back, re-reading the count each step, each entry a link whose first field is the character: (e)
`0x00A24EB0` at `0x008F2AC4`–`0x008F2AF4` for every faction with a record, and at `0x008F2B47`–`0x008F2B7D` in the
rebels' branch; (o) `0x009653D0` at `0x008F2C5B`–`0x008F2C8D` (not modelled); the human-only achievement loop at
`0x008F26DC` (`0x00A198C0`, not modelled); and the turn end's `0x009DA210` at `0x008BD1AF`–`0x008BD1DF`, right
before its region loop. None goes by character id. The list: a constructor appends (`0x008CF6B0`), removal keeps
the rest's order (`0x008B0150`), a faction transfer moves the character to the new list's end (`0x00A1B4E0`). Model:
`World::faction_characters_in_list_order` (the one filter of `characters_in_list_order` by faction, also used by
the round-end death checks) feeds `TurnStep::CharactersStart` (refill, `CharacterTurnStart`, then the hidden flags)
and the `CharacterTurnEnd` walk of `TurnStep::FactionEnd`; before, both went in id order. Test
`the_turn_walks_the_faction_character_list_with_gains_last_and_losses_dropped` (a character made in play last, a
removed one dropped, the rest in list order, at both the turn end and the next turn start). Ghidra: pre comments at
`0x008F2AC4`, `0x008F2B47`, `0x008BD1AF`.

**The other region walks** (idwalks 2026-10-20, CONFIRMED by listing unless tagged). (1) The round end's population
step (`RunFactionRoundEndEconomy` `0x008BC68E`–`0x008BC6C3` → `UpdateRegionRoundEnd` `0x00AB42F0` per entry) walks
the faction region list. Vanilla's rules read only their own region, but a mod rule's state writes reach the next
region's rule, so `population_round_end` now walks `World::faction_regions_in_list_order` (test
`the_population_round_end_walks_the_faction_list_with_gains_last`). (2) The population refresh walks the list in
both passes (round start `0x008F1FB3` → `0x00AAE710`; round end `PrepareFactionRoundEnd` `0x008E1040` →
`0x00AB3E70`), and the policing trackers too (`0x008E10CE`–`0x008E10FE` → `0x00AB4400`); order cannot change
either: each region's factors read only its own state, and each tracker books a wrapping integer change into one
category. Code left. (The exe interleaves refresh and trackers per faction; ours refreshes every faction first, the
same result since a tracker reads only its own region's freshly stored cost.) (3) The rival score's victory-region
share (`UpdatePlayerRivalScoreBelief` `0x00BD22BC`–`0x00BD2309`) counts over the CAI faction's region list
(`0x00A676E0`, CAI faction +0x114): a count, any order; code left. (4) The AI's repairs are not a region walk in the
exe: `0x00AA4910`'s one caller is the CAI slot-intention executor `0x00CE9A60` (table entry `0x01389564`; it
repairs when the slot's building handle equals the intention's +0x110, else builds through `0x00A67890`). The order
is the CAI's intention order, not traced, and it matters (an AI repair's cost is capped at the treasury left), so
`capture::ai_repairs` keeps its id-order stand-in (tagged in its doc).

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
| Backstabbing `0x00B0E420` | — | `peace_treaty` set: value −3·n·t, limit −5n (n = treaties the declarer broke, manager +0x1C; t = friendship turns) | same subculture as the declarer (`0x00B0E446` loads the declarer, not the target, for the lookup: `0x008F3490` returns its record's +0x10, `0x00B0E451` reads +0x10 of that): value −5n·t, limit −25n; then n += 1 (3 when t = 10) |
| Peace (`MakePeaceWithFaction` `0x00B262C0`) | `war` += `peace` (or `peace_dragged_by_ally`), #5 cleared, stance neutral, #15 = 10, #10–#13 = 0 | the same | the proposer's allies' access suspended against the enemy restored (`0x00B0D0C0`, proposer's side only; a side given back none expelled, §Access-loss expulsions) |
| Alliance `0x00B29DA0` | `alliance` set, stance allied, #6 = 20, #15 = 10 | the same | — |
| Break alliance `0x00B13840` | `alliance` set from `break_alliance` (0, −2, 0), stance neutral, #6 = 0, access from #17 restored (none back: the target expelled, not from the war path) | a protectorate's tie ended first (`0x00B64B10`); `alliance` reset to 0, `alliance_broken` set (−40, +2, 0), stance neutral, protectorate lines cleared, access from #17 likewise | while #6 runs: same subculture as either, `cultural_alliance_broken` value −#6 − 5k, limit −5k (k = alliances broken, manager +0x14) |
| Trade `0x00B55090` | `trade` set, #2 true | the same | — |
| Break trade `0x00B29BB0` | `trade_broken` set from `break_trade` (no effect), `trade` cleared, #2 false | `trade` cleared, `trade_broken` set (−20, +2, 0), #2 false | — |
| Embargo `0x00B28DB0` (A embargoes X at B's request) | A–X trade broken first; A→X #27 = 10 | X→B (the requester, not A) `trade_embargoed` set | — |
| Military access `0x00B44550` | #3 = −1 or += turns, #24 = #3, #15 = 10, #25 = 0 | — | — |
| Cancel access `0x00B67BD0` | #26 += 50 − 10e / 60 − 6e / 70 − 7e/2 / 90 − 5e/2 (granted 5 / 10 / 20 / other, e = #25), #3 = 0; manager +0x20 += a third (`0x00B67B20`); the target expelled from the owner's regions | — | — |
| State gift `0x00B44590` | giver charged, no credit to the receiver (`0x00C4B440`, §Treasury changes) | `state_gift` += trunc((100 − current) × x × 0.01), x = min(100, `state_gift_multiplier_linear` × m + `…_quadratic` × m² / √(GDP sum)) | — |
| Protectorate `0x00B105C0` | its old ties settled first (`0x00B104E0`: a patron's tie ended, `0x00B64B10`; a protectorate passed on); stance 4, alliance factor; then its alliances broken (`0x00B13840`) | stance 3; the patron's income line = tribute (`0x00BBCD00` = (5 + 6 + 7 of the protectorate's running record) / 5) | — |

**Per-turn update** `0x00B29100` → `0x00B29170`, called from the faction's round-end economy `0x008BC650` after its
regions (CONFIRMED place). Skipped when the owner or the target is out of the game (+0x824, CONFIRMED;
ours `CampaignModel::in_the_game` reads the flag, and only an own save from before the flag falls back to the
exe's own rule, `CampaignModel::out_of_game_by_startpos_rule`, `0x008B4330`). Order: factors drift; #6 −1; access: timed −1 with #25 and #19 +1, indefinite #19 +1, none #19 = 0;
#17 items of dead factions dropped, their turns −1; #15 −1, then 10 while allied or given access; at war #10 / #11
(armies in the other's regions, GDP and army balance: `CampaignModel::war_balances`, army value PROVISIONAL) and
#12 / #13 +1; #7 toward 0 (−2 / +1); protectorate tribute and patron income set again (booking the change); payments:
turns −1, finished removed, their lines taken back (`0x00B298F0` also adds to faction +0x954, a counter, only between
two humans); #26 −2; #27 −1. **ORIGINAL BUG** (treatyfix, 2026-10-20, static trace CONFIRMED): the round end runs
every faction's economy in one loop in turn order (`PassTurnToNextActiveFaction` `0x00948DF7` / `0x00948E06` →
`RunFactionRoundEndEconomy` `0x008BC650`), whose settle (`0x00BABE30`, step 1) comes before the diplomacy update
(`0x00B29100`, step 5). The update ticks each item (`ProcessRegularPaymentTurnTick` `0x00B298F0`: +0x14 −1) and removes
those at 0 (`0x0055AD10`: +0x14 == 0) at once, and the destructor `0x0086A120` takes back the payee's income line
(`BookEconomicsGroupAmount` on the payee's +0xAC, −amount, group 0) and the payer's expense line (`0x0088CB40` →
`0x00BBE930`, group 4). So a payer before its payee in the turn order pays an N-turn item N times while the payee
receives N − 1: the last payment's money leaves the payer and reaches no one, against the item's own turn count. Ours
(`diplomacy_round_end` step 8) keeps a finished item at 0 turns while the payee's `TurnStep::Economy` is still queued
this round; the payee's economy removes it right after its settle (`CampaignModel::drop_finished_payments_to`), so
the payee receives every payment the payer makes in either order (no toggle). Test
`the_payee_receives_every_payment_in_either_turn_order` (treaty_money_tests.rs); the user kept the fix (2026-10-20). Then the
manager's +0x20 −1 (only while above 0: `test eax,eax; jz` at `0x00B29139`, CONFIRMED; ours `saturating_sub`).

- **Diplomacy manager counters** (CONFIRMED, 2026-10-16). The manager (faction +0x528, 0x24 bytes; null for the
  rebels, which have no `DIPLOMACY_MANAGER`) holds four u32 after its relationships, saved as `DIPLOMACY_MANAGER`
  v3 #1..#4 (reader `0x00AE8E10`, called by the faction loader `0x0087A190`; writer `0x00AFC160`, called by
  `0x00892A80`, writes all four). Both constructors (`0x00AE8DD0`, the new-faction path `0x0087BD70`, and the
  reader before it reads) preset +0x14 = 0, +0x18 = 0, **+0x1C = 1**, +0x20 = 0; the reader reads #3 only when the
  block version > 1 and #4 only when > 2. Every faction of every vanilla save and startpos holds #2 = 0, #3 = 1,
  #4 = 0; #1 is 0 except one faction at 1 in `original\auto_save.save`. Users (decompile scan of 0x860000–0xCC0000;
  no destruction or re-emergence reset touches them):
  - +0x14 alliances broken while committed: `0x00B13840` reads k, then +1 (ours `World::alliances_broken`).
  - +0x18 access abuses: the war path `0x00B26700` → `0x00B27250` → `0x00B0DA90` reads the *abuser's* count
    (`mov ecx,[edi+0x528]` at `0x00B0DA9F`, getter `0x004CE450`), passes it as `count` to `0x00B0DAF0`
    (`abused_military_access` += −30 + count × (−10), clamped ≤ 0), then `inc [eax+0x18]` at `0x00B0DABA` (ours
    `World::access_abuses`, new; it replaces the `count` = 0 we had, UNKNOWN before).
  - +0x1C treaties broken: backstab `0x00B0E420` reads n, then +1 (+3 at 10 friendship turns). Ours started at 0, so
    a faction's first backstab set `peace_treaty` to 0; it now starts at `TREATY_BREAKS_START` = 1
    (`World::treaty_breaks_of`), so the first backstab costs −3·turns, limit −5.
  - +0x20 access cancel marks: `0x00B67B20` adds a third of the grievance; `0x00B29100` −1 a turn.
  Ours loads all four (`ntw_campaign::world::read_diplomacy_counters`, version-gated like the reader) and writes
  them back (`save::write_diplomacy_counters`); a faction without an entry has the constructors' values. Own saves
  are format 6 or later (`own_save::FORMAT_VERSION`): an older one holds treaty-break entries only for backstabbers, counted
  from 0 (one lower than the exe's), so its load adds 1 to each (`own_save::migrate_treaty_breaks`; the rest read
  the preset 1), and it has no `access_abuses` (read 0; older builds charged every abuse at count 0). Tests:
  `war_reads_and_bumps_the_diplomacy_manager_counters` (ntw_sim), `diplomacy_counter_tests`,
  `tests/diplomacy_counters.rs` (vanilla saves, startpos round trip) and
  `a_version_5_save_moves_its_treaty_break_counts_to_the_preset_start` (own save). No ORIGINAL BUG. Ghidra (V5): struct field
  +0x18 `nUnknown18` → `nAccessAbuses`; `0x00AE8DD0` `FactionDiplomacyManager::InitializeForFaction` (89.4 raw),
  `0x00B0DA90` `FactionDiplomacyManager::ChargeMilitaryAccessAbuse` (77.0 raw). Named 2026-10-16 (polish36):
  - `0x00B67B20` `FactionDiplomacyManager::CancelMilitaryAccessToFaction` (V5 done, 94.6 raw).
  - `0x00B0E420` `FactionDiplomacyManager::ApplyBackstabbingPenalties` (V5 done, 85.6 raw). The victim's
    record toward the declarer gets peace_treaty −3·n·t, limit −5n (`0x00B0E500`). Every other faction of the
    **declarer's** subculture (`0x00B0E446` loads `this->pOwnerFaction` for the lookup; the comparison follows in the loop), other than the declarer and the
    victim, gets −5n·t, limit −25n toward the declarer (`0x00B0E540`). Then +0x1C += 1, or += 3 when t == 10.
    Ours (`treaties.rs` `backstab`) filters on the victim's subculture: BACKLOG line, not fixed here.
  - `0x00B13840` `FactionDiplomacyManager::BreakAllianceWithFaction` (V5 done, 73.1 raw). Commitment c = record
    #6. If both sides are major powers, every human major power is notified (model +0xFA0). If c ≠ 0, factions
    of the owner's or the ally's subculture get `0x00B25F80`(owner, −c, +0x14) toward the owner; then +0x14 += 1.
    Then `0x00B13A30`/`0x00B0CA20` (with the war flag), the refresh and the +0x810 / +0x3A8 events. Ours matches.
    - The notification (allybreak, 2026-10-17, CONFIRMED by disassembly): `0x009CA7B0`
      `CampaignMessageManager::SendAllianceBrokenMessage` (V5 done, 96.9 raw, the rest the auto `this`;
      thiscall on the message manager, model +0xFA0, struct `CampaignMessageManager` partial; RET 0xC; args
      recipient, first, second) builds a 0x28-byte message (vtable `0x0136F9A8`, +0x04 the title "<first name> - <second
      name>", names via `0x008DA780`, faction +0x518) and posts it with `0x009D3250`(recipient, type 0xB, 0xB,
      message, 0). Type 11 is `alliance_broken` in the type table `0x0145C530`; the post skips the rebels
      (`0x008CEEF0`), skips while model +0xD9C → +0x6C is 1 (meaning UNKNOWN), and for a faction in a revolution
      (`0x008CA0B0`, faction +0x814) needs faction +0x81C; it looks the `message_events` row up by (type,
      the recipient's subculture, `0x00F54F80`) and hands it to the UI event model +0x270, or first holds it
      in the manager's per-faction queue (+0xC8). BreakAlliance passes (human major F, ally, breaker) for
      each F in the world's faction list (`0x00B1387F..0x00B138E5`: model +0xF5C → +0x2C count / +0x30 array,
      `FACTION_ARRAY` file order from the loader `0x0090BEB0`, no rebels; ours walks
      `World::factions_in_turn_order`, filled in that order, rebels skipped); the other caller, `CancelMilitaryAccessToFaction` `0x00B67B20`, passes (target, target, canceller) with no human or
      major test. Ours: project event `CampaignEvent::Message { recipient, CampaignMessage::AllianceBroken }`
      from `break_alliance` and the `CancelMilitaryAccess` command (test `alliance_broken_messages_follow_the_exe`);
      the delivery gates and the feed belong to the display, which has no message feed yet.
    - The +0x810 event ({owner, ally}): its one campaign listener is the CAI interface (vtable `0x01377BA4`,
      slot `0x00A8F5B0`, no function defined there), which only runs its profiling timer: no gameplay
      state, nothing to model. `0x00B67B20` also fires a +0x870 event ({canceller, target}) after the message:
      its listeners are not traced (BACKLOG line).
  - `0x00AFC160` `WriteFactionDiplomacyManagerBlock` (cdecl, from `0x00892A80`; V5 done, 100 raw). Writes the
    block at version 3 and the records through `0x00AFC340`, then +0x14, +0x18, +0x1C, +0x20 as u32; returns
    the parent `EsfWriter *`. String `0x0137D908` named `g_szEsfBlock_DiplomacyRelationshipsArray`.
  - `0x00AE8E10` `FactionDiplomacyManager::InitializeFromSave` (thiscall, `RET 0x8`, from `0x0087A190` only
    when the DIPLOMACY_MANAGER block exists; V5 done, 80.8 raw, the rest register-only temporaries). It presets
    +0x1C = 1 and +0x14, +0x18, +0x20 = 0, reads each record into a stack temp (`0x00AE9480`), appends a copy
    (`0x00AE9160`, capacity 1 then doubled), then reads #1 → +0x14 and #2 → +0x18 always, #3 → +0x1C when the
    version is > 1, and #4 → +0x20 when > 2. `pEsfReader` typed `EsfRecordReader *` (version at +0x2C).

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

**Correction (deal-items, 2026-10-10, UI_FIDELITY.md §4.9):** `0x00B449F0` is
`TransferSettlementOwnership`, a *settlement* method (`this` = the settlement, `+0x178` its region,
`+0x88` its owner; arguments new owner, force or NULL, report flag, a, b); its callers are the capture
variants (settlement vtable), liberation and the deal's region item (`0x00C18BF0`). `0x008BEAA0` is
called (disassembly `0x00B44A6A..0x00B44ABA`) when the region's *old* owner is in a revolution
(`0x008CA0B0`, faction +0x814; corrected by campover2, see "Campaign over" (3)) and that owner's `+0x818`
(the revolution's settlement, its capital's) is this settlement, with (region, new owner): the end of the revolution, not a
peace-treaty option. The government-change trigger above is therefore UNKNOWN again (not traced
further here).

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

- **Allies called into a war** (`FactionDiplomacyManager::DeclareWarAndCallAllies` `0x00B268B0`, CONFIRMED
  flow and order; V5 in Ghidra, polish50 2026-10-17; order matched in ours, warorder 2026-10-20). Every ally
  of the attacked side (stance 2–4, in the game, not the attacker `0x00898B80`, not yet in a list
  `0x00B673D0`) is called, then every ally of the attacker (`0x00B67380`). An ally already at war with the
  enemy is not asked: it joins at once (listed, `0x00B0CCE0` only). Each side's allies are taken in the
  order of its diplomacy records (stride 0x848), which is the file's: the reader `0x00AE8E10` appends
  each record (`0x00B5F330`) and nothing sorts them (CONFIRMED, warnotice 2026-10-20; ours: the loader
  keeps it in `World::diplomacy_record_order`, own save format 10, and `call_allies` and the peace ally
  loop walk it through `record_targets_in_order`; before, faction-id order). Else:
  - a human ally (+0x6E0) gets an offer and the declaration waits (CONFIRMED by disassembly, allyoffer
    2026-10-20; ours: `call_allies` returns the `AllyWarCall`, `declare_war_rules` pushes it on
    `CampaignModel::pending_ally_war_calls` and raises `AllyWarCallPending`). The offer (built by
    `0x00885DA0` at `0x00B26C06` / `0x00B26DFE`, 100 bytes, vtable `0x01357314`, class string
    "pending_diplomacy") holds +8 the target, +0xC the declarer, +0x10 the ally, +0x14 the chosen side (0),
    +0x18 / +0x19 the call flags, +0x1C..+0x5B copies of the four lists so far (`0x00AE6260`), +0x5C the
    negotiation context, +0x60 allied to the target, +0x61 allied to the declarer, +0x62 accepted (0). In the
    target-side loop +0x60 = 1 and +0x61 = `0x008CE390`(ally, declarer) (not the rebels; same faction or
    stance allied / patron / protectorate) with the declarer-loop flag; the declarer-side loop the mirror.
    `0x008E2B80` (at `0x00B26C1F` / `0x00B26E17`) pushes it on the model's pending-diplomacy stack
    (+0xF70, a linked list, newest on top) and sets the stack's countdown to 1 when the offer's vfunc +4
    (`0x008EA660`: the ally human and not in revolution, `0x008CA0B0` / faction +0x814 / +0x81C) is false,
    else 0. The UI event (model +0xB10 vfunc +8, `0x00B26C6A` / `0x00B26E5F`) carries {declarer, target, ally,
    allied to the declarer, 1 from the target-side loop / 0 from the declarer's}; then `0x00B268B0`
    returns: no notices, no war, no zones. The campaign update `0x008F4120` calls `0x008F7850` (at
    `0x008F4699`): with a countdown of 0 it waits (the offer's vfunc +0x24); else it counts down and at 0
    pops and applies the offer unanswered (+0x62 = 0: a refusal), so an ally that is no longer
    human-needed is refused automatically (not modelled: revolutions; BACKLOG). While the stack is not
    empty `0x008F4120` does not advance the turn phases (`0x008F41F9`, `0x008F457A`, `0x008F4686`,
    `0x008F4706`, `0x008F4754`); ours: `CampaignModel::step` returns none and keeps the queue, except for
    an `AiTurn` step (its orders already ran; PROVISIONAL: the AI's later orders of that turn are applied
    before the answer, the exe's CAI pauses).
    The answer is `CCQ_RESPOND_TO_ALLY_WAR_INVITATION` (string `0x0135BA00`, registered `0x00422480`,
    handler `0x009370F0`): it reads {side faction, accept byte} (`0x0090E2F0`), sets +0x62 (`0x008A8190`)
    and +0x14 (`0x008EFCE0`), then `0x008BF910` pops the top offer, applies it (vfunc 0) and frees it.
    Applying (`0x008BFA80`): refused: +0x60 → listed broken with the target (`0x00B27500`) and
    `BreakAllianceWithFaction` `0x00B13840`(ally, target, no war flag); +0x61 → listed refusing
    (`0x00B0DB40`). Accepted with +0x14 = the target: joined (`0x00B274E0`), `0x00B26700` unless at war,
    `0x00B0CCE0` on both records; +0x61 also listed refusing. Accepted with the declarer: joined
    (`0x00741C70`), the war on the target, `0x00B0CCE0`; +0x60 also listed broken (its war broke that
    alliance, `0x00B27070`). A side matching neither lists it nowhere, so it is asked again. Then
    `0x00B268B0` again with the offer's lists, which skips the answered allies (`0x00B673D0` lists +0x20 /
    +0x30 for the target's side, `0x00B67380` lists +0 / +0x10 for the declarer's), asks the next human
    ally or declares the war. Ours: `CampaignCommand::RespondToAllyWarCall` → `respond_to_ally_war_call`
    (an acceptance naming a side not asked for is refused and the call keeps waiting: the same end state
    without the round trip); the call is in our own save (serde default, no format bump). The panel is the
    `ally_attacked` event message (`ui/campaign ui/event_message_scripts/event_message.luac`, layout
    `ui/campaign ui/layout_ally_attacked`: assist_button1 / assist_button2 / decline_button "breaks
    alliance"), filled from the Lua table of `0x00A08B90` (Aggressor, Defender, LocalFaction,
    AlliedWithAggressor / AlliedWithDefender, InvitedByAggressor / InvitedByDefender): not wired yet, so the
    app does not ask a human ally at all: `CampaignModel::ask_human_allies` stays off, the human is
    skipped and keeps its alliance, as before this branch (PLACEHOLDER at the gate in `call_allies`;
    BACKLOG §3 event messages; the panel turns the switch on). Refusing at once instead would break the
    human's alliance on every AI attack on its ally (review round 1). Not traced: `CCQ_INVITE_ALLIES_INTO_WAR` (`0x0135BB34`, handler `0x00936630`, the
    diplomacy screen's `InviteAlliesIntoWar`);
  - an AI ally decides through the AI (`0x00AAA950` → `0x00AAA920`, §6).
  - Join (`join_war`): the joined list (`0x00B274E0`), the ally's own `0x00B26700`(enemy, ally, 0, 0) unless
    already at war, then `0x00B0CCE0` on the side's record for the ally and the ally's for the side
    (access turns saved in #17, access −1). The joining war is the full `0x00B26700` with an ally
    (`war_on` in ours): ally→enemy `0x00B27070` sets the `war_dragged_by_ally` event (−70 drifting to −130)
    and #5 = the side (+0x794, written only with an ally), clears a patron's +0x844, breaks an alliance
    (`0x00B13840` war flag 1), ends the access (+0x78C), breaks trade (`0x00B29BB0`), backstab only without an
    ally, drops payments, stance war; enemy→ally `0x00B27250` gets the same ally argument (`PUSH ESI` for
    both calls): `war_dragged_by_ally` too, payments dropped, stance war, and a given access only ends
    (no `0x00B0DA90` abuse charge, which needs no ally). No third-party loop and no notice for a joining war
    (both need no ally). (Ours before: enemy→ally got `war`, no alliance or trade break, no access end,
    `0x00B0CCE0` before the war, an ally at war with the enemy skipped.)
  - Refuse: an ally of the attacked side breaks its alliance with it (`0x00B27500` list, then
    `BreakAllianceWithFaction` `0x00B13840`, war flag 0); an ally of the attacker that refuses is only
    listed (`0x00B0DB40`) and keeps its alliance. Ours (`call_allies`) now does the same (it broke it on both sides; the attacker-side case is unreachable while the AI decision is the PROVISIONAL "refuse only when allied to both", since the attacked side's call already broke that alliance).
  - Order: the allies are called first, then every other faction in none of the four lists gets the war
    notice `0x009DAFE0` (CONFIRMED by disassembly, warnotice 2026-10-20). The loop walks the faction list
    (model +0xF5C, turn order, no rebels), skips the two sides and any faction in one of the four lists
    (`0x00B2BEC0` tests lists +0, +0x10, +0x20, +0x30: the attacker's joiners `0x00741C70`, the attacker's
    refusers `0x00B0DB40`, the attacked side's joiners `0x00B274E0`, the attacked side's refusers
    `0x00B27500`; an ally already at war counts as joined). The notice is message type 0x112 `war_declared`,
    posted through `0x009D3250` only when both sides are major (+0x524), titled the declarer's name, a new line and the
    target's, with an empty detail; there is no human test, every listed faction gets one (ours:
    `CampaignMessage::WarDeclared` posted by `declare_war_rules` in list order). Then the
    achievement hook below, and only then the war itself, `0x00B26700`(target, on-behalf, skip-notice,
    refresh 0) (ours: `declare_war_rules` → `call_allies` → `war_on`, CONFIRMED). So the third parties
    judge the war by their attitude after the allies' wars and broken alliances: an ally of the declarer
    that just joined is hostile to the target (−70) and gets `war_against_enemies` for the declarer.
    `0x00B26700`'s third-party loop (no ally only) walks the faction list (+0xF5C, no rebels): X not a side,
    X's live category to the target (`0x00B0DBA0`) 0 and the target neither the rebels nor the pirates
    (`0x008CEE00` = `0x008CEEF0` || `IsPiratesFaction` `0x008CEE20`, tested on the **target**, not X;
    ours: `is_rebel_faction` / `director::is_pirate`) → `0x00B27390` (war_against_enemies); else
    category 4 → `0x00B27330` (war_against_friends). (Ours walked every faction, the rebels included;
    `listed_third_parties` now skips them, warnotice 2026-10-20.) It also sets faction +0x925 = 1 on the
    target (`0x00B2670D`) then the declarer (`0x00B26716`), for a joining war too, the flag the script
    condition `WarStartedCharacterFaction` reads (`0x008A66C0`: the context character's +0x2A0 faction's
    +0x925; false without a character). The flag is cleared (word +0x924 = 0, `0x008BD408`) in the
    faction's turn end `0x008BD0F0` (only caller `0x00948CF0`), after the FactionTurnEnd dispatch
    (`0x008BD3F6`) and before the shroud update; the constructors (`0x008799D0`, `0x0087BD70`) start it
    clear; it is saved as `FACTION` #63 (the loader `0x0087A190` reads it at `0x0087B769`, 8 after the second
    `CAMPAIGN_BONUS_VALUES`; writer `0x00892A80`). The shipped `eur_napoleon` start position holds it set on
    exactly the factions at war with a non-rebel faction (13; CONFIRMED by the real-install test
    `war_started_and_record_order_load_and_round_trip`), so it is true for them until their first turn end. Ours: `FactionDetails::war_started`, set in `war_on`,
    cleared at the start of the faction's next `TurnStep::Advance` (so FactionTurnEnd handlers still see
    it), read and written at #63 (CONFIRMED). With its refresh flag set `0x00B26700` also posts
    `0x009DAE40` (type 0x14D `campmap_faction_joins_your_enemies`, to the target); only the deal's join-war
    loop `0x00C188F0` passes it, never `0x00B268B0` (modelled: "Deal join war" below). The declaration's own war
    (`0x00B27070`) also calls `0x00A63490`(6, target) at `0x00B270DC`, before it sets the `war` attitude
    factor: each population class of the declarer's capital gets `0x008A94D0`(kind 6), which moves the
    class's war factor [8] (+0x20) by its attitude category to the target (§Population, "The class war
    factor"). Ported (classfactor 2026-10-20): `war_on` → `move_class_war` with `ClassWarEvent::WarDeclared`
    when there is no ally; ORIGINAL BUG at `0x00B270D0` (a declarer without a capital crashes; how it is reached:
    §Population, "The class war factor") fixed. Then the
    declarer, the target and everyone listed except the attacker's refusers get a path-grid obstacle
    refresh.
  - Deal join war (joinwar 2026-10-20, CONFIRMED by disassembly): record 11 of the negotiation (vtable
    `0x0138202C`; lists as the regions record: +0x14 demanded (count +0x1C, data +0x20), +0x24 offered (+0x2C, +0x30);
    proposer `0x00C4BC20`, recipient `0x00C4CE90`). Its accept, vtable +0x3C `ApplyJoinWarRecordOnAccept`
    `0x00C188F0` (thiscall, no args), runs in `0x00C114B0`'s record loop after the payment (7) and before
    the expulsions: for each **offered** faction F, the proposer's manager (+0x528) `0x00B26700`(F, on
    behalf of the recipient, skip 0, refresh 1); then for each **demanded** F, the recipient's
    `0x00B26700`(F, on behalf of the proposer, 0, 1). No at-war, stance or same-faction test, here or in
    `0x00B26700`. With an ally, `0x00B26700` skips the third-party loop and `0x00A63490`; with refresh 1
    it then posts `PostFactionJoinsYourEnemiesNotice` `0x009DAE40` (thiscall on model +0xFA0, RET 8,
    args target, joiner): a 0x28-byte text message (vtable `0x0136F9A8`), title the joiner's name
    (`0x008DA780`), detail empty, sent by `0x009D3250` to the target as type 0x14D (type table entry
    `0x0145CA64` → `campmap_faction_joins_your_enemies`), no major-power or human test (unlike
    `war_declared` `0x009DAFE0`); then zones target then joiner (`0x00B795A0`), path view `0x00B67490`,
    prices `0x00BCB020(0)`, then the diplomacy events +0x7E0, +0x3A8. No allies are called and no access
    is saved for the war (no `0x00B0CCE0`, unlike `0x00B268B0`'s joining allies). The command
    `CCQ_DIPLOMACY_PROPOSE_JOIN_WAR` (registered in `0x00420B80`, posted by the record's +0x40
    `0x00C4A960` with descriptor `0x01387A04`: header, then list counts, then each faction id;
    executor `ExecuteProposeJoinWarCommand` `0x009336C0`): returns if the command's +4 flag is set;
    reads list A then list B through `0x0090E2F0` (a failure flag kept across both lists,
    `0x00933754`..`0x00933766`: any id that does not resolve, or a failed read, aborts the command); clear → `0x00C11B80(11)`
    → `0x00C5C730`; two empty lists → nothing; else a temporary action (`0x00BF3900`) gets A as demands
    (`0x00C1F690`, +0x90) and B as offers (`0x00C43D30`, +0xA0), and `0x00C49AA0` replaces the record
    through its setter +0x44 `0x00C4A670` (src +0x90 → record demanded, +0xA0 → offered, `0x00522F00`;
    the row text `offer_or_demand_join_war` lists offered first). Ours: `Negotiation::join_war`,
    `CampaignCommand::ProposeJoinWar` (`propose_join_war`, an unknown faction aborts as for regions),
    `accept_deal` → `join_war_by_deal` → `war_on(x, F, Some(ally))` + `CampaignMessage::FactionJoinsYourEnemies`
    + `refresh_faction_zones(F)`, then the joiner's; `war_on` now skips the stance change when already at
    war (the records are applied again, as the exe's). An item naming its own joiner fails the deal
    (project check: the exe would declare a faction's war on itself). PROVISIONAL (not ported): the AI's
    evaluation of record 11 (`0x00AA5ED0`: decline when its `diplomacy_options` index 11 is 1 or 3, else
    a goal by `0x00A62400` with the record's value +0x38 `0x00C12D40` = per offered F `0x00A75330(.., F,
    1, ..)` and per demanded F `0x00A75330(.., F, 0, ..)`, summed and inflated): ours declines every
    deal with a join-war item. The panel (`ntw_script` `diplomacy.rs`): `Propose(offers, demands,
    'request_join_war')` (`0x009BF3C0` case 0xD: two tables of faction keys through
    `FindCampaignFactionByKey`, demands → `0x00C1F690`, offers → `0x00C43D30`) posts `ProposeJoinWar`
    (an unknown key proposes nothing, logged once: the exe pushes a null entry, what its command then carries UNKNOWN);
    `RemoveAction` posts it with the clear flag (`0x00C5CA50`); `BuildOfferAndDemandStrings` gives one
    join-war row per side with items (its `offer_or_demand_join_war` text not built, as the other
    records' rows).
  - Achievement hook (polish57 2026-10-19, CONFIRMED by disassembly): `0x009C7660` returns the campaign
    achievement tracker (model +0xD9C; class vtable `0x01319E90`, constructor `0x0048E930`, +0x1C the
    tracked local faction's key; names table `0x0144F3F8`, 75 entries from EmpireAchievements.cpp). Its
    vtable +0x34 (`0x004B08B0`) gets (declarer key, target key, `0x008CE390`(target, declarer): the target's
    record for the declarer is allied, patron or protectorate, or the two are one faction; evaluated
    before the war step) and adds 1 to tracker +0x6C when the declarer is the tracked faction and the flag
    is set: a "declared war on your own ally" stat. Sibling slots are the same kind (+0x30 `0x004B0220`
    counter +0x68; +0x70 `0x004B1130` trade ≥ 8000 a turn; +0x78 `0x004B0F60` 10 protectorates). No rule
    reads the tracker for a game rule, so it is not modelled (no Steam integration). The only reader of
    +0x6C is the tracker's vtable +0xB4 (`0x004B0690`, jumps to `0x0049E5D0`; read at `0x0049E67F`; also
    reached from vtable +0x0C `0x004B0790` and from `0x004B020B`). When the counter is above 0 it clears
    it and sets a changed flag, which triggers the stats store `0x004BF340` through [this+8]. It does the
    same for +0x68/+0x70/+0x78/+0x7C/+0x80 and unlocks achievements (`0x00499FA0`). So these are stat and
    achievement counters, not game rules.
  - Negotiation lines (polish57, CONFIRMED by decompile): the `WarCallNotifyContext` argument is the
    diplomacy negotiation the war came from (+0x18 / +0x1C its two sides, +0x19C its action record). Only
    the deal's war item `0x00C4B990` (vtable `0x0138201C`) passes one (its deal state +4), directly or,
    when the target is human (+0x6E0), through the deferred war command `0x00885E80` / `0x008BFCA0`
    (+0x14) and the human ally's `AllyWarCallOffer` +0x5C (`0x008BFA80`); `ApplyRecordStanceAfterSetup` and
    `0x00932B70` pass 0. At the end `PostWarCallNegotiationLines` `0x00C4FFC0` posts to each side, on the
    UI hub's +0x360 negotiation-line event (the one `ApplyDealToNegotiationRecord` `0x00C49AA0` uses for
    its result lines), payload `0x00BF6270` = {side, code 5, the negotiation string `0x00C5C540` about the
    other side, copies of the four lists}. Presentation only (what the screen shows for code 5 is not
    traced; BACKLOG line proposed for the diplomacy screen).
  - PROVISIONAL AI decision: join, unless also allied to the enemy.
- **Treaty money** (CONFIRMED, booked into the running records and paid by the round settle; "Running record"):
  - regular payments: the payer's category 22, the payee's 8, each round until their last turn;
  - a protectorate pays its patron a fifth of categories 5 + 6 + 7 of its running record (`0x00BBCD00`, truncating):
    the protectorate's category 24 (kept in #8), the patron's 10 (kept in #9).

### Scripted diplomacy permissions: `force_diplomacy` (§0-E, 2026-10-10; CONFIRMED by disassembly)

The permissions are the relationship's u32[14] at +0x7EC (save #18; option order = `DIPLOMACY_OPTIONS`). Code:
`CampaignModel::{set_diplomacy_option, diplomacy_option, may_propose, may_accept}` (ntw_sim negotiation.rs) and the
bit rules in details.rs. The panel, the deal rules, the AI snapshot and the save all read these.

- **Script handler** `SetDiplomacyOptionFromScript` `0x009792D0` (registered at `0x01458154`). It takes
  `(a, b, option, offer, accept)`.
  - Arguments are read last-first: 0x01055840 reads a bool from stack −1 and pops it, so accept is read, then offer;
    0x01055760 reads the three strings.
  - A non-boolean or non-string argument is a Lua error, and nothing is stored.
  - `a` and `b` are looked up by exact key in the campaign's faction map (`+0xF5C`, `0x00955B70`). There is no
    "all" form and no culture form. An unknown key stores nothing and returns 0.
  - The value is `!accept + 2·!offer` (`0x009793C8`..`0x009793DE`).
  - The option is matched exactly against 14 literal keys (case-sensitive, `0x0044F000`). An unknown key stores
    nothing.
  - It writes only a's relationship towards b, through a's diplomacy manager (+0x528) and
    `FindDiplomacyRelationshipByTarget` `0x00B64C50`. It is not symmetric.
  - The store goes through `SetRelationshipDiplomacyOption` `0x00B28670`, the only writer.
- **Readers.** All go through `GetRelationshipDiplomacyOption` `0x00B27FE0`:
  - offer bit (2|3), when the owner may not propose:
    - the negotiation records are forbidden (`0x00BF5CB3`);
    - the AI's offered-goal filter (`IsDiplomacyProposalBlockedByScript` `0x00CAD070`, thiscall on a
      {owner, target} context, candidate stride 0xD4; remove_if wrapper `RemoveScriptForbiddenProposalGoals`
      `0x00C73F50(begin, end, owner, target)`, called from `0x00CB8200`);
    - the diplomacy screen's trade check at `0x009F33E1` (below);
  - peace is possible only when the value is ≤ 1 (`0x00C1A730`);
  - accept bit (1|3), when the owner declines the option from the target:
    - the AI deal evaluator (`0x00AA5ED0`);
    - the AI's asked-goal filter (`IsDiplomacyAcceptanceBlockedByScript` `0x00CAD0E0`, stride 0xC8; wrapper
      `RemoveScriptForbiddenAcceptanceGoals` `0x00C74410(begin, end, owner, target)`, CONFIRMED by decompile);
  - deal goals (`0x00CCB150`) need both: the proposer's offer bit and the recipient's accept bit;
  - counter-deal items (`0x00C1E770`) go through the action → option table `0x01459080`;
  - `0x00C561C0` tests == 3 (below).
- **The four sites of BACKLOG §0 (dipperm, 2026-10-17; CONFIRMED by disassembly unless tagged):**
  - **AI proposal task** `0x00CE4A30` (vtable method, data ref `0x01389400`; `this+0x108` = the target AI
    faction). It builds the goals it asks of the target (the asked-goal builder
    `BuildAiDealGoalsAskedOfFaction` `0x00CC13D0` + `0x00CCCCA0`; V5 79.2, maximum 81.1, 59.2 before polish46:
    cdecl (`CaiFaction *` target, `AiDealGoalVector *` out {+4 capacity, +8 count, +0xC 200-byte goals},
    `AiBdiCore *` core, int use-third-faction flag, `CaiFaction *` third faction); its campaign factions come
    from the new `CaiFaction::GetCaiFactionCampaignFaction` `0x00C3C330` (CaiFaction +0x168, 96.9); the one raw
    offset left is a region object from `0x008CF5F0` whose type is not traced) and filters them with `0x00C74410` at `0x00CE4B3C`, owner = its own faction, target = the
    target: the AI never asks for what it would itself decline from that faction (its own accept bit, not the
    target's). It then builds the goals it offers (`BuildAiDealGoalsOfferedToFaction` + `0x00CCB750`) and calls
    `0x00CB8200`, whose `0x00C73F50` passes drop what the AI may not propose to the target. Then
    `ProcessNegotiationDealProposal`.
  - **Counter-offer builder** `0x00CC58C0`, called only from `EvaluateProposedNegotiationDealAI` `0x00AA5ED0`:
    the same two filters (acceptance at `0x00CC59BE`; `0x00CB8200` at `0x00CC7409` / `0x00CC7794`). Owner = the
    AI, target = the counterpart (`+0x108` of its task record), INFERRED by analogy with `0x00CE4A30`.
  - **Counter-deal round** `0x00C1E770` (callers `0x00CC58C0` and the `CCQ_DIPLOMACY_COUNTER_DEAL` executor
    `0x00932EE0`, registered by `0x00420780`). It calls `0x00C561C0`, which increments the round `+0x24`, swaps
    proposer `+0x18` and recipient `+0x1C`, and per record calls virtual `+0x48` and marks it forbidden
    (`0x008CE140`) when the new proposer's value == 3. `0x00C1E770` then clears every record (`0x00C28DB0`) and
    re-marks forbidden at 2|3 for the new proposer → recipient, so the result is
    `forbidden = !may_propose(new proposer, new recipient, option)`: the == 3 pass is overwritten (redundant,
    not a bug). Ours: no counter deal yet (`PrepareCounterOffer` is a stub returning false; the AI declines);
    `negotiation_actions` for the swapped sides already gives the same forbids (`deals_tests.rs`).
  - **Diplomacy screen trade check** `0x009F33E1`, in `RetrieveFactionListForDiplomacy` `0x009F2EA0`
    (registered at `0x0042A070`; it was not a function in Ghidra). Per faction f (skipping `+0x824`), with the
    player's relationship to f: stance `+0x790` → `+0x10` == 0 (war) → no `Trading` entries. Held agreement
    (`+0x788`, `0x009F30E6`): `Trading` = "cannot_trade" when `0x00B28E10` (embargo, `+0x840` != 0) holds on
    player → f or f → player, else "trading"; `LandTrade` = `0x00B28620` (no waypoint with sea byte `+0x14`) on
    the first of f's routes (trade manager exporter map `+0x11C`) whose importer (`+0x54`) is the player, false
    without one; the tooltip is `trade_status_tooltip_already_trading`. Else: scripts = player → f option 0 not
    2|3 and f → player option 0 != 3 (`0x009F345C`..`0x009F3467`: f's value 1, it declines trade, still allows);
    possible = `0x00C1A7D0(player, f, 0)`; `Trading` "can_trade" when both, else "cannot_trade"; `LandTrade` =
    both and `CheckTradeRoutePossible` `0x00BA42E0(player, f)` == 1; tooltip `_can` when both, else `_cannot`,
    formatted (`0x004F1200`) with `0x00BC7910(player, f, 0)` when the scripts allow, else "" (`0x0131005F`).
    `0x00C1A7D0(p, r, cancel)` (CONFIRMED by decompile): cancel → held; else not at war, not held, both route
    checks non-zero and no embargo either way. Ours: `CampaignModel::{trade_possible, diplomacy_trade_status}`
    (negotiation.rs), which also gives the negotiation records' trade test (the route check is no longer
    assumed); the panel (`faction_list_for_diplomacy`) reads it; tests `diplomacy_trade_status_*` in
    trade_check.rs. The tooltip parameter is decoded below (dippermai).
    The list itself skips factions out of the game (`+0x824`, `0x009F2F69`; ours too, test
    `the_diplomacy_faction_list_skips_factions_out_of_the_game_and_the_pirates`), "britain" while the string at
    `[0x015C4938]` `+0x9FC` → `+0x8` → `+0xD9C` → `+0x30` equals "episodic_3" (`0x009F2F77`..`0x009F2FA3`; the campaign
    key, see the trade agreement explanation entry below) and "pirates" (`0x009F2FB5`). `Relationship` is set only for
    stance 0, 2 or a patron (`IsStancePatron` `0x00B5DCB0` at `0x009F3064`): stance index 3, "protectorate" when
    the player is f's patron; the player as f's protectorate (index 4) gets no key (ours: `world.stance`, test
    `the_diplomacy_relationship_names_only_the_players_protectorates`). The first-route search walks f's routes
    in stored order (`0x009F3143`..`0x009F31B5`, the order-keeping route vector), the same order as ours
    `trade_paths` (CONFIRMED). V5 for `0x009F2EA0`: 71.6 (45, then 68.9 before polish46; effective 91.6,
    maximum 80): polish46 typed the script context `UIComponent *` and, through the class moves of its
    callees (`CampaignUiManager::GetLocalPlayerFaction` `0x009BF110`, `CampaignFactionRegistry`,
    `FactionDiplomacyManager::FindDiplomacyRelationshipByTarget` `0x00B64C50` now returning a typed
    `DiplomacyRelationRecord *`, 0x848 bytes), removed the other generic locals. What is left is structural
    (register-only temporaries) plus the raw offsets +0xC/+0x10/+0x14/+0x18 on one reused register and a
    reused stack slot (it holds a relationship, then a faction, then a builder), which no single truthful
    type covers.
- **Trade agreement explanation and the "episodic_3" skip (dippermai, 2026-10-17; CONFIRMED by disassembly).**
  - `0x00BC7910(out, a, b, counts)` on the trade manager (campaign `+0xF90`; suggested name
    `BuildTradeAgreementTooltipText`, not renamed in Ghidra) builds why a can or cannot make a trade agreement
    with b. Either side without a capital (faction `+0x72C`, `0x008C9200`) → the empty string (`0x00BC7D3D`).
    With `counts`, it first fills random string 0x1C7 `trade_agreement_tooltip_export_route_count` ("Sea export
    routes remaining: You: %d/%d Them: %d/%d ...", each `%d` in turn, `0x00446C80`) with (a's left, a's cap,
    b's left, b's cap), cap = `0x008DC150` + `0x008DC360` (sea route cap + capital port cap), left = cap less the
    side's export routes (trade manager map `+0x11C`, `0x00BB0980`) that are not land-only (`0x00B28620`).
    The reason (`0x00BC7BB1`): an embargo either way (`0x00B28E10`) → 0x1CD `embargoed`; else the route check
    b → a (`0x00BA42E0`) == 0 → 0x1C9 `no_route_them`; else a → b: 0 → 0x1C8 `no_route_you`, 1 → 0x1CA
    `neighbour`, 2/3/5/7 → 0x1CB `sea_routes`, 4/6 → `sea_routes` then 0x1CC `existing_route_shifted`. The text is
    the reason, then the counts line, then the shifted line (all `random_localisation_strings_string_trade_agreement_tooltip_*`,
    present in the shipped loc). Callers: the diplomacy screen's `TradingTooltip` (`0x009F35CC`, counts 0, only
    when no agreement is held and the scripts allow, after the `_can` / `_cannot` text via `0x004F1200` at
    `0x009F362A`) and the negotiation panel's trade button (`0x009B55B7`..`0x009B5615`, counts 1, while the local
    player is not at war with the counterpart). `BuildPossibleActions` sets an entry's `Tooltip` only when the
    built text is non-empty (UniString length test `0x004F3500` at `0x009B58B4`, CONFIRMED disassembly), so a
    side without a capital leaves the trade button's `Tooltip` unset. Ours: `CampaignModel::{trade_agreement_tooltip,
    diplomacy_trade_tooltip}` and `ActionTooltip::TradeAgreement` (negotiation.rs), the counts from
    `CampaignModel::sea_route_use` (trade.rs, shared with `build_trade_route`), rendered by
    `trade_agreement_text` (ntw_script diplomacy.rs); a mod's route check code outside 0..7 reads as
    `no_route_them`, as the exe's switch default. Tests `trade_agreement_tooltip_*`,
    `diplomacy_trade_tooltip_only_for_an_allowed_new_agreement` (deals_tests.rs),
    `the_diplomacy_trade_tooltip_explains_and_hidden_factions_are_left_out`,
    `the_trade_tooltip_adds_the_counts_and_shifted_lines`, `an_empty_trade_tooltip_is_left_unset` (diplomacy_tests.rs).
  - The string `0x009F2EA0` compares with "episodic_3" is the campaign key, read through the campaign's `+0xD9C`
    pointer (its `+0x30`). The pointer is set by the campaign model constructor `0x008742C0` at `0x008742EB` (to the
    first dword of its first argument, a local of the caller `0x009894A4`; CONFIRMED disassembly 2026-10-17); where
    that object's `+0x30` is written is not traced. The two other
    readers are `CampaignIsEpisodic` (`0x009E0570`, script registration at `0x00428410`: starts with "episodic",
    `0x004F4480`) and `0x00A8D659` (== "mp_eur_napoleon"). A third (techswitch, 2026-10-20): the building effects table `0x009B2BB0` (starts with "episodic", `0x009B2FD9`; §Research, `CampaignFeatures::effects_technologies_hidden`). "episodic_3" is one of Empire's campaign keys (the static
    list built by `0x0046D990`: episodic_1, episodic_3, episodic_5, main_2, ...); no Napoleon campaign folder has
    it, so the skip never fires in vanilla. Ours: `CampaignFeatures::diplomacy_hidden_factions`, set to
    `["britain"]` for the key `episodic_3` by `ntw_campaign::features::original`, read by `faction_list_for_diplomacy`.
- **Diplomacy UI stances (dipstance, 2026-10-17; CONFIRMED by disassembly).** The stance table `0x0145D378`
  names 0 war, 1 neutral, 2 allied, 3 "patron", 4 "protectorate". A relationship record's stance (`+0x790` →
  `+0x10`) is 3 on the patron's record about its protectorate (`SetPatronStanceAndIncome` `0x00B10480`) and 4
  on the protectorate's record about its patron (`MakeProtectorateOfPatron` `0x00B105C0` through `0x00B104E0`);
  `IsStancePatron` `0x00B5DCB0` tests 3, `IsStanceProtectorate` `0x00B5DCC0` tests 4. Ours matches
  (`Stance::Patron` = 3 on the patron's side, `Stance::Protectorate` = 4, mirrored by `World::set_stance`). Three
  diplomacy-screen readers had the two swapped or read the wrong side:
  - `RetrieveDiplomacyDetails` (`0x009F2750` → `0x009B2690`, all five list bits `0x1F`): a record with stance 3
    (`0x009B2834`) goes to "Protectorates", 4 (`0x009B284D`) to "ProtectorOf"; factions out of the game (`+0x824`,
    `0x009B27E1`) are skipped. Ours had the lists swapped and listed out-of-game factions; fixed (test
    `diplomacy_details_list_protectorates_under_their_patron`). "TradeRights" reads `+0x788` on the faction's own
    record about the other (the `[f + 0x528]` entry, `0x009B2869`), ours `World::trade_held(f, other)` (CONFIRMED,
    test `diplomacy_details_trade_rights_read_the_faction_s_own_record`).
  - `RetrieveRemainingMilitaryAccessTurns` (`0x009F4380`): -1 (indefinite) for the record whose stance is 4, so
    a protectorate gives its patron access for good. Ours gave the -1 to the patron's record; fixed (test
    `a_protectorate_gives_its_patron_access_for_good`).
  - `RetrieveExistingTreaties(a, b)` (`0x009F2B80` → `BuildExistingTreatiesText` `0x00B750D0` on b's record about
    a): first protectorate_of_player when that record is 4 (`0x00B750E6`), else for every other record of b with
    stance 4 (`0x00B751AD`) protectorate_of_non_player + " " (`0x013165D0`) + that faction's display name (faction
    `+0x518`, `0x008DA780`), appended without a separator (`0x004F0CE0` at `0x00B75273`); then at_war (0),
    alliance (2), trade_agreement (b's `+0x788`), giving_military_access_* (b's `+0x78C`), has_military_access_*
    (a's `+0x78C`), trade_embargoed (b's `+0x840`), embargoing_trade (a's `+0x840`), each after "\n\n"
    (`0x00B0B120`). There is no peace-treaty line. Ours tested the patron's side (the same result through the
    mirror), read a's trade flag instead of b's and lacked the non-player protectorate line; now it reads b's
    record throughout (trade line `World::trade_held(b, a)`, `+0x788` on the same record as the giving-access line at
    `0x00B753D7`, CONFIRMED) and has the line (test `existing_treaties_name_the_protectorate_ties`). The display name
    is shared with DefaultSaveName (`faction_display_name`, ui/campaign/mod.rs).
  - Empty cases (dipempty, 2026-10-17; CONFIRMED by disassembly). `RetrieveDiplomacyDetails` (`0x009F2750`,
    one key), `RetrieveExistingTreaties` (`0x009F2B80`) and `RetrieveRemainingMilitaryAccessTurns`
    (`0x009F4380`, two keys) look each key up with `FindCampaignFactionByKey` `0x00955B70` and return nothing
    at all (`return 0`, no value pushed, not nil) unless all are found; found, they return one table (filled
    by `0x009B2690`, bits 0x1F), one string, or two numbers.
    - The registry (model `+0xF5C`, `CampaignFactionRegistry`) hashes every faction of its own list
      (`+0x2C` count / `+0x30` data) under its record key (rebuild `0x009516A0`, key `0x008E5060`; empty
      buckets "%UNUSED_KEY%" `0x0135FD20`). The list is `WORLD/FACTION_ARRAY` in file order on load
      (`0x0090BEB0`) or every faction of the new campaign's setup records (`0x0090C4A0`);
      the rebels (`REBEL_FACTION`) sit apart at `+0x1C`. Nothing shrinks the list: a destroyed
      faction only gets `+0x824` = 1 (`DestroyFactionAndResetRelations` `0x008AB6C0`), and the third rebuild
      (`0x0095A810`, from `0x00915E20`) reorders it, human factions (`+0x6E0`) first, keeping every one. So
      dead, out-of-the-game and not-yet-emerged factions are all found; only the rebels and unknown or empty
      keys are not. Ours: `CampaignModel::registry_faction`, every faction but the keyless rebels (a mod
      faction left out of `turn_order` still takes its turn, so it counts too; test
      `the_faction_registry_holds_every_faction_but_the_rebels`). The rebel tests the two-key calls make
      after the lookup (`IsFactionWithoutRecord` `0x008CEEF0`, faction `+0x514` null) can never fire.
      `0x009F2B80` has no out-of-the-game (`+0x824`) test (its plate said so wrongly and is fixed, 2026-10-17).
    - Arguments: each wrapper calls the string reader `0x01055760` once per key. The reader reads index -1,
      the top of the stack, then pops it (`lua_settop(-2)`, `0x00FE9AD0`); the two-key wrappers' first read
      fills b, the second a. So the keys are the last N arguments and extra ones before them are never read
      (`f(x, a, b)` is `f(a, b)`). A string or a number (converted) is taken; anything else is the script error
      "Trying to retrieve type string, found type %s" (`0x01095090` → `0x0106BD10`, which returns, no Lua
      error), the value stays on the stack and the reader's ok flag (context `+4`) is cleared, so the later
      reads are skipped and their keys stay empty: only the first fault in read order, from the last argument
      back, is reported. A missing argument is not bounds-checked (`index2adr` `0x00FE8B50` returns top + idx
      for any idx > -10000): it reads the slot below the arguments, the called function (`luaD_precall`
      `0x00FECED0` sets base = func + 1 for a C function), and reports "found type function".
    - Callers (diplomacy_panel.luac): `InitPanel` passes its faction to RetrieveDiplomacyDetails and indexes
      the result's lists at once (also at lines 848/849); RetrieveExistingTreaties (line 244) is measured with
      `#`; RetrieveRemainingMilitaryAccessTurns (line 1138) feeds `InitialiseMilitaryAccessButtons`, which
      compares each number (`< 0`, line 1176). All pass exactly the player and the opposing faction, so
      vanilla never reaches an empty case or an extra argument; a mod's script that does gets a Lua error in
      its own code.
    - Ours had returned nil (two nils for the turns), found the rebels by their empty key
      (`faction_by_key("")`), given the player's details for a call without a key, raised a Lua error on a
      nil key to the two-key calls, and (first round of this branch) read the keys from the first argument
      and logged only a call's first error. Now the three calls look up `registry_faction` and read their
      keys through `faction_key_args` in the exe's order, logging each distinct fault (call, argument, type)
      once with the exe's type name (tests `diplomacy_reads_return_nothing_for_the_rebels_and_unknown_factions`,
      `a_missing_faction_key_returns_nothing_and_is_logged_once`, `faction_keys_are_the_last_arguments`). The
      exe's cleared ok flag stays on its script context and
      silences its later reads (the ORIGINAL BUG at CreateFromLayout, host.rs); ours fails only that call.
  - A patron is never unnamed: the exe's record holds the patron's faction pointer (record `+4`, read by
    `0x004B4A50` at `0x00B75255`, no null test). Ours keys by id, so a patron id with no faction is a broken
    model: logged once and its line left out (test `a_patron_missing_from_the_world_is_logged_once_and_left_out`).
  V5 for `0x00B750D0`: 73.1 (prototype, locals, 40 EOL comments, its 11 key strings named
  `g_szKey_CurrentTreaty*`). `0x00B0B120` AppendTreatyLineAfterBlankLine: 97.0 (`0x0131604C` "\n\n" named
  `g_szBlankLineSeparator`). `0x009F2B80` RetrieveExistingTreatiesFromScript: 93.9 (left: the untyped script
  context, no struct). `0x00B105C0` FactionDiplomacyManager::MakeProtectorateOfPatron: 91.1, effective 94.2
  (typed `this`, records as `DiplomacyRelationRecord`, its `+0x790` field named `pStanceRecord`; left: the UI
  notifier's vtable call, no struct).
- **Pirates.** After loading, `0x00B5BD70` (from `0x008BAF20`) sets every option to 3 on each relationship whose
  owner passes `IsPiratesFaction` `0x008CEE20` (key == "pirates"), through `0x00B28690`. Ours does this in the
  importer (`ntw_campaign` world.rs).
- **ORIGINAL BUG** (`0x00B64C50`): on a miss, the lookup returns the manager's first relationship (stride 0x848).
  So `force_diplomacy(a, a, …)`, or a `b` with no relationship entry, overwrites the option on an unrelated
  relationship. Ours refuses it and logs the refusal (`set_diplomacy_option` returns false). Like the exe, ours never creates a relationship there: a pair with none (e.g. an open-format map without the full matrix) is refused too.
- **Ours.**
  - The AI's peace gate is `may_propose(me, other, peace) && may_accept(other, me, peace)`, the deal-goal rule.
    The war gate is `may_propose(me, other, war)`.
  - The save writes #18 from the model.
  - The diplomacy screen's trade status calls `may_propose` / `may_accept` (above).
  - Not yet modelled, as there is no port of the code they filter: the AI proposal task `0x00CE4A30` with its two
    goal filters, the counter-offer builder `0x00CC58C0` / `0x00CB8200`, and the counter-deal round
    `0x00C1E770` / `0x00C561C0`. Each must call `may_propose` / `may_accept` with the directions above when it is
    ported. Checked again (dippermai, 2026-10-17): our AI (ntw_ai) proposes only peace and war and makes no
    counter-offers (`ai_accepts_deal` declines where the exe may counter), so these readers wait for the AI deal
    proposal / counter-offer port (§6); the deal evaluator's accept bit (`0x00AA5ED0`) is already read
    (`ai_accepts_deal` → `may_accept`, `deal_goal_allowed`).

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

## Campaign director (worker region-transfer 2026-10-11, static trace, CONFIRMED unless tagged; code `ntw_sim::campaign::director`)

`CDIR_INTERFACE` (campaign +0xF80) steers the campaign around the humans. Ported: its one map
change, `CDIR_INTENTION_TRANSFER_REGION_OWNERSHIP` (ctor `0x00B93840`), behind the seam
`director.region_transfer`. The other desires' RNG draws and rules are ported behind the seam
`director.desires` ("Desires" below, worker director-rng 2026-10-12); the effects they hand to
systems the model lacks stay PROVISIONAL.

- **When** (`AdvanceFactionTurn` `0x008A9920`, our `TurnStep::Handover`): first the transfers queued
  for the starting faction (`0x00BA90E0`). Then, when the ending faction is human and the starting
  one is not (`0x00BC6DF0`), the director pool runs (`0x00BA9220` via `0x008F1F40`, loop
  `0x00BBC880`): the rival-score beliefs are refreshed, then the desire takes one step (`0x00BAE0A0`).
- **RNG:** the director's own, +0x60; the constructor seeds `0x61266`, but the loader then sets it
  from the file (below), so a new eur_napoleon campaign starts from the start position's 37138.
  Draws are the MSVC LCG (×0x343FD + 0x269EC3) scaled to a float range (`0x00BBD6E0`).
- **In the files** (loader `0x00B94170`, writer `0x00B9D6E0`, run by the model loader `0x00872550`
  at `0x00873F3D` for start positions and saves alike, after the portrait allocator `0x008733CB`):
  `CAMPAIGN_MODEL/CAMPAIGN_DIRECTOR` is a record array of one item holding `CDIR_INTERFACE`, v3
  (written as 3, `0x004CDC50`): #0 u32 pool component id (+0x1C → +0x18; the load fails without it);
  `CDIR_INTERFACE_AUTORESOLVER_MODIFIERS[]` {u32, u32} (+0x34); `CDIR_INTERFACE_REGION_OWNERSHIP_CHANGES[]`
  {receiver, region, queued owner} ids (+0x3C list, count +0x48, data +0x4C, entry ctor `0x00518210`;
  read from v1); `CDIR_INTERFACE_UBER_FACTIONS[]` {u32} (+0x54; read from v3); u32 RNG (+0x60; read
  from v2; below v2 one campaign-RNG step (+0xFB8), RNG = state >> 16, the record's last read, after the uber
  factions; CONFIRMED, worker deadqueue 2026-10-17: in `0x00872550` only `0x008733BA` (the portrait allocator's
  RNG, stepped by `0x008733CB`), `0x00873F3D` (this loader) and `0x008741BC` reach +0xFB8 within three call levels,
  so ours steps after the portraits and nothing after it steps the campaign RNG during a load). A file without the
  record: the model loader builds a new director (`0x0087418C` → constructor `0x00B949B0`), which also takes one
  campaign-RNG step for its RNG and builds a new BDI pool (`0x00B95230`); ours keeps the new director's seed and the
  file's counters (PROVISIONAL, ntw_campaign director.rs `fill`: our writer cannot add the record, so stepping would
  repeat at each reload of our save; no vanilla file lacks it). The desire's counter (+0xFC)
  is `CDIR_DESIRE_TRANSFER_REGION_OWNERSHIP` #0 u32 (vtable `0x0137FEB4`; loader `0x00B90210`,
  writer `0x00B9B6C0`) under `CAI_INTERFACE/CAI_INTERFACE_DIRECTOR_BDI_POOL/CAI_BDI_POOL/CAI_BDI_POOL_DESIRES`.
  Values: eur start position v3, pool 4000, RNG 37138, counter 0; `orig_fr_may1811` RNG 3875805033,
  counter 3 (tests `ai_areas_on_the_europe_start`, `director_state_in_an_original_save`). Code:
  `ntw_campaign` director.rs reads and writes the RNG, queue, counter, autoresolver modifiers and
  the army XP counters; the uber factions (not modelled) are kept from the file. Our own save keeps
  `DirectorState`.
- **Save ids** (CONFIRMED on `orig_fr_may1811.save`): the queue, the modifiers and the XP counters
  hold CAI component ids, not faction or region indices. A faction's is the first plain value of
  its `CAI_WORLD_FACTIONS` item, whose `CAI_FACTION` field #6 is the model faction; a region's is
  the first plain value of its `CAI_WORLD_REGIONS` item, whose `CAI_REGION` field #10 is the region
  key. Our importer read the queue as raw indices before; it now maps both ways and logs the
  unmapped ids once. The XP counters are the `CAI_BDI_DESIRE_FACTION_EXPERIENCE` record whose first
  bool is true (army; false is the navy desire), {faction id, count} written in CAI faction order.
  may1811 (Peninsular campaign; components 213 spa_britain, 216 spa_portugal, 219 spa_france
  (human), 222 spa_spain, 225 a faction with an empty key): XP counters {britain 3, portugal 3,
  spain 3}; modifier pairs britain, portugal and spain each over 225.
- **Step** (`0x00BAE0A0`): chance = `BASE_CHANCE + EXTRA_CHANCE_PER_TURN × counter` (f32, clamped to
  [0, 1]; `cdir_configs`, lookup `0x00BD2A40`, as float `0x00BD29B0`). A roll above it adds 1 to the
  counter. Otherwise the candidates are gathered (`0x00BA2B90`); with none nothing changes; else a
  second roll in [0, total] picks one; the transfer (receiver, region, the region's owner now) is
  queued (`0x00BAF410` → `0x00BA33A0`, list +0x48/+0x4C) and the counter goes to 0.
- **Candidates** (`0x00BA2B90`): for each transferable area, in area order, each linked area's owner
  that is another faction, a major power (+0x524, `0x00C3FB20`), not human and at war with the
  owner, in link order (a receiver bordering twice is listed twice), weighted by its rival score,
  times `UBER_FACTION_MODIFIER` for the `CDIR_CVN_UBER_FACTION` faction.
- **Transferable** (`0x00BB52E0`), in order: the area has a game region with an owner (CAI region
  vt+0xC; seas and theatres have none); not under siege (`0x00C3E1E0`: CAI region +0x12C, its +0x34
  interface, vtable `0x01382C20` built by `0x00BEDE70` from the loader `0x00BF0130`, slot +0x94 =
  thunk `0x00C402D0` → [this+0x138] → `0x00A63FD0` (game region +0xFC, the settlement) → settlement
  vt+0x98, the `IsUnderSiege` slot of `0x008A1FF0` / `0x00C5C040`; never true in ours until sieges
  exist, BACKLOG "Sieges on the campaign map"); the owner is neither major nor human (`0x00BB51B0`);
  no human mobile in the area (`0x00C42DC0`/`0x00C42DD0`, the area's mobiles +0x18C as the loader
  `0x00BECCE0` reads them: the mobiles situated in the area, i.e. whose `CAI_SITUATED` #2 names it,
  `cai_audit mirror_loc`; in the original's save every mobile's #2 is the `regions.esf` region
  under its position, sea included, test `mobiles_are_situated_in_the_region_under_them`, so a sea
  area holds the fleets at sea in it; ours finds them on the path grid, whose sea cells carry their
  sea region, director2 2026-10-15); for each link (`0x00C43560`/`0x00C43570`, other side
  `0x00C1BCE0`) the far area is not a human's and holds no human mobile; and no human sees the settlement or its other
  slots (`0x00BB55A0`: per human with a shroud, `0x00BB5640` / `0x00C3C920` positions, our
  `sees_fully`, exe `0x00B7A1D0`).
- **Apply** (`0x00BA90E0` → `0x00BCA430` → `0x00B58A30`): each entry for the starting faction leaves
  the list; it is carried out when the region's owner id still equals the queued one (`0x00CF0D30`
  compares the +0x18 ids) and the region is still transferable: the garrison is destroyed and the
  region changes hands like a capture, with no capture report.
  An entry whose receiver is out of the game is neither purged nor carried out: it stays in the list (and in
  the saves) because that faction never starts a turn (0x00948CF0, above "Who gets a turn"), and is served at its
  first turn start after it re-emerges (CONFIRMED; worker deadqueue 2026-10-17). The list's other users are the
  queueing step and the loader / writer; the killer path's garrison spawn (0x00BA80E0) does not touch it. Each
  served entry is erased without freeing its 12-byte triple (a memory leak only; no gameplay effect, nothing to
  fix in ours). V5 (polish46, 2026-10-17, CONFIRMED by disassembly): the applier is
  `CdirInterface::ApplyPendingDirectorRegionTransfers` (96.1, its maximum; 78.6 before). Structs: `CdirInterface`
  (80 bytes: +4 pOwner, +0x48 nPendingTransferCount, +0x4C ppPendingTransfers), `CaiWorldHost` (140 bytes:
  +0x88 pCaiWorld), `CaiWorld` (312 bytes: +0x11C nFactionCount, +0x120 ppFactions, +0x130 nRegionCount,
  +0x134 ppRegions); its former void* locals are three `CaiWorldHost *`, the starting `CaiFaction *`, the
  entry's region id (int) and owner id (uint).
- **Rival score** (`0x00BD21B0`, belief +0x100): max over the humans of `m × (RELATION_PCT × (1 −
  relation) + VICTORY_REGIONS_PCT × b + PROXIMITY_PCT × c)`, capped at 1; relation from `0x00A7A2E0`
  (attitude mapped to [0, 1] plus allied / access / trade / peace weights); `b` the share of its
  regions the human's victory conditions name (imported from the start position's victory option,
  `ntw_campaign::victory::fill`); `c` the share of its regions' linked areas the human owns (seas
  count in the denominator only); `m` 1 for a major power else `MINOR_FACTION_WEIGHT`. 0 for the
  rebels, the listed no-rival keys (`pirates`, `0x00C3FD50`, set by the importer
  `ntw_campaign::rules_from_db`), humans and factions out of the game.
- **The relation is a stored copy** (worker cai-belief 2026-10-13/14, CONFIRMED by decompile and
  disassembly unless tagged): `0x00A74A20` (entry +8 of the faction's +0x100 list found by
  `0x00A89E00`) refreshes it by `0x00ABAFB0` only when read while dirty (`0x00CF1170`, then
  `0x00CCCF70`). Ours keeps the copy too: `DirectorState::beliefs` (director.rs) holds each clean
  belief (relation per faction and the enemy list, saved and hashed); a faction not listed is
  dirty, and a read uses the stored value or what a rebuild would give (`relation_belief_value`,
  `believes_at_war`). The AI's evaluation of a proposed deal (`CampaignCommand::ProposeDeal`,
  `0x00933690` → `0x00C49BE0`; the read `0x00CCB810` / `0x00CCB210` → `0x00A74A20` is the
  recipient's, below) stores the recipient's belief (`refresh_relation_belief`).
  PROVISIONAL: the exe's other 184 readers of `0x00A74A20` are not modelled, so a belief one of
  them would have stored during a human's turn is rebuilt later in ours.
  - **Dirty on every diplomacy change.** The model's diplomacy-changed event (model +0x3A8; an event
    object whose vt+8 Broadcast `0x008DBAC0` calls each listener's vt+4) is fired by war
    (`0x00B26700`), the treaty, gift and payment changes and each in-game faction's round-end drift
    (`0x00948CF0` → per faction `0x008BC650` → `0x00B29100`). The CAI interface registers ~50
    model listeners (`0x00A6FE40`, called from the ctors `0x00A3CE70` / `0x00AAE5C0`; iface +0x61C is
    the campaign model, faction → model is `[faction+0xA8]+8`); its +0x3A8 listener (iface +0x24C,
    vtable `0x01377AA8`, slot 4 `0x00A8F320` → `0x00A88CE0`) marks every relation belief of the
    registry (+0x124/+0x128) dirty. Ours: `CampaignModel::diplomacy_changed` clears every stored
    belief at each modelled broadcaster: war, joining a war, peace, alliance and its break, trade
    and its break, access grant and cancel (command and abuse), state gift, regular payment,
    protectorate, government change, round-end drift, a capture, and an agent's misdeed (detected:
    `0x00B0D870` / `0x00B67FD0` / `0x00B71CF0`; blamed: `0x00B0D700` / `0x00B67E60`; each skips
    everything, the broadcast included, only when the victim is the rebels, `0x008CEEF0`; fix round
    cai-belief-fix2, disassembly). Not broadcasters: an embargo without a trade agreement (below),
    the one-off payment, the round start's computed factors, a deal's region handover (test
    `which_changes_mark_the_relation_beliefs_dirty`). The exe's broadcast sites (`ADD ECX,0x3A8`
    then `CALL [EAX+8]`, script over the whole listing, 2026-10-14): 23 functions; the 20 above are
    modelled (count re-checked by cai-broadcasters: the 3 below are the only others). The other three
    (cai-broadcasters 2026-10-14, CONFIRMED by decompile):
    - `0x00B1B4E0` (`SetupAllReligionAttitudeFactors`): resets the religion factor (slot 15, +0x260;
      `0x00B1B540`, ours `setup_relationship_factors`) of every record, then broadcasts. Its only caller
      `0x00B45EA0` builds the diplomacy records from the start position (`0x00B0A3A0` per startpos item),
      ← `0x008CB820` ← `0x0090C4A0` ← `0x008742C0`: campaign setup, before any belief is read. Nothing
      to clear in ours.
    - `0x00B64B10` (`EndProtectorateTie`; its skip flag is 0 from `0x00B104E0`, the war flag from `0x00B0CA20`, accessloss 2026-10-17): stance neutral both ways (`0x00B64C10`
      on the protectorate→patron record clears the tribute +0x7AC; `0x00B54530` on patron→protectorate
      clears the income +0x7BC), the patron's characters moved out of every one of the protectorate's
      regions (`0x008D9DC0` per region, patron in ECX; modelled, unless the skip flag is set), both factions'
      path-grid obstacles refreshed (`RefreshFactionPathGridObstacles` `0x00B795A0`, then
      `CampaignPathGrid::ReapplyPathGridFactionView` `0x00B67490`; ours builds them at search time), then the broadcast. No attitude factor. Callers: the alliance break's patron side `0x00B0CA20` (stance 4 inside
      `0x00B13840`; ours `break_alliance_with` calls `end_protectorate_tie_with` there, the war flag as the skip flag) and `0x00B104E0`
      (`SettleProtectorateTiesAndJoinPatron`, inside make protectorate `0x00B105C0`): for each of the new
      protectorate O's records, stance 4 (O is X's protectorate) → `0x00B64B10`; stance 3 (O is X's
      patron) → X becomes the new patron P's protectorate (`0x00B105C0` recursively, deal flag 0); then
      O→P `0x00B29DA0` unless allied, stance 4, tribute line `0x00BBCD00`. `0x00B105C0` then breaks every
      other alliance of O (stance 2, target ≠ P, `0x00B13840`) and broadcasts. Ours ran a full alliance
      break (alliance / alliance_broken / cultural factors) on O's old ties and documented the patron's
      alliances as breaking; now `end_protectorate_tie`, the recursion, and O's alliances break (tests
      `a_new_protectorate_settles_its_old_ties`, `which_changes_mark_the_relation_beliefs_dirty`). Not
      modelled: `0x00B105C0`'s deal flag (param_3, deal record +0x16): an AI X (+0x6E0 clear) at war
      with O whose `0x00B0CDF0` towards P holds makes peace with O (`0x00B262C0`) (BACKLOG).
    - `0x00B73430` (`BreakTradeBothWays`): trade broken on both records, then the broadcast. Callers:
      faction destruction `0x008AB6C0` → `0x00B2B8B0` → `0x00B2B920`, only on the dead faction's record
      towards its killer (§Destruction and re-emergence, "Destruction by a killer"; ours `destroy_faction_by`), and a lost trade route: `0x00BB9410` / `0x00BB9B90` / `0x00BBE1A0` when
      the route check `0x00BA42E0` fails (← `0x00A64AC0`, the owner change). The owner-change path
      `0x00BBE1A0` is modelled (§Trade, Route loss); the listener handlers `0x00BB9410` / `0x00BB9B90`
      are not (BACKLOG).
  - **Capture.** Every capture variant sets the owner change's capture flag (`0x00B58560`,
    `0x00B58890`, `0x00B58C00`, `0x00B58A30`); `0x00A64AC0` with it set, neither side the rebels
    (`0x00C3FE80`) and the owner a different faction, fires the event through `0x00B62120`. A deal's
    or a liberation's handover `0x00B58A10` leaves the flag clear (battles.rs `take_settlement`).
  - **+0x6DC re-mark flag.** Not only file-loaded: the CAI round-end listener (iface +0xA8, vtable
    `0x013779F4`, slot 1 `0x00A8F830`; on model +0xA8, fired at `0x008A9A25` in `0x008A9920`) sets
    it to 1, also propagates the CAI world +0x1EC object's dirty state (vt+0x30 `0x00BCED30` →
    `0x00CF0B10`; object ctor `0x00BEEEC0`, vtable `0x01382DBC`, stored by the world loader
    `0x00BF0130` at `0x00BF16BA`) and calls `0x00A41890` / `0x00A77CC0` (+0x73C) / `0x00A502E0`. The
    CAI frame update `0x00A761F0` clears it and calls `0x00CF7F60` on the current core (BDI
    processing `0x00CCCE90` only for an AI faction; the human's per-frame path is `0x00A658C0`).
  - **Turn change (INFERRED).** `0x00CF7F60` marks [W+0xF0] (W = core vt+0x14 `0x006E9F70`,
    [core+0x84]) with a NULL source. The refresh `0x00CCCF70` clears the dirty flag and the source
    list; a NULL source (the registry sweep `0x00A88CE0`, this mark) always rebuilds through vt+0x24
    (= `0x00ABAFB0`, vtable `0x01378434`+0x24), a non-NULL source (propagated from a dependency,
    `0x00CF0B10`) only when that source's own refresh returns 1. Relation beliefs get the turn
    change's mark only by propagation, so whether they rebuild needs the debugger; ours marks every
    belief dirty at the handover (`director_handover`). It matters only with two humans or a read
    after the pool.
  - **The one change without a broadcast before the pool.** Round start (`0x0096C050` at
    `0x008A9AFE`) resets the computed factors per faction (`0x008BAF30` → `0x00B71E90` →
    `0x00B71EC0`: faction_leader slot 21, enlightenment slot 22) without a broadcast, in the same
    `0x008A9920` call after the round-end broadcasts; no function between them reaches a belief read
    (`0x00A74A20` / `0x00CCCF70` / `0x00ABAFB0`, direct-call search to depth 7-8 from every step and
    from `0x00A8F830`), so the beliefs are still dirty and the pool's read (`0x00BD21B0`) refreshes.
  - **The embargo.** `0x00B28DB0` (`EmbargoFactionTradeForRequester`) broadcasts only through `0x00B29BB0` (break trade, which ends with
    the broadcast) when +0x788 is set; otherwise it only writes +0x840 = 10 (`0x00B28E00`) and the
    attitude (`0x00B734E0` → `0x00B69640`, a plain store). So a proposal to Z followed by an embargo
    of Z with no trade agreement leaves Z's belief stale, in the exe and in ours (`CampaignModel::embargo`;
    test `an_embargo_after_a_proposal_is_not_seen_by_the_rival_score`). In a deal (loop `0x00C18800`,
    lists +0x2C/+0x30 and +0x1C/+0x20) it runs with this = side A's diplomacy (+0x528), X from the
    list and the deal's other side B, so X's record towards B gets `trade_embargoed` (CONFIRMED, deals-fix 2026-10-14: the list loop
    passes the deal's other side as the third argument); ours matches (`CampaignModel::embargo(a, x, requester)`,
    `DiplomaticAction::Embargo(x)`; test `a_deal_embargo_makes_the_embargoed_faction_resent_the_requester`).
  - **Attitude writes without a broadcast** (callers of the setters `0x00B69640` / `0x00B69620` /
    `0x00B02CB0` / `0x00B69B90`; cai-broadcasters 2026-10-14, CONFIRMED by decompile):
    - `0x00B544F0` (`AddAbandonedAllyFactorUnlessAtWar`): on a record whose stance is not war, `0x00B02CB0` adds
      the value to `abandoned_ally_in_war` (slot 11, +0x1C0) with event 0's drift 2 and limit 0
      (`0x00B27FF0`). Callers, both `spa_napoleon` only: the loot option `0x00B541E0` (loot = choice 0 of
      `0x008C0310`, or 2 with no liberation target): the region's rebel key (+0x10C) `spa_portugal` /
      `portugese_rebels` → `spa_portugal`, `spa_spain` → `spa_spain`; that faction (by key, `0x00955B70`),
      when it is not the looter (`0x00898B80`) and the looter is not `spa_france`, gets −50 towards the
      looter, no broadcast. Ours: `CampaignFeatures::loot_resentment` (the importer fills it for
      `spa_napoleon`), applied in `resolve_capture`; tests `looting_a_homeland_region_angers_its_faction`
      and `which_changes_mark_the_relation_beliefs_dirty`. The other caller `0x00AAB440` (−20, a region
      revolt or handover in `spa_napoleon`; meaning INFERRED) belongs to region revolts, not in the model
      (BACKLOG).
    - `threatened` (slot 20, +0x328, event 22: 0, 3, −30): `0x00B72F00` adds it to the recipient's record
      towards the proposer when the deal's threat flag (+0x2B8) is set, on accept `0x00C114B0` and decline
      `0x00C1F210`; no broadcast. CONFIRMED and modelled (`CampaignModel::apply_threatened`, tests
      `a_threat_leaves_the_threatened_factor_on_accept_and_decline`,
      `propose_deal_carries_the_threat_flag_and_decline_reaches_the_model`):
      - The flag: negotiation +0x2B8 is stored by every propose `ProcessNegotiationDealProposal 0x00C49BE0` from the command's argument.
        Lua `negotiation:ProposeDeal(bool)` (`0x009BFCF0`) reads it with `0x01055840` (top of the
        stack, a boolean; any other type is the script error `0x01095090`, ours logs once and proposes
        nothing). diplomacy_panel.lua `SendOffer` (:634) passes false, `ThreatOfForce` (:1565) true.
      - Accept `0x00C114B0` applies the records (`0x00C43520`), sets result +0x28 = 1, then (at
        `0x00C11671`) calls `0x00B72F00` on the recipient (+0x1C) faction's diplomacy (+0x528) towards
        the proposer (+0x18). Decline `0x00C1F210` sets result 2, calls `0x00B72F00` the same way,
        then the listener (vfunc +0x38) and posts the reply strings.
      - A repeated decline adds the event again: the executor `0x00932F00` checks only command byte
        +4 and tail-jumps to `0x00C1F210`, which keeps no record of an earlier answer; `0x00B02CB0`
        caps the value only from above at limit 0 (drift 3 > 0), so two declines give −60. Ours
        matches (test `a_repeated_decline_adds_the_threat_again`).
      - `0x00B72F00` → `0x00B72EC0` → `0x00B02CB0` adds the config's event 22 (+0x110 value, +0x10C
        drift, +0x108 limit) to slot 20.
      - The AI evaluator `0x00AA5ED0` itself calls accept or decline; the flag there only suppresses
        the counter-offer. Executors: DECLINE_DEAL `0x00932F00`, ACCEPT `0x00932E40`, PROPOSE
        `0x00933690`. Ours: `CampaignCommand::ProposeDeal { threat }` / `DeclineDeal` / `AcceptDeal`.
      - `CanThreaten` (which gates the threat button) stays UNKNOWN.
    - Re-emergence: `0x00A64AC0` → `0x008AE6E0` → `0x00B2BB00` (a faction coming back into the game) resets
      its records both ways (`0x00B2BC10` / `0x00B72AC0`, which reset the religion factor through `0x00B1B540`), no
      broadcast of its own; in `spa_napoleon` a re-emerging `spa_spain` or `spa_portugal` (the liberation flag clear) then
      grants `spa_britain` military access with no end (`0x00B444B0`, -1 turns; the grant broadcasts). It
      does not when the region's previous owner (the second argument, region +0xF4, read by `TransferRegionToFaction`
      `0x00A64AC0` through `0x006649C0` before `AddRegionToFaction`) is `spa_britain`. The liberation path is
      modelled (§Destruction and re-emergence); this revolt-path step is not (BACKLOG).
    - The loaders `0x00B0A3A0` / `0x00B30A20`.
- **Victory options:** `victory::fill` gives every faction with options option 0 in a start position
  (the human is not known yet); `LoadedCampaign::set_human` keeps only the human's, as the other
  factions keep the empty default (the Coalition saves).
- **Map areas:** the CAI world's `CAI_WORLD_REGIONS[]` (loader `0x00BF0130` appends them to its list
  +0x130/+0x134 in file order; saver `0x00C09400`), imported as `World::ai_areas` (key + links). The
  eur start position has 101 (72 land regions, the seas, the theatres and `all`); 52 of the 72 land
  regions have the same neighbour set as `regions.esf`, 18 the same order; seas link to land and every
  link is two-way (test `ai_areas_on_the_europe_start`). A map without them uses its regions in id
  order and `region_neighbours`.

### Desires (worker director-rng 2026-10-12, static trace, CONFIRMED unless tagged)

- **Pool order** (`0x00BBC880`, by score; may1811's pool holds one army desire of each): uber 11000
  and teleport 10000 (no draws), region transfer 9000 (above), at war 8000, boost 7000, force-minimum
  6000, unit balancer 5000, unit upgrader 4000, XP 3000, replenish 2000, trade-theatre filler 1000
  (state 2 in the vanilla pool, which the pool skips). Ours: the rival scores once, then the ported
  steps behind `director.desires` (`director_desires`).
- **Served** (`0x00BB5160` / `0x00BB50D0`, used by boost, XP and replenish): not human (`0x00BB51B0`),
  still in the game (faction +0x824), not the rebels, not the pirates (`0x00C3FD50`). Factions are
  walked in the CAI faction list's order (`0x006E9F70` → `0x006F0690` → vt+0x14 → `0x00B2B7B0`, list
  +0x88 = `CAI_WORLD_FACTIONS`), imported as `World::ai_factions` (a map without it: id order).
- **RNG draws:** float range `0x00BBD6E0`, int range `0x00BBD6A0` (both on +0x60).
- **At war** (`0x00BAC560`): for each faction `f` (not human, in the game, not rebels/pirates), each
  `g` of f's enemy list (not human, in the game; the step has no `g == f` or rebel/pirate test on
  `g`), each unordered pair once per step (`0x00898B20`): rebels or pirates (`g`) lose without a
  draw; else a roll in [0, score(g) + score(f)] above score(f) makes `g` the
  winner. (winner, loser) replaces the first entry naming the same two in either order and is
  appended (`0x00BC5240`, list +0x34). PROVISIONAL: the autoresolver's use of the pairs
  (`CDIR_CVN_AUTORESOLVER_MODIFIER_*`) is not modelled.
  The enemy list (fix round, CONFIRMED by `0x00ABAFB0`): f's CAI relation belief (`0x00A74A20`, the
  info object of ctor `0x00A44C50`) list `+0x40` (count `+0x4C`, data `+0x50`; `0x00A649A0` /
  `0x00A649C0`), built from the CAI world's faction list (`0x00B2B7B0` → `+0x110`, count `+0x11C`,
  data `+0x120`: `World::ai_factions`) in its order, skipping `f`: when `f` or the other is the
  rebels (`0x00C3FE80`, `IsFactionWithoutRecord`) the other is appended (and to `+0x7C` when it is
  the rebels); else, when the other is in the game (`+0x824` clear), f's stance record towards it
  (`f +0x528` → `0x00B64C50`, `+0x790 → +0x10`) sorts it: 2 → `+0x2C`, 0 (war) → `+0x40`; `+0x78C` →
  `+0x54`, `+0x788` → `+0x68`, `0x00B16570` → `+0x7C`, `0x00B5DCC0` → `+0x94`, `0x00B5DCB0` → `+0x90`.
  So the list is `at_war(f, g)` in `ai_factions` order, ours (test
  `the_at_war_desire_walks_a_factions_enemies_in_the_cai_order`). The save loader `0x00A39F90`
  reads the same list in file order. The belief is a stored copy refreshed only when read while
  dirty; ours keeps it too (`DirectorState::beliefs`, `believes_at_war`: the stored list while
  clean, else the current stances; "The relation is a stored copy" above).
- **Boost** (`0x00BAC310`): the served factions sorted by rival score, highest first (MSVC
  `std::sort`); the first `min(max(floor(n × PERCENTAGE), MIN), MAX)` get their `AMOUNT` (faction
  int) credited to the treasury. No draw.
- **XP for armies** (`0x00BAE910`): every served faction has a counter (find or insert 0,
  `0x00BAE9AE`..`0x00BAEAA9`); chance `BASE_CHANCE + EXTRA_CHANCE_PER_TURN × counter` (lookups
  `0x00BAECBE` / `0x00BAED13`, clamped as the transfer's), roll at `0x00BAED7A`; above it the counter
  goes up by 1 (`0x00BAF0F1`). Else the armies with units no human sees are gathered (`0x00BA30B0` →
  `0x00BB5270`), weighted `n² − (rank sum / n)⁴` (integer mean, kept above 0); none: nothing changes
  (counter kept); else a roll in [0, total] (`0x00BAEE24`) picks one, each unit draws its rank's steps
  (`0x00BA3240` → `0x00BD4A60`: (min, max) table `0x01380978` {(0,3), (0,2), (0,1) ×7, (0,0)}, chance
  table `0x0138098C` {0.7, 0.6, 0.6, 0.5, 0.4, 0.2, 0.15, 0.1, 0.05, 0}; the first `min` steps count,
  each other draws a roll and counts when ≤ the chance) and the counter goes to 0 (`0x00BAF0C0`).
  PROVISIONAL: no unit experience in the model, so every rank is 0, and the steps are not applied.
- **Replenish for armies** (`0x00BADA40`): faction / army / unit allowances from
  `REPLENISH_UNITS_FACTION_ / MOBILE_ / UNIT_ARMY_ALLOWED` (floats of the faction or owner). Per served
  faction with allowance ≥ 1: the units needing it (`0x00BC0800`: men < `floor(max × ARMY_MIN_PCT)`,
  army not aboard a navy) with unit allowance ≥ 1, of its unseen armies with allowance ≥ 1
  (`0x00BA2A00`), are listed; then list-length times while the faction allowance ≥ 1, a unit is drawn
  (`int_range(0, len − 1)`), queued when its army's and its own allowances are ≥ 1 (all three −1) and
  taken off the list. After every faction, each queued unit still in need (`0x00BC06E0`) draws a
  fraction in [MIN_PCT, MAX_PCT] and gets `floor(max × fraction)` men. PROVISIONAL: the allowances'
  carry-over and save, MAX below MIN (setter `0x008AABB0`), the navy half.
- **Transfer garrison** (`0x00BA8400`): after a director transfer the receiver's new garrison size
  is `int_range(a + 3, 4a + 6)`, `a` = 1 for an AI receiver (4 to 10 units). It returns at once
  unless the region's owner is the receiver, and skips the draw and spawn while the region still
  has a garrison (CONFIRMED, director2 2026-10-15). CAI region `+0x12C` (`0x008CF5F0`) is its
  settlement object, one of two classes the loader `0x00BF0130` builds (ctors `0x00BF6F00` and
  `0x00BEDE70`); both build their `+8` part with `0x00BF8140`, which puts the `CAI_GARRISONABLE`
  (`0x00BF5160`, vtable `0x0138297C`) at `+0x34`. Its `+0xFC` is the garrison army's CAI mobile and
  `+0x100` a navy garrison (setters `0x00C543C0` / `0x00C54410`; the mobile's back link `+0x134`
  is set by `0x00C54470`, which with 0 clears the garrisonable's field); `0x00C3B7A0` tests `+0xFC`.
  The game keeps the CAI links in step through listeners the CAI interface registers on the
  model's broadcasters (`0x00A6FE40`); a broadcaster's vt+8 is called directly, so each update is
  synchronous. In `0x00B58A30` the garrison army is deleted, then its commander (`0x00A0C6F0` →
  vt+4 → `DestroyCampaignCharacter` `0x0099D2D0`), which fires model `+0x948` unconditionally; the
  CAI interface's `+0x1C` listener (`0x00A8DE80`) removes the CAI mobile (`0x00C4E930` →
  `0x00C4F6D0`, which first calls `0x00C54470(0)`). So by `0x00BA8400` the old garrison's link is
  gone and the skip never holds on the director path; ours tests `region.garrison` after the
  capture. Other listeners: a fort/slot removal (`0x00A8DF40`, fired by the fort destructor
  `0x00AF71C0` on model `+0x78` unless model flag `+0xFC5`), an army's garrison change
  (`0x00A8E320` → `0x00A79EA0`). The army it raises (CONFIRMED from the disassembly, director2
  2026-10-15; raised in ours since director3, `director_transfer`): candidates are the 32-byte entries of the region's
  unit list (`0x00C3C340`, `+0x1B4` entries, `+0x1B0` count) whose record (`+0x18`) has a non-zero
  unit key (`+0x9C`) and passes `0x00BAF9A0`(receiver, region, record), sorted by `0x00B7F510`;
  each of the N units is built by `0x0087FEA0`(campaign faction, key, 0, 0, 1) cycling through the
  sorted candidates; the army is created by `0x008D25F0`(campaign faction, units, settlement
  position from `0x00C3C920`, 1, 0, 1, 1); its character (army `+0x74` via `0x00898E80`) is put
  in the settlement through `0x00C1BEF0`(settlement) vt+0x88 (character, 0, 0, 1, 1, 0), then
  `0x009D3B60`(character, 0, 0), then the `+0x5E8` broadcaster of the object at the director's `+0` is told of the army.
  `0x00C3C330` is the `CaiFaction` → `CampaignFaction` getter (`+0x168`).
  Director3 (2026-10-15, CONFIRMED unless marked):
  - The count is drawn before the candidates are gathered; the gather draws nothing. With no
    candidate the draw stays spent and nothing is raised.
  - The unit list is the region's recruitable list (region `+0x1A8`), which the capture
    (`CaptureSettlementForDirectorTransfer` `0x00B58A30` → `0x00A6AE40`) rebuilds for the new owner
    (`0x00C3C340` gives the CAI region's game region, `+0x1A0`; entries have stride 0x20, flags
    `+0x14` and record `+0x18`, as `MergeRecruitableEntrySorted` `0x00B08E30` writes them; ours
    `recruitable_units`). Record `+0x9C` is the land stats record (set for land units), `+0xA0` the naval one.
  - `0x00BAF9A0`: for a land unit, the unit passes when the region (if the receiver owns it) or a
    region across one of its CAI boundaries (`GetCaiRegionBoundariesBegin`/`End`,
    `GetBoundaryOtherRegion`) that the receiver owns has it in its land recruitment belief. A
    naval-only record goes to `0x00BAFAF0` (never reached here). The belief comes from `0x00D5F720`
    (cached per faction view, rebuilt through `0x00CCCF70` when `IsBeliefDirty` `0x00CF1170`).
    Membership is `0x00D5BE50` → `0x00D5BDB0`, a scan of the belief's land set (belief `+0xFC`:
    count `+0x28`, data `+0x2C`, 16-byte entries).
  - The region belief is rebuilt by `0x00D9C9D0` from two parts:
    - When the region has a settlement (`0x00C3E1E0`), the settlement belief (`0x00D5F930`): the
      region's priced land entries whose flags hold nothing but TOO_DEAR (0x02).
    - The belief (`0x00D5F7D0`, rebuilt by `0x00D9CD60`) of each slot that passes `0x00C3F480`,
      i.e. one with a recruitment manager (model slot `+0x1E4`, our `RegionSlot::port`):
      - A slot of CAI building slot type 3 lists its port manager's units (`+0x1E8` vt+0xC), keeping
        ships only (`+0xA0`), so it adds no land unit. The type is set by the `CAI_BUILDING_SLOT`
        constructor `0x00BE6EF0` from the model slot's type key (`+0x1DC` → `+0x44`), and "port"
        maps to 3. The ESF reader is `0x00BE6DC0`; the type sits at the building slot's `+0x100`,
        reached through CAI slot `+0x16C` (`0x0080DC60`); thunks `0x00C55560` / `0x00C1BF00`. The
        exe hardcodes the "port" key in `0x00BE6EF0`; no table marks a slot type as a port, so ours
        names it in `director.rs` `PORT_SLOT_TYPE`.
      - Any other slot, when its building (`+0x1E0` → `+8`) has health ≥ 100 (`+0x18`), adds the land
        entries of its `0x00B43CA0` list whose flags & 0xFD are 0. NO_TECHNOLOGY entries are
        skipped outright, and OCCUPIED keeps a unit out.
  - The sort `0x00B7F510` ranks by the unit record's category code (`+0x1C`): infantry 3;
    cavalry, dragoons, elephants and camels 2; artillery 1; others 0. The highest rank goes first.
    It is the library introsort, with partition `0x00B7D810` and the comparator inlined.
  - Unit and army construction are the same calls as the land recruit placement `0x008ECAD0`
    (`0x0087FEA0(.., 0, 0, 1)`, `0x008D25F0(.., 1, 0, 1, 1)`). `0x008D25F0` → `0x008D24B0` →
    `0x00870E60` with character argument 0 makes no general. So ours places each unit with
    `spawn_recruited_unit`: the first raises a colonel's army garrisoned in the settlement, and the
    rest join it. Ours emits no `UnitTrained` for them.
  - Known difference: the original puts every unit in its one army whatever the army cap. Ours
    honours the cap (vanilla's 20 is above the most drawn, 10), so a lower cap set by a mod starts a
    second army.
  - INFERRED: ours builds the beliefs from the current state, while the original reads its cache,
    which is fresh only if every change the belief reads marked it dirty (`MarkBeliefDirty`
    `0x00CF0AB0`, 147 call sites, which propagates to dependents through `0x00CF0B10`). Which
    changes mark a region recruitment belief is not traced.
- **Transfer and the old owner's forces** (director2 2026-10-15, CONFIRMED): the settlement
  handover `TransferSettlementOwnership` `0x00B449F0` (assault `0x00B58560`, deal and liberation
  `0x00B58A10`, the director `0x00B58A30`) calls `TransferRegionToFaction` `0x00A64AC0` with its
  fourth argument 0. With that argument 0, for each faction of the world's list (campaign
  `+0xF5C`) other than the new owner (`0x00898B80`), with a capital (`+0x72C`) and
  still in the game (`+0x824`), it calls `ExpelFactionForcesFromRegion` `0x008D9DC0`(faction,
  region). That moves only the faction's land armies standing in the region (`0x009D3A40`, the character's `+0x2A8` holder vt+0x3C; position
  `0x00A06D80` → `0x00A17D60`, region test `0x00A075B0` → `0x00B4D7B0`) and its navies docked in
  the region's port when not at war with the new owner (`0x009D3A80` vt+0x44, undocked by
  `UndockCharacterFromPort` `0x00A8C0B0`, `0x00B377E0`); the "may stay" test (`0x00B16570`, below)
  only orders the candidates and silences the move notice. **Agents are not ejected**:
  a character without an army or navy stays where it is (ours: test
  `a_transfer_leaves_the_old_owners_agents_in_place`; our agents never stand in a settlement).
  Modelled (transfer3, 2026-10-15, CONFIRMED from the disassembly): the rule is
  `expulsion::expel_faction_from_region` behind the seam `region.expulsion`
  (`CampaignSeams::region_expulsion`), applied by `CampaignModel::expel_from_region`; its callers are
  `change_region_owner` (`expel_after_owner_change`, every handover), `accept_deal` and
  `end_protectorate_tie` (tests `campaign/tests/transfer.rs`):
  - **Where it runs:** inside `0x00A64AC0`, after the research stop `0x008B4AB0`, the early return
    when the owner does not change, the queue drains, the diplomacy event `0x00B62120`,
    `RemoveRegionFromFactionAndMoveCapital`, `AddRegionToFaction` and the re-emergence handling;
    before `0x00AA16E0`, `RebuildRegionRecruitablesAndEffects` and `UpdateRegionTownWealth`. In
    ours: the end of `change_region_owner`, after the capital change and the trade routes
    (`0x00BBE1A0` is called at `0x00A64E87`, the expulsion loop at `0x00A650E4`), before `take_settlement`
    puts the capturer inside and before `end_if_capital_lost`. The factions are walked in the world
    list (campaign `+0xF5C` → `+0x2C` / `+0x30`, which holds no rebels: the rebel faction is
    `+0x1C`), so the rebels' forces are never expelled; a faction without a capital (`+0x72C`) is
    skipped too.
  - **"May stay" flag** (N = the region's owner, already the new one; F = the expelled faction):
    set when either has no record (`0x008CEEF0`), N is F, N's relationship record towards F
    (`N +0x528` → `0x00B64C50`) passes `0x00B16570`, or N is at war with F (`0x008CE9B0`).
    `0x00B16570` on a record: true when its military access turns (`+0x78C`, #3) are not 0, its
    `allied_in_war_against` list (`+0x7D0` count) is not empty, its stance is protectorate
    (`IsStanceProtectorate`) or its stance index (`+0x790 → +0x10`) is 0 (war). The flag (a) puts
    the region itself first in the candidate list (the caller writes the region into the search
    context's `+0x24` slot after `0x008C6010`, `0x008D9E72..0x008D9E80`) and (b) silences the move
    notice `0x009FB680`.
  - **Candidates** (`0x008C6010` builds, `0x008DCB30` orders; entries region + point): (1) the
    region itself at the army's own position, when "may stay"; (2) F's regions (faction `+0x778` /
    `+0x77C`, in its list's order: see "Faction region list order") in the region's theatre or with an overlapping theatre box, other
    than the region; (3) the world's regions (`+0xF5C` → `+0x34` → `+0x10` / `+0x14`, the region manager's
    order; ours `World::regions_in_manager_order`) in the same
    theatre, owned by another faction not at war with F whose record towards F passes
    `0x00B16570`, other than the region; (4) F's capital, only when it lies in another theatre.
    Lists (2) and (3) are each sorted (std::sort `0x00868A30`) by squared distance from the army's
    position to the region's nearest outline point (`0x008AF750`). With each map one theatre (as
    `nearest_capital`), (2) is all of F's other regions and (4) never happens. The context's list
    holds 12-byte entries (region, x, z); `0x008DCB30` copies entry (1) first, then appends the
    sorted (2) and (3), then (4). Ghidra (polish44, 2026-10-17): the context is the struct
    `RegionSearchContext` (0x2C: own-region vector `+0x00..+0x0C`, access-region vector
    `+0x10..+0x1C`, `pCapitalRegion +0x20`, `pStayRegion +0x24`, `pLeftRegion +0x28`), the
    ordered list `RegionCandidateList` of `RegionPointCandidate` {region, x, z}; the "may stay"
    flag is `fMayStay` in `ExpelFactionForcesFromRegion`, and `DiplomacyRelationRecord +0x78C`
    is `nMilitaryAccessTurns` (was `dwTradeAux`; the 5 sites checked are all access code, the other ~40 uses unchecked):
    round-end decrement `0x00B291BF`, cancel `0x00B67C39`, grant `0x00B44550`, treaty text
    `0x00B7546A`, the AI's access goals `0x00CC02E9`, which skip a record at -1 indefinite).
  - **Land armies:** each character of F (faction `+0x768`, count `+0x764`) whose army (holder
    vt+0x3C, `0x009D3A40`) has a map position (`0x00A17D60`: its own, or its carrier's when
    aboard) inside the region's polygons (`0x00B4D7B0`). The candidates are tried in order until
    one places it: one in the region itself through `0x00B37670`, any other through `0x00B35570`,
    both with 100.0. Both are wrappers over the path grid's layers, each layer prepared for F
    (`0x00B06280`); `0x00B37670` also lifts the character's own footprint off the grid around the
    search (`0x00AF59B0` / `0x00B0C830`, put back by `0x00B66720`). The flood `0x00B35640`(point,
    region, radius) is a best-first search over the region's cells from the point's cell (no cell
    there: fails), 8 neighbours, writing the nearest valid position (`0x00B41A60`); it succeeds when
    that position is closer than the radius. Then, when the army has a residence
    (army `+0x9C`): N at war with F and the residence's vt+0x54 returning 0 leaves the army
    untouched where it is; otherwise the army leaves the residence (vt+0x90: a settlement
    `0x00B0D610`, which unlinks the garrison handle `+0xC0`, a region slot `0x00A64450` → `0x00B0D610`; a fort `0x00B0D570` also
    tears the fort down), placed or not, **whether or not it "may stay"** (the flag only orders
    the candidates; ours: test `a_garrison_that_may_stay_still_leaves_the_settlement`). vt+0x54
    (fix round, 2026-10-15, CONFIRMED from the vtables) returns the residence itself only for a
    settlement (vtable `0x0137C9A4`, `0x004470B0` `mov eax,ecx`; constructor `0x00AF48D0`, region
    `+0xF4`); it returns 0 (`0x00461180`) for a fort (`0x0137D3E4`, constructor `0x00AEB4A0`), a
    region slot (`g_apfnRegionSlotVtbl` `0x0137695C`, constructor `0x00A4D990`: ports and the other
    slot buildings) and their two base classes (`0x0137C7F0`, constructor `0x00AEC210`; `0x0137C8B4`, constructors
    `0x00AF5090` and `0x00AF4B00`, the latter reading `FORTIFICATIONS_BLOCK`). No RTTI names them. So the
    exception is an army garrisoning anything but a settlement while N is at war with F. A placed
    army is teleported (`0x0096C8D0`) and, unless "may stay", the notice fires.
  - **Out of the game:** the function itself returns at once when F is out of the game (faction
    `+0x824`, `0x008D9DD5` / `0x008D9DDC`), so every caller gets it (ours: in
    `expel_faction_from_region`; test `a_faction_out_of_the_game_is_not_expelled`).
  - **Embarked armies** (INFERRED): the region test (`0x00A075B0` → `0x00B4D7B0`, campaign
    `+0xF64`'s layers `+0xB4` / `+0xB8`, each `0x00B4D800`) takes the path-grid polygon under the
    position (`0x00A17D50`) and compares its region index (`0x00B62280`) with the region's
    (`0x00B62290`); an index past `+0x140` instead names an entry of the shared list `+0x154`
    (16-byte entries holding region ids, compared with region `+0x10` → `+0xA4`). An army aboard
    stands at its carrier's position: at sea, or for a docked carrier its port's exit
    (`0x00A99A60`), both on sea polygons, so ours never counts an embarked army. Not traced: whether
    a shared-list polygon under a port exit can name the land region (debugger sitting).
  - **Navies:** for every character, when N is not at war with F and its navy (`0x009D3A80`) is
    docked in a port (`+0xE0`) of the region: undocked (`UndockCharacterFromPort` `0x00A8C0B0`),
    then placed at sea from the port's exit point (`0x00A99A60`) by `0x00B377E0` (mode 2, 100.0);
    teleported on success, else it stays where it is, undocked.
  - **Positions** (`0x00A17D60`): a garrisoned army stands at its settlement, an army aboard at its
    carrier's position, a docked navy at its port's exit (`0x00A99A60`), anything else at its own.
  - Agents are never moved. Our stand-ins (PROVISIONAL, in `expulsion.rs`): the candidate's point
    is the centre of the region's path-grid cell nearest to the army (the settlement without the
    map) for the outline point; the flood is `PathGrid::nearest_place_within` (the point itself if
    its cell fits, else the nearest fitting cell of the region closer than 100; no footprints in the
    model); F's regions go in in its list's order (`World::regions_in_list_order`, CONFIRMED below,
    "Faction region list order"), the third parties' in the manager's order (`World::regions_in_manager_order`),
    but our sort is stable, std::sort is
    not; the port's exit is the harbour's dock point, else the port slot; no theatres in the model
    (every shipped map is one theatre, CONFIRMED, see "Theatres CONFIRMED a no-op"; a map with several needs the
    exe's theatre filter). The move notice `0x009FB680` (message 0x119) is not modelled (BACKLOG).
    An army that no candidate places stays where it stands, out of the settlement; the model
    garrisons armies only in settlements, so the at-war exception for other residences never
    applies.
  - **Other callers** (CONFIRMED): the deal `AcceptCampaignNegotiationDeal` `0x00C114B0`, after
    all records: the regions record (`+0x94`) expels the proposer from each offered region (`+0x20`)
    and the recipient from each demanded one (`+0x10`), with no capital test (modelled); the peace
    record (`+0x184`) expels the proposer from all of the recipient's regions and the recipient
    from all of the proposer's (not modelled: our deals hold no peace record; PROVISIONAL note in
    `accept_deal`). `EndProtectorateTie` `0x00B64B10`: after the neutral stance and the tribute /
    income clear, the patron leaves each of the protectorate's regions unless its skip flag is set
    (`0x00B104E0` passes 0, `0x00B0CA20` its war flag; modelled, region ids for the list order), then `0x00A0B570`,
    the path-grid obstacle refreshes `0x00B795A0` / `0x00B67490` and the broadcast. The loss of military access expels the target
    from all of the owner's regions: `0x00B0CA20`, `0x00B13A30`, `0x00B0D0C0`, `0x00B67BD0` (cancel
    access), `0x00B29170` (timed access running out): modelled, see "Access-loss expulsions" below. `0x00B4F090` (both sites, `0x00B4F126` and `0x00B508C5`): the liberation expulsion, modelled. `0x00936B37` / `0x00936B76`: a mutual
    expulsion inside a command handler Ghidra has not defined; which command is UNKNOWN.
    `TransferSettlementOwnership` always passes 0; only the start-position load `FUN_00a46a70`
    passes 1 (no expulsion). ORIGINAL BUGs: none found.
  So the earlier "moves no army" PROVISIONALs of `battles.rs`, `capture.rs`, `commands.rs` and
  `director.rs` are gone: a foreign garrison now remains only when its faction has no capital (or
  comes from a loaded save).
- **Access-loss expulsions** (accessloss 2026-10-17, CONFIRMED by decompile and disassembly). Each of
  these runs the same loop on one relationship record (owner O → target T): `0x008D9DC0`(T, region) for
  every region of O's list (faction +0x778, in its order), then `0x00A0B570` per character of T,
  `RefreshFactionPathGridObstacles` `0x00B795A0` on both factions and
  `CampaignPathGrid::ReapplyPathGridFactionView` `0x00B67490` (path-grid obstacle refreshes, not visibility;
  ours builds the obstacles at search time in `plan_path`, so it needs no refresh).
  Ours: `CampaignModel::expel_from_lands_of(T, O)` (`expulsion.rs`), the `region.expulsion` rule per
  region, in O's list order (`World::regions_in_list_order`, accessloss2 2026-10-20, CONFIRMED; it
  matters only for a force that may stay but cannot be placed where it stands). Whether a force leaves
  is the rule's "may stay" test, read at the moment of the call.
  - **Faction region list order** (accessloss2 2026-10-20, CONFIRMED by decompile). The list is the
    owned-list object at faction +0x76C (count +0x778, data +0x77C). A gain appends: `AddRegionToFaction`
    `0x008AE270` → `0x008CFA40` → `0x008CF7D0` pushes the region at the end (when its owner link is
    empty) and sets the owner; its only caller is `TransferRegionToFaction` `0x00A64AC0`. A loss removes
    in place: `RemoveRegionFromFactionAndMoveCapital` `0x008BA580` → `0x008B0210` erases the entry and
    shifts the rest down. At a load the region array loader `0x00A4CE20` (from the world loader
    `0x0090BEB0`, after `FACTION_ARRAY` and `REBEL_FACTION`) reads `REGION_MANAGER/REGIONS_ARRAY` in file
    order, and each region's constructor `ConstructCampaignRegionFromEsf` `0x00A45950` resolves the saved
    owner (`0x0105AC60`) and appends itself to the owner's list (`0x00A46008`: `0x008CFB40` → `0x008CF6B0`
    with the region's owner link, region +0xF4). The writer `0x00A52FE0` saves `REGIONS_ARRAY` in the
    manager's vector order (manager +0x10 count / +0x14 data, filled only by the loader), and the file
    holds no per-faction list: so each faction's list after a load is the file order filtered by owner,
    and a save and reload in the original puts a gained region back in its file place. (The dev set-up
    path `0x0090C4A0` → `0x00A4D050` → `0x00A46A70`, which hands regions over through `0x00A64AC0` with
    its fourth argument 1, runs only under the dev preprocessor `0x00A19350`.) Ours: `World::region_order`
    (the importer's file order, kept by own saves from format version 11; an older one takes the start's)
    and `World::region_gains` (each owner change in `change_region_owner` moves the region to the end;
    not saved, and left out of `saved_state_hash`). The ESF writer patches the array in place, so the file
    order survives our saves too (test `vanilla_save_region_order_is_the_manager_order`: in the evidence save
    `orig_fr_may1811.save` the file order of 31 regions has 12 descents from id order, so walking by id was
    wrong). Ghidra names (V5): `ConstructRegionManagerFromEsf` `0x00A4CE20`, `SaveRegionManagerRegions`
    `0x00A52FE0`, `AttachOwnedLinkToOwnerList` `0x008CF6B0`, `AppendOwnedLinkToOwnerList` `0x008CF7D0`,
    `RemoveOwnedItemFromOwnerList` `0x008B0210`.
  - `0x00B67BD0` (cancel access, record O→T): after the grievance and #3 = 0, always. Callers: the
    command `0x00B67B20` and the war's abuse `0x00B0DAF0`. In the war `0x00B26700` the stance turns to
    war (`0x00B27250`) before the abuse, so there the target may stay; ours had declared war after the
    abuse and now declares it first (`declare_war_rules`).
  - `0x00B29170` (round end, record O→T): when timed access counts down to 0 (from 1), right after the
    access step and before the dead-item clean-up of #17, friendship and the war balances (so the
    balances see the moved armies; ours moved the step first in `diplomacy_round_end`). When the
    count-down leaves 1, `0x00A0E630` posts T message 0x76 (`military_access_prompt`), below.
  - The access prompt (accessprompt 2026-10-20, CONFIRMED by decompile and disassembly; V5 in Ghidra):
    `SendMilitaryAccessPromptMessage` `0x00A0E630` (`this` = the message manager, model +0xFA0;
    recipient, granter; `RET 0x8`) builds the same 0x28-byte text message as `SendCeasefireDeclaredMessage`
    (vtable `0x0136F9A8`, title = the granter's name +0x518, empty detail, flags 0) and posts it with
    `0x009D3250`(recipient, 0x76, 0x76, msg, 0): entry 0x76 of the type table `0x0145C530` is the string
    `0x013CFD4C` `military_access_prompt`. Its only caller is `UpdateRelationRecordForRoundEnd` `0x00B29170`
    (`0x00B2920D`): after the timed count-down, when the new count is 1 (so a grant of 2 or more turns,
    the round its count goes 2 → 1; a 1-turn grant and indefinite access −1 never post it), recipient =
    the record's target (record +4), granter = the record's owner (record +0, the faction whose
    `0x00B29100` walks its record vector in order, stride 0x848). No human or major test (the display
    gates of `0x009D3250` apply). The message asks nothing back: it is the plain text message class,
    its `message_event_strings` rows (four subcultures) use layout `standard` with pictures
    `map-region.tga` / `army.tga` (as `alliance_broken`'s `standard`), and the text
    (`message_event_text_text_military_access_prompt`) tells the player to enter negotiations; nothing
    in the model reads the post back, so renewing is the ordinary diplomacy offer and letting it lapse
    is the next round's 1 → 0 expulsion. Ours: `diplomacy_round_end` returns
    `CampaignEvent::Message { recipient: T, message: CampaignMessage::MilitaryAccessPrompt { granter: O } }`
    in the turn's events (`TurnStep::Economy`), walking the records in `World::record_targets_in_order`
    (before: id order). Test `timed_access_with_one_turn_left_posts_the_access_prompt`
    (capture_diplomacy_tests).
  - `0x00B13A30` (alliance break, breaker's record A→B) and `0x00B0CA20` (the other's record B→A), both
    with `0x00B13840`'s war flag (1 only from the war path `0x00B27144`): when #17 is not empty, its
    first item's saved access goes back to #3 and the list is cleared; if the flag is 0 and that access
    is 0, the record's target leaves the owner's regions. `0x00B0CA20` first tests B→A stance 4 (B is
    A's protectorate) and calls `EndProtectorateTie`(B, A, war flag): the patron A leaves B's regions
    unless the flag is set. This happens before B→A's own #17 is cleared, so a list there still lets A
    stay at that step. A protectorate breaking away from its patron runs no tie end (A→B stance 4 is
    not tested by `0x00B13A30`; B→A is 3), so the patron's forces stay unless access was suspended:
    the exe's behaviour, kept (no contradiction with its data shown). Ours: `break_alliance_with`.
  - `0x00B0D0C0` (peace, record O→T, enemy E): the first #17 item for E gives its saved access back to
    #3 and is erased; if that access is 0, T leaves O's regions; no item, nothing. The peace
    `FactionDiplomacyManager::MakePeaceWithFaction` `0x00B262C0` runs it
    only on the proposer's manager (`this` = the first faction's +0x528: `force_make_peace` `0x00936710`,
    `MOV ECX,[ESI+0x528]`): for each of the proposer P's records with stance 2 / 3 / 4 (ally Y), on P→Y
    then on Y→P. Ours restored both sides' allies, for both factions; now P's side only
    (`make_peace_rules`). `0x00B262C0` also runs `ClearCommerceRaidsBetweenFactions` `0x00A25570` per force of each side
    towards the other (per its plate: a commander with a raid on the other side loses all his raids, see the commerce raid ends). Its other two steps (accessloss2 2026-10-20, CONFIRMED by decompile; V5 in Ghidra):
    a commander whose force (`0x009D3A40`) besieges a settlement of the other side (siege object +0x94, its +0x7C
    target's faction vt+0x14) ends the siege, `EndForceSiegeOfSettlement` `0x008BD550` (destroys the siege object,
    clears +0x94, calls the untraced `0x00B084F0` on the target's character, then unless model flag +0xFC5 a price
    update `0x00BCB020(0)` and `RebuildRegionRecruitablesAndEffects` on its region, then event 0xDF); a navy
    (`0x009D3A80`) blockading a port of the other side (+0xE8 port-slot handle, the slot's +0x70 owner) lifts it,
    `LiftPortSlotBlockade` `0x00A784E0` (unlinks slot+0x68, clears the building's +0x1E8→+0x58 byte, price update,
    `0x00B084F0` on a docked navy's character, events, slot +0x74 = 0; see "Port blockades" under Trade). The siege
    call (`0x00A255C5`) comes before the blockade call (`0x00A25627`). Ours ends the siege (`end_sieges_on_peace`,
    siegeattr; see "Siege ends") and then lifts the blockade (`lift_blockades_on_peace`, portblockade).
    `0x00B084F0` stays untraced. Its other steps (polish50 2026-10-17, CONFIRMED by decompile; V5 in Ghidra):
    the two stance records (`0x00B26590`), the model's +0x390 peace event, the path-grid obstacle
    refreshes of both sides, the trade price update `0x00BCB020`(0), then, unless its second argument
    (read from `force_make_peace`'s command stream; `MakeProtectorateOfPatron` `0x00B105C0` passes 0) is
    set, campaign text message 0x2F to each side about the other (`0x009D1F20` on the message sink
    +0xFA0); `0x008B3780` for each human side; in `spa_napoleon` only, a peace between spa_spain or
    spa_portugal and spa_britain makes the Iberian side grant Britain indefinite military access
    (`GrantMilitaryAccessToFaction` `0x00B444B0`(britain, −1), a hardcoded key check); last the +0x3A8
    diplomacy-changed event.
  - Peace messages and access grant (peacemsg 2026-10-20, CONFIRMED by decompile and disassembly; V5 in Ghidra):
    `SendCeasefireDeclaredMessage` `0x009D1F20` (`this` = the message manager, model +0xFA0; recipient, other)
    builds a 0x28-byte text message (vtable `0x0136F9A8`, title = the other faction's name +0x518, empty
    detail, flags 0) and posts it with `0x009D3250`(recipient, 0x2F, 0x2F, msg, 0): type 47 of the message
    type table `0x0145C530` = `ceasefire_declared`. `0x00B262C0` calls it to the target about the proposer
    (`0x00B2645D`), then to the proposer about the target (`0x00B26474`), with no human or major test (the
    display gates of `0x009D3250` apply: none for the rebels). The suppress flag is the `CCQ_MAKE_PEACE`
    command's byte (command vtable `0x0136ABC4`, sender `0x00983850`, id `0x015C268C`, registered by
    `0x00421C80`): `HandleMakePeaceCommand` `0x00936710` reads proposer, target, the byte; needs a good
    stream, both factions, different, at war (`0x008CE9B0`); then `MakePeaceWithFaction` on the proposer's
    +0x528. The script `force_make_peace` (`0x0097AC90`) sends 0 (then `CCQ_MOVE_ARMIES_OUT_OF_REGIONS`,
    `0x009838B0`); `force_make_protectorate` (`0x0097AA20`) sends 1 when the two are at war, its only setter;
    `MakeProtectorateOfPatron` (`0x00B10685`) and the negotiated deal's peace item (`ApplyPeaceRecordOnAccept`
    `0x00C18B60`, created 2026-10-20, vtable slot `0x01381FC0`, `PUSH 0` at `0x00C18B7C`) call `MakePeaceWithFaction` with
    0. A transient argument: no model field, never saved. The `spa_napoleon` step compares the campaign key
    (campaign link +0x30), then the keys (`GetFactionKeyOrRebels`): target ∈ {spa_spain, spa_portugal} and
    proposer = spa_britain → the target grants; else proposer ∈ {spa_spain, spa_portugal} and target =
    spa_britain → the proposer grants; `GrantMilitaryAccessToFaction` `0x00B444B0`(britain, −1). Ours:
    `make_peace_rules(a, b, suppress_messages)` appends `CampaignMessage::CeasefireDeclared` to b then a
    unless suppressed (`CampaignCommand::MakePeace::suppress_messages`; `force_make_peace` and the AI pass
    false); the grant is data, `CampaignFeatures::peace_access_grants` (`PeaceAccessGrant { granters,
    grantee, turns }`, filled for `spa_napoleon` by `ntw_campaign::features::original`), run after the
    messages through `grant_military_access`. `force_make_protectorate`: the next entry.
    Tests: `peace_posts_ceasefire_messages_unless_suppressed`, `a_peace_access_grant_row_gives_the_grantee_access`
    (capture_diplomacy_tests), `the_original_keys_get_the_exe_switches` (ntw_campaign features).
  - `force_make_protectorate` and its two commands (forceprot 2026-10-20, CONFIRMED by disassembly; V5 in
    Ghidra). The binding `HandleScriptForceMakeProtectorate` `0x0097AA20` reads two faction keys (the
    reader `0x01055760` pops from the stack top, so the first script argument is read last): the first is
    the patron, the second the protectorate (vanilla: `ita_napoleon\scripting.lua` 1005,
    `force_make_protectorate("ita_french_republic", "ita_piedmont")` in `subjugate_piedmont_timer`). It does
    nothing unless both exist and differ. At war (`0x008CE9B0`, the rebels count as at war) it first queues
    `CCQ_MAKE_PEACE` (patron proposer, suppress byte 1). Unless either side has a patron
    (`FactionHasPatron` `0x008CEEA0`, faction +0x754 not 0) it queues `CCQ_MAKE_PROTECTORATE` (type id
    `0x008FD2A0` → `0x015C3650`, registered by `0x00421D00`; payload vtable `0x0136AB84`, its writer
    `0x00953A70` the two faction pointers). Then always `CCQ_MOVE_ARMIES_OUT_OF_REGIONS` (sender
    `0x009838B0`, type id `0x008FD2D0` → `0x015C3A2C`, registered by `0x00421E80`) with patron, protectorate.
    `HandleMakeProtectorateCommand` `0x009367A0` decodes patron then protectorate; needs a good stream, both,
    different, not at war, neither with a patron; then `0x00B105C0` on the protectorate's diplomacy (+0x528)
    with the patron and the deal's peace step 0 (`PUSH 0` at `0x00936817`).
    `HandleMoveArmiesOutOfRegionsCommand` `0x00936AB0` (no function in Ghidra before this branch) decodes a
    and b; needs a good stream and both; then b's forces leave each region of a's list (count +0x778, data
    +0x77C; `0x008D9DC0` per region), then a's each of b's. No stance, same-faction or turn test and no zone
    flood after (each placement rebuilds what it moves). `force_make_peace` `0x0097AC90` queues the same
    command after its peace when the two are at war (ours does not yet, BACKLOG).
    Faction +0x754 is the patron: `SetFactionPatron` `0x008AF590` writes it (passing the old and then the new
    patron's +0x6F8 object with the faction to `0x008F9860`, two calls on it, not traced), from `0x00B104E0` at `0x00B1058B` right after stance 4 is set and from the
    tie's end `0x00B64B10` at `0x00B64C2D` with 0; `0x008E2A90` (from `0x0090BEB0`) and the loader
    `0x0087A190` also write it, `DestroyFactionAndResetRelations` `0x008AB6C0` clears it; `GetFactionPatron`
    `0x008E2A80` reads it. So "has a patron" is the model's `has_patron` (`0x00B4D9A0`, a record with
    stance 4). This settles destruction.rs ~128 (the patron reading) and negotiation.rs's war option
    (`0x00C1A890`: with a patron, war only on the patron or on a faction the patron is at war with; not
    modelled yet, BACKLOG). Ours: `CampaignCommand::MakeProtectorate` (`make_protectorate_command`, then the
    existing `make_protectorate`) and `CampaignCommand::MoveArmiesOutOfRegions`
    (`move_armies_out_of_regions`, `expel_from_each_region_of` both ways); the script applies the three in
    the exe's order, their events queued for the host. The peace step sends `CampaignCommand::MakePeace`, which
    has no turn test (below). All three go through `apply`, so a pending battle refuses each of them (no
    half-applied result).
  - **Peace has no turn test on any path (peaceturn, 2026-10-20, CONFIRMED, disassembly).**
    `MakePeaceWithFaction` `0x00B262C0` has exactly three callers, and none of them, nor anything before them,
    tests whose turn it is: (1) `HandleMakePeaceCommand` `0x00936710` (good stream, both factions, different, at
    war `0x008CE9B0`); `CCQ_MAKE_PEACE` is sent only by the scripts `force_make_peace` `0x0097AC90` (both keys
    found and at war, then `CCQ_MOVE_ARMIES_OUT_OF_REGIONS`) and `force_make_protectorate` `0x0097AA20`, both
    through `SendMakePeaceCommand` `0x00983850` (renamed; type id `0x015C268C`, the payload written by the
    command's vtable slot 0); its registration (`0x00421C80`) carries the same flag word 0 as every other
    command (e.g. `CCQ_DIPLOMACY_ACCEPT_DEAL` `0x00420600`). (2) `MakeProtectorateOfPatron` (call at
    `0x00B10685`). (3) `ApplyPeaceRecordOnAccept` `0x00C18B60` (created; the deal's peace record, deal +0x184,
    vtable slot +0x3C): if the record's +0x14 byte is set, the proposer's (`0x00C4BC20`) diplomacy +0x528 makes
    peace with the recipient (`0x00C4CE90`), suppress 0; no turn, at-war or same-faction test. It runs from
    `AcceptCampaignNegotiationDeal` `0x00C114B0`, called by the human's `ExecuteDiplomacyAcceptDealCommand`
    `0x00932E40` (only the reader's error byte checked) and the AI's `EvaluateProposedNegotiationDealAI`
    `0x00AA5ED0` (at `0x00AA62FF`): the exe's AI makes peace only through deals. Ours: the `turn_test` bool is
    gone; `CampaignCommand::MakePeace` never checks the turn, for the scripts and the AI alike, and the deal's peace
    item (the human's diplomacy) will call the same rule when it lands (one rule; a network sender check, if ever wanted, belongs to the network layer for every command, not
    to this command). Our deals have no peace record yet (negotiation.rs `accept_deal`, BACKLOG), so path (3) has
    no test of its own. Tests: `peace_holds_outside_the_proposers_turn` (economy_tests),
    `force_make_peace_outside_the_first_factions_turn_makes_peace` (ntw_script host),
    `the_ai_peace_is_the_models_make_peace_with_messages` (ntw_ai). Ghidra: `ApplyPeaceRecordOnAccept` and
    `SendMakePeaceCommand` documented per V5 (completeness 89 and 79; the rest is the structural `void *this`
    and untyped command payloads).
    ORIGINAL BUGs: none. Tests:
    `the_make_protectorate_command_follows_the_exe_handler`, `moving_armies_out_empties_both_sides_settlements`
    (capture_diplomacy_tests), `force_make_protectorate_makes_peace_then_the_protectorate`,
    `force_make_protectorate_outside_the_patrons_turn_returns_its_events`,
    `force_make_protectorate_with_a_battle_pending_changes_nothing` (ntw_script host).
  - ORIGINAL BUGs: none found. Tests: `tests/transfer.rs` (`cancelled_access_expels_the_target_but_not_at_war`,
    `timed_access_running_out_expels_the_target`, `a_broken_alliance_expels_a_side_given_back_no_access`,
    `peace_gives_back_the_proposers_allies_access_only`, `a_patron_breaking_with_its_protectorate_leaves_its_regions`).
- **CAI save writer and sea regions** (director2 2026-10-15): with the path grid's sea cells now
  carrying their sea region, the CAI writer (`crates/ntw_campaign/src/cai_world.rs`, the mobiles'
  `CAI_SITUATED`) writes a moved fleet's sea region into `CAI_SITUATED` #2 (and its theatres into
  #3), as the original's saves do (test `mobiles_are_situated_in_the_region_under_them`); before,
  a fleet at sea got no region there. Edge: the sea fill (`grid_from_pathfinding`) gives a region
  only to Water cells whose centre lies in a sea region's `regions.esf` triangles; a Water cell
  that `regions.esf` puts in a land region keeps no region, where the original would situate a
  fleet there in that land region.
- **Config parsers** (`cdir_configs` text values): float `0x004F38A0` (a `.` is required: `"10"`
  fails and reads as 0; `e`/`E` exponent with optional `-`, no `+`, no spaces), uint `0x004F37B0`
  (digits only, wrapping; `all` int lookup `0x00BD2920`), int `0x004F3720` (optional `-`; faction int
  lookup `0x00BD29F0`); faction float lookup `0x00BD2960`.
  Every vanilla row of a float variable has a `.` (merged database, test
  `vanilla_director_floats_all_parse`; e.g. `1.0`, `10.0`, `0.125`), so none relies on the parser's failure.

ORIGINAL BUGs (fixed in ours, no toggle):
1. `0x00A7A2E0`: the attitude mapping has branches for < 0 and > 0 but none for exactly 0, which
   falls through to 0 ("hostile") instead of the 0.5 both branches meet at. Ours gives 0.5.
2. `0x00BD21B0`: for the excluded factions it writes 0, then computes the score anyway and
   overwrites it. Ours keeps 0.
3. `0x00BD23F8`, `0x00BD24C0`: the two neighbour lists' "already listed" tests are inverted (a region
   is appended only when already present), so both lists stay empty, the proximity share is always 0
   and `PROXIMITY_PCT` is dead. Ours lists each linked area once.
4. `0x00BAE1F5`..`0x00BAE230`: the pick loop drops the front candidate while the roll is above 0,
   subtracting its weight, then takes the new front, so each candidate is picked with the previous
   one's weight (the first only on a roll of 0). Ours gives each candidate its own weight.
5. `0x00BAEE3F`..`0x00BAEEDA` (XP desire `0x00BAE910`): the same pick loop over the armies. Ours
   gives each army its own weight.

Open (PROVISIONAL in code): the siege test is never true (no sieges in the model; BACKLOG "Sieges on
the campaign map"); the transfer does not expel the other factions' armies and docked navies
(`0x00A64AC0` → `0x008D9DC0`, placement `0x008DCB30` untraced; the same for deals and liberation);
the desires' unmodelled effects above (autoresolver pairs, XP steps, replenish
allowances and navy half, the transfer's garrison); the teleport desire's effect
(`CDIR_CVN_TELEPORT_GENERALS_TO_FRONT`) and the force-minimum, balancer and upgrader gathers are not
traced, so they draw nothing in ours (the RNG caller `0x00BAC9C0` is not traced either), and the
desire states are not read from the file. The RNG sequence matches the original's until one of
those draws.

Ghidra names (V5 write-ups in the program): `DrawDirectorRandomFloat` `0x00BBD6E0`,
`GetCdirConfigFloat` `0x00BD29B0`, `FindCdirConfigValue` `0x00BD2A40`, `HasCaiRegionSettlement`
`0x00C3E1E0` (CAI region +0x12C; the siege test goes through that object), `IsSameComponentId`
`0x00CF0D30`, `GetBoundaryOtherRegion` `0x00C1BCE0`, `GetCaiRegionBoundariesBegin` / `End`
`0x00C43560` / `0x00C43570`, `IsFactionInHumanList` `0x00BB51B0` (the human list +0x14/+0x18; the
major-power test is `0x00C3FB20`), `IsRegionSeenByAnyHuman` `0x00BB55A0`,
`IsRegionTransferableByDirector` `0x00BB52E0`, `ComputeDirectorRelationValue` `0x00A7A2E0`,
`StepTransferRegionOwnershipDesire` `0x00BAE0A0`, `CollectRegionTransferCandidates` `0x00BA2B90`,
`UpdatePlayerRivalScoreBelief` `0x00BD21B0`. Their completeness scores (44.8 to 89.4) lose points only to
void * parameters, x87 float locals and register locals whose committed type re-splits on every retype.
Plates only (not renamed): `0x00B94170` / `0x00B9D6E0` (CDIR_INTERFACE load / save), `0x00B90210` /
`0x00B9B6C0` (transfer desire load / save), `0x00FCD150` / `0x00FCBD50` (cdir_configs row reader /
loader); `0x00B58A30`'s plate now names the handover queue applier.
Stored relation belief (worker cai-belief, V5 write-ups; completeness 78 to 80, the rest is void *
`this` with no struct): `OnCaiDiplomacyChangedMarkRelationsDirty` `0x00A8F320`, `MarkAllCaiRelationBeliefsDirty`
`0x00A88CE0`, `OnCaiRoundEndSetRemarkFlag` `0x00A8F830`, `BroadcastModelEventToListeners` `0x008DBAC0`,
`PropagateCaiWorldBeliefDirty` `0x00BCED30`, `MarkBeliefDirty` `0x00CF0AB0`,
`PropagateBeliefDirtyToDependents` `0x00CF0B10`, `IsBeliefDirty` `0x00CF1170`, `SetBeliefDirtyFlag`
`0x00D09730`, `FindCaiRelationEntryForFaction` `0x00A89E00`, `GetCaiRelationBelief` `0x00A74A20`,
`FindCaiRelationBeliefInRegistry` `0x00A87130`, `ApplyThreatenedAttitudeFactor` `0x00B72EC0`,
`ApplyThreatenedFactorTowardsFaction` `0x00B72F00`, `RefreshRoundStartRelationFactors` `0x00B71EC0`,
`UpdateComputedFactorsForAllRelations` `0x00B71E90`, `UpdateFactionComputedRelationFactors` `0x008BAF30`,
`ApplyRoundEndDriftAndBroadcastDiplomacy` `0x00B29100`; globals `g_bCaiWriteHistoryHtmlLog` /
`g_bCaiWriteHistoryXmlLog` `0x015C5748` / `0x015C5749`, the shared node pool `g_pSharedNodePoolFreeHead` /
`g_pSharedNodePoolBlocks` / `g_dwSharedNodePoolBlockSize` `0x0145925C` / `0x01459260` / `0x01459264` (10000).
Broadcasting diplomacy writes (worker cai-broadcasters, V5 write-ups, 2026-10-14; completeness per
function, the rest of each score is `void *` `this` / relation records with no struct and register
locals that re-split on retype): `BreakTradeBothWays` `0x00B73430` 95, `IsStanceProtectorate`
`0x00B5DCC0` 91, `IsStancePatron` `0x00B5DCB0` 91, `SettleProtectorateTiesAndJoinPatron` `0x00B104E0`
86, `SetupAllReligionAttitudeFactors` `0x00B1B4E0` 86, `MakeProtectorateOfPatron` `0x00B105C0` 84,
`ResetRelationsOnFactionReemergence` `0x00B2BB00` 84, `SetPatronStanceAndIncome` `0x00B10480` 84,
`ClearProtectorateStanceAndTribute` `0x00B64C10` 83, `ClearPatronStanceAndIncome` `0x00B54530` 83,
`EndProtectorateTie` `0x00B64B10` 87 (polish19: `CampaignFaction` grown to its allocation size 0xA24,
`push 0xA24` at `0x0090C0A4` before the faction loader `0x0087A190`; fields +0x764 / +0x768 force-handle list
(count / data, as `0x008B4330` reads them), +0x778 / +0x77C region list, +0x820 prestige, +0x824 out-of-game
byte (read by `0x008D9DC0`, not by `0x00B64B10` itself); the patron is now typed `CampaignFaction *`),
`AddAbandonedAllyFactorUnlessAtWar`
`0x00B544F0` 82; strings `g_szFactionKeySpaSpain` / `g_szFactionKeySpaPortugal` beside the existing
`g_szFactionKeySpaBritain` (all three `szGame_FactionSpa*` until polish31 added the `g_` prefix).
Desires: Ghidra not yet updated (names and plates for the next writer-lock batch, V5 with them):
`StepAtWarDesire` `0x00BAC560`, `StepBoostPlayerRivalsDesire` `0x00BAC310`, `SetAutoresolverModifierPair`
`0x00BC5240`, `StepReplenishUnitsDesire` `0x00BADA40`, `StepUnitXpUpgradeDesire` `0x00BAE910` (needs
create_function), `IsDirectorServedFaction` `0x00BB5160`, `CollectXpUpgradeArmies` `0x00BA30B0`,
`CollectReplenishUnits` `0x00BA2A00`, `UnitNeedsReplenish` `0x00BC0800`, `ApplyReplenishUnit`
`0x00BC06E0`, `QueueUnitXpUpgrade` `0x00BA3240`, `DrawUnitXpSteps` `0x00BD4A60`, `SpawnDirectorTransferGarrison`
`0x00BA8400`, `DrawDirectorRandomInt` `0x00BBD6A0`, `GetCdirConfigFactionFloat` / `Uint` / `FactionInt`
`0x00BD2960` / `0x00BD2920` / `0x00BD29F0`, `ParseFloatStrict` / `ParseUint` / `ParseInt`
`0x004F38A0` / `0x004F37B0` / `0x004F3720`; also `0x00BB50D0`, `0x00BB51E0`, `0x00BB5530`, `0x00BB5270`.
Fix round (V5 write-ups, completeness re-scored 2026-10-14): `BuildCaiRelationBelief` `0x00ABAFB0`
(60.2; labels `push_to_stance_list` `0x00ABB0EE`, `append_entry_without_attitude` `0x00ABB212`),
`CaiRelationBeliefInfo::InitializeCaiRelationBeliefInfo` `0x00A44C50` (80.9),
`GetCaiWorldFromInterface` `0x00B2B7B0` (89.4), `GetCaiWorldFactionList` `0x0047B520` (89.4),
`PtrVector::PushBackPointerVector` `0x00483D40` (77.7), `ModelEventBroadcaster::BroadcastModelEventToListeners`
`0x008DBAC0` (83.3, was 66.7). Structs: `PtrVector` (0x14: vtable, +4, capacity +8, count +C, data
+10), `CaiRelationBeliefInfo` (0xAC: entries +8..+10, allies +2C, enemies +40, access +54, trade +68,
+7C, +90, +94, enemy-majority flag +A8), `ModelListenerNode` (0xC: prev, next, listener),
`ModelEventBroadcaster` (0x10: vtable, +4, first node +8, end node +C). The rest of the deductions
are x87 / register locals whose committed type re-splits on every retype and `void *` parameters.

## Destruction and re-emergence (broadcasters2, 2026-10-15)

Ours: `ntw_sim::campaign::destruction` (`capital_after_owner_change`, `end_if_capital_lost`,
`destroy_faction`, `liberate_region`, `reemerge_faction`), the flag `FactionDetails::out_of_game`
(faction +0x824, read from the start position and the save by the importer; `in_the_game` reads it).

- **Owner change** `TransferRegionToFaction` `0x00A64AC0` (CONFIRMED): with its second (liberation)
  flag and the gainer out of the game, the gainer comes back first (`0x00A64C98`..`0x00A64CB9`,
  below). The loser's side is `0x008BA580`: its capital (+0x72C) cleared when it is the region, the
  region removed, then `0x008DABC0` picks a new capital: over the loser's regions in its home theatre
  (+0x734), the squared single-precision distance from the lost capital's settlement; the nearest
  region whose settlement has at least 4 building slots (settlement +0x1A8), else the nearest other,
  else none; strict `<`, first wins a tie. Neither step runs for the rebels. The gainer's side
  (`0x008AE270`) makes the region its capital when it has none or it is its home region.
- **End of a faction** (`0x00A651D5`..`0x00A65208`, CONFIRMED): when the first flag is set (a capture,
  the director's transfer, a deal `0x00C18BF0` → `0x00B58A10(faction, 1, 0)`), the loser is not the
  rebels, has no capital left and is in the game, `0x008AB6C0` ends it: +0x824 set,
  `0x00B2B8B0` resets each record both ways (`0x00B2B920` on its own, which first breaks trade both ways
  `0x00B73430` only towards its killer; `0x00B72900` on the other's: trade, access, payments, income,
  counters cleared, stance neutral; attitudes, options and the start attitude kept). An AI faction loses
  its forces and characters; a human one keeps them. Event `FactionDestroyed` (exe event +0x108).
  Tests `capturing_the_last_region_ends_an_ai_faction`, `a_human_faction_out_of_the_game_keeps_its_forces`,
  `a_deal_giving_away_the_last_region_ends_the_giver`, `a_lost_capital_moves_to_the_nearest_town_and_back_home`.
- **Destruction by a killer** (handover, 2026-10-20, CONFIRMED by disassembly and decompilation unless tagged; code
  `destruction.rs` `destroy_faction_by`, `grant_faction_handover`, `take_over_faction`; `director.rs`
  `spawn_transfer_garrison`). Callers with a killer: the script binding `grant_faction_handover` `0x00978950` and the
  mission reward `0x00A06050` (only caller `0x00A26600`: the money, then, when mission +0x68 names a faction,
  `0x008AB6C0(target, mission owner +4)` with no other check; mission completion is not modelled, so ours only offers
  `take_over_faction`).
  - `0x00978950` (thiscall, `RET 4`): pops max_turn then min_turn, each a Lua number only (`0x01055700`, type 3; another
    type logs and clears the reader's ok flag, so the keys after it read empty) rounded ties to even (`0x010556D0`),
    then target and killer keys (`FindCampaignFactionByKey`; the target only when its key is non-empty, `0x004F3500`).
    Needs both found and the target's +0x824 clear; no killer ≠ target check. max = max(max, min) (`CMOVG`
    `0x00978A55`); turn = the calendar's elapsed turns (model +0xF68 → +8); nothing unless min ≤ turn ≤ max (signed).
    Then one campaign RNG draw (model +0xFB8, inlined: next16 until ≥ 0x58, then % 101, ours `percent_0_100`) and the
    destruction when draw ≤ 100.0f (`0x01318064`) / (f32)(max − turn + 1) (`DIVSS`, `COMISS` / `JNC` `0x00978AE6`) or
    turn ≥ max (`0x00978AEB`); call at `0x00978AF4`. Vanilla: `campaigns\mp_eur_napoleon\scripting.lua` 803-810
    (France AI and the handover flag unset → france takes bavaria and wurttemberg, turns 0..0: always at once).
  - `0x008AB6C0` with a killer: the faction's region list (+0x76C vector; count +0x778, data +0x77C) is copied first.
    The relation reset `0x00B2B8B0(killer)` → `0x00B2B920` breaks trade both ways (`0x00B73430`) on the dead faction's
    own record towards the killer when its +0x788 is set, then resets. AI target: the characters and forces go, the
    slots are handed back (as without a killer), then killer +0x924 = 1 (`0x008ABC50`, even with no region) and, while
    the faction's list is non-empty, region[0] goes to the killer, or region[1] when region[0] is the faction's capital
    (`0x00A8B5A0`) and more than one is left (`0x00B58A10(killer, 0, 0)` at `0x008ABC30` / `0x008ABC98`: no end check,
    no liberation; ours `transfer_region`); then the fort hand-back `0x00B1B010` (`0x008ABD3E`). An AI target with a
    killer posts no event +0x108; without one it posts it with flag 0. Human target: the campaign-over flag
    `0x008C30A0`, event +0x108 flag 1 and the multiplayer report; regions, forces and characters stay.
  - Hand-off (both kinds, when the copied list is non-empty): `0x00BA80E0(director, killer, list)` finds the killer's
    CAI faction (`0x00C3C4E0` over the CAI world list +0x11C / +0x120; none → nothing; ours `ai_order` contains the
    killer) and each region's CAI region (`0x00C3C970`; any missing → nothing; ours has every region), then
    `0x00BA8090`: library sort `0x00B7FEE0` with the comparator at `0x00B7CF1A`..`0x00B7CF5D` (x before y when its CAI
    settlement's building slot count is larger; on a tie when its region's recruitable list count, region +0x1B0, is
    at least y's: a non-strict `>=`, harmless for the insertion sort), then `SpawnDirectorTransferGarrison`
    `0x00BA8400(killer, sorted[0])` and, when more than three regions were listed, sorted[1]. The slot count is the
    settlement's own slots plus two on every settlement of `orig_fr_t1.save`, `orig_fr_may1811.save` and a new
    campaign's turn-1 save, so ours sorts by `settlement_slots` (INFERRED above: the `settlement:` key prefix).
    `0x00BA8400` needs the killer to own the region with no garrison: a human target's regions get none.
  - Killer +0x924: set only at `0x008ABC50`; cleared at the faction's turn end (`0x008BD0F0`, word write at
    `0x008BD408` with +0x925); constructor `0x0098C2F0` (`0x0098CFD5`); saved by `0x00892A80` (`0x00893888`) and
    loaded by `0x0087A190` before +0x925 (FACTION #62, ours `FactionDetails::destroyed_faction`). Read by the script
    condition `FactionDestroyedByCharacterFaction` `0x0089F4B0` (the context character's faction, +0x2A0; false
    without a character).
  - ORIGINAL BUG (`0x008AB6C0` / `0x00A64AC0`): no killer ≠ target check; with the same faction the transfer returns
    early at `0x00A64D13`, region[0] never leaves the list and the loop never ends. Ours refuses it (logged, nothing
    changes). No vanilla data does it.
  - CONFIRMED tie order: every walk uses the faction's region list (faction +0x76C, ours `World::faction_regions_in_list_order`: the loaded order, then regions gained since in gain order): the hand-over takes the list's first region, or its second when the first is the capital; the garrison pass sorts its copy of that list with the MSVC `std::sort` replica, so equal keys fall as the exe's sort leaves them from that input; the new capital (`0x008DABC0`, faction +0x76C at `0x008DAC2F`..`0x008DACF7`) keeps the first of equally near regions in that list (strict `<`). INFERRED: the garrison sort key (the CAI settlement's slot count read as the settlement's slots plus two, constant on three original saves; the CAI slot-list builder is not traced).
  - Ghidra names (V5, raw completeness score): `HandleScriptGrantFactionHandover` `0x00978950` (75),
    `SpawnKillerHandoverGarrisons` `0x00BA80E0` (78), `SpawnGarrisonsInSortedTransferRegions` `0x00BA8090`
    (89), `IsRegionCapitalOfOwner` `0x00A8B5A0` (100), `ApplyMissionSuccessReward` `0x00A06050` (71),
    `CheckFactionDestroyedByCharacterFactionCondition` `0x0089F4B0` (86); `DestroyFactionAndResetRelations`
    `0x008AB6C0` plate updated (76). The remaining points are register-only locals and untyped `this` (structural).
    Also named: `g_szFmt_InvalidUnitTableKey` `0x01316FC0`.
  - Tests (ntw_sim `campaign/tests/handover_tests.rs` and the two `director.rs` hand-off garrison tests, ntw_script
    host tests, ntw_campaign round trip of #62).
- **Re-emergence** `0x008AE6E0` (CONFIRMED): `0x00B2BB00` on its diplomacy manager, then +0x824
  cleared, then `RebuildFactionEffectSum` (ours derives effects when read). For each of its records,
  `0x00B2BC10` resets its own record and `0x00B72AC0` the other faction's record towards it:
  attitude = the record's start attitude (+0x830), stance neutral; the pirates on either side: war,
  attitude + the `war` event (−140); else when the pair's other faction is the previous owner (the
  record's target on the own side, its owner on the other side): with the liberation flag protectorate
  (own) / patron (other); without it, an AI previous owner costs 50 (own) / 25 (other) attitude, a
  human one means war + the `war` value. Then `0x00B45A40`: `initial_modifier` = the attitude, the caps
  of the two `declared_war_against_*` factors from the `war_against_*` events, the twin links of the
  two factor arrays, the `war` factor for a war, the `alliance` factor for an alliance or a
  protectorate, the government factor; `0x00B1B540` the religion factor; `0x00B45C20`: a held trade
  agreement re-checked (`0x00BB33C0`, remade through `0x00B55090` when the route check `0x00BA42E0`
  passes both ways), a war reset to neutral and declared through `DeclareWarAndCallAllies` `0x00B268B0`, an alliance through
  `0x00B29C60`, a protectorate through `MakeProtectorateOfPatron` `0x00B105C0` (no peace step). Ours
  models the liberation path (`liberate_region`, from the capture choice). Tests
  `a_liberation_brings_the_faction_back_as_the_liberators_protectorate`,
  `a_returning_faction_is_at_war_with_the_pirates`,
  `a_return_from_an_ai_owner_costs_attitude_and_from_a_human_means_war`,
  `in_the_game_follows_the_out_of_game_flag`.
- **PROVISIONAL** (in `reemergence_record`): the attitude adjustment `0x00B30A50` after the setup (not
  traced to its field); the record's second factor array (+0x3C8, the model holds one); a war set
  directly as the stance with its factor instead of the declaration `DeclareWarAndCallAllies` `0x00B268B0` (its ally calls: "Allies called into a war"; the
  flags `0x00B45C20` passes are not traced). **INFERRED** (in `destruction.rs`): the settlement's slots read as the slots whose key
  starts with `settlement:`; the owner of a town slot read through the slot's +0x38 object. The tie
  order is the region list order (CONFIRMED, see the hand-over above).
- **Not modelled** (BACKLOG): the revolt path (`0x00AAB440`: a re-emergence without the liberation flag,
  the rebel-record redirect, the `spa_napoleon` access grant `0x00B444B0`, the multiplayer case); the
  lost-route trade break; the destruction's handing out of regions in other theatres (never reached
  on a one-theatre map); in `0x008AB6C0`, the achievement / stat
  tracker (`0x009C7660`, event +0x120; not a game rule).
- **Ghidra (V5, TOOL_USAGE guide):** renamed with prototypes and plate comments:
  `HandleFactionReemergence` `0x008AE6E0` (95.0 effective), `ResetRelationsOnFactionReemergence`
  `0x00B2BB00` (83.9), `ResetOwnRecordOnReemergence` `0x00B2BC10` (91.6),
  `ResetOtherRecordOnReemergence` `0x00B72AC0` (91.6), `InitializeRelationshipRecordFactors`
  `0x00B45A40` (95.0), `ApplyRecordStanceAfterSetup` `0x00B45C20` (81.7),
  `ResetRelationsOnFactionDestruction` `0x00B2B8B0` (83.8), `ResetOwnRecordOnDestruction` `0x00B2B920`
  (93.1), `ResetOtherRecordOnDestruction` `0x00B72900` (93.1), `RemoveRegionFromFactionAndMoveCapital`
  `0x008BA580` (80.9), `CampaignFaction::FindNearestCapitalRegion` `0x008DABC0` (82 raw, typed and commented in the hand-over fix round) and, in the fix round,
  `CampaignFaction::DestroyFactionAndResetRelations` `0x008AB6C0` (90.0): the destruction itself (out
  of game +0x824, +0x754 cleared, relations reset, characters deleted for an AI faction, regions to the
  best neighbour or to the killer, town slots returned, FactionDestroyed). New structs:
  `DiplomacyRelationRecord` (the 0x848-byte relationship record) and `DiplomaticStanceRecord` (20
  bytes, stance id at +0x10). Globals named: the stance keys (neutral `0x0137D714`, protectorate
  `0x0136CC60`, patron `0x0137D74C`, war `0x0136D47C`), the two database key error formats
  (`0x0137E010`, `0x0137E758`) and the key separator `0x013305F8`. The `0x00B2BB00` plate now reads the
  transfer flag's low byte as the liberation flag (the resets take it as such) and says what ours models.
  The remaining deductions were register-only locals and the diplomacy manager (faction +0x528), which
  had no struct then; polish31 (next bullet) added it.
- **Ghidra, polish31 (2026-10-16, V5):** struct `FactionDiplomacyManager` (0x24 bytes; `operator new(0x24)`
  at `0x0087BEF4` / `0x0087A8DD`, constructors `0x00AE8DD0` (new faction) and `0x00AE8E10` (save load)):
  +0x0 owner faction, +0x4 never set, +0x8 record capacity, +0xC record count, +0x10 the
  `DiplomacyRelationRecord` array (save `DIPLOMACY_RELATIONSHIPS_ARRAY`), +0x14 alliances broken,
  +0x18 access abuses (`0x00B0DA90`; §Diplomacy manager counters), +0x1C treaties broken (both constructors preset 1; the load overwrites it only when
  the block version is above 1), +0x20 access cancel marks (version above 2). CONFIRMED from both
  constructors. `CampaignFaction.pDiplomacy` (+0x528) is typed with it, and `0x00B2BB00` is now
  `FactionDiplomacyManager::ResetRelationsOnFactionReemergence` (93.1 raw; locals typed, its three
  `spa_*` faction-key strings renamed `g_szFactionKeySpa*`). Callees named:
  - `CampaignFaction::GetFactionKeyOrRebels` `0x008E5060` (88.9): the faction's key string, the
    pointer at faction +0x514 (the record the faction was built from, key first); a faction without one
    (the rebels) gets a function-static "rebels" string (`g_RebelsFactionKey` `0x015C1264`, built once
    under `g_nRebelsFactionKeyInitGuard` `0x015C1270`). The struct field +0x514 is still called
    `pPlayerSlot`, which does not match this use; left for the faction struct's own pass.
  - `CampaignFactionRegistry::FindCampaignFactionByKey` `0x00955B70` (96.9, its maximum; 87.5 before
    polish46): looks a faction up by key in the faction registry (model +0xF5C), an open-addressing
    hash table (16-byte buckets at +0x44, count +0x48, empty-key sentinel +0x4C, UTF-16 XOR hash with
    seed 0xF73A51C9), linear probe from the home bucket with wrap-around, null when an empty bucket or
    the full circle is reached. polish46 (2026-10-17, CONFIRMED by disassembly): struct
    `CampaignFactionRegistry` (0x5C: +0x2C dwFactionCount, +0x30 ppFactions, +0x44 pBuckets, +0x48
    dwBucketCount, +0x4C emptyKey, +0x59 key-hasher byte) types `this`; struct `FactionKeyBucket`
    (16 bytes: key string +0, pFaction +0xC) types the bucket cursor; `CampaignModel` +0xF5C is
    `pFactionRegistry`.
  - `FactionDiplomacyManager::GrantMilitaryAccessToFaction` `0x00B444B0` (96.9, its maximum): finds the
    record towards the grantee, applies the access there (`0x00B44550`), runs the two path-grid
    refreshes below, then broadcasts the model's +0x858 event with {granter, grantee, turns} and the
    diplomacy-changed event (+0x3A8) with {granter}. Ours (`grant_military_access`, treaties.rs) has
    the record fields and the diplomacy-changed broadcast; the +0x858 event is not modelled, and the
    refreshes need no port: our `plan_path` (ntw_sim campaign/commands.rs) builds the obstacles at
    search time and blocks a zone only for a force at war with the mover (`Stance::War`), as the
    exe's mode test `0x00B57C00` does (own faction or not at war → core only). That test also takes a
    third faction, whose obstacles get no "not at war" pass; which faction its callers pass is
    UNKNOWN and ours has no counterpart ("Access-loss expulsions" above). The refreshes (polish46, 2026-10-17, CONFIRMED by disassembly):
    - `RefreshFactionPathGridObstacles` `0x00B795A0` (cdecl, one CampaignFaction; 84.7, 0 before; the
      rest is a void* local of untraced type and a vtable slot): reaches the path grid (faction +0xA8
      → +8 model → +0xF64), has every grid layer recompute its obstacle modes against that faction
      without storing the view (`0x00B06210`(faction, 0)), then walks the faction's force handles
      (+0x764 count masked & 0x3FFFFFFF, +0x768 array): each live character (vtable +0x24 check true,
      byte +0x1B7 clear) recomputes its zone of control (`0x00A28440`(1)) and is re-added to the grid,
      by its army's vtable +0x44 object (`0x00B3F630`) when it has one, else by itself (`0x00B1B700`)
      unless its army exists and `0x00A0B820` is false. 12 callers (polish50, by xrefs; each also calls `0x00B67490`): ten diplomacy changes, a region transfer and the round end: access grant
      `0x00B444B0` and cancel `0x00B67B20`, the alliance-break records `0x00B13A30` / `0x00B0CA20`, protectorate
      make `0x00B105C0` and end `0x00B64B10`, peace `0x00B262C0` and its access step `0x00B0D0C0`, war `0x00B26700`
      (only with its refresh flag), the ally call `0x00B268B0`; the capture event `0x00B62120` of a region
      transfer (from `0x00A64AC0`, not a diplomacy change); and the round end `0x00B29170`.
    - `CampaignPathGrid::ReapplyPathGridFactionView` `0x00B67490` (thiscall on the grid; 96.9, 6.4
      before): rebuilds the layers for the view the grid last stored, by mode +0x414: 0 one faction
      (`0x00B06210`(+0x3FC, 1)), 1 faction plus list (`0x00B06C00`, list +0x404..+0x410), 2 one faction
      masked to the polygons of region +0x400 (`0x00B07870`); other modes nothing. 27 callers: 12 run it
      after `0x00B795A0` (the 12 callers of that function listed above: access, alliance breaks, peace, war,
      protectorate changes, a region transfer's capture event, round-end relation update); the other 15 call it alone (e.g. the bare
      wrapper `0x00AF6D00`, `0x00AF6930`, `0x00B4E2F0`, `0x00B64E30`, `0x00A87F50`). The view it rebuilds
      is whichever one was stored last, not necessarily the player's: `0x00B06210` with store=1 sets
      +0x3FC to its faction argument and mode 0, and e.g. `0x008C1140` stores its own faction
      parameter that way.
    - Struct `CampaignPathGrid` (0x418: +0xB4 dwLayerCount, +0xB8 ppLayers, +0x300 layer prepare
      argument, +0x3FC pViewFaction, +0x400 nViewRegionId, +0x404..+0x410 view list, +0x414
      nViewMode); `CampaignModel` +0xF64 is `pPathGrid` (polish21's "free-spot finder" field: its
      `0x00B38F10` is called on this grid). `CampaignCommandOwner` grew to 180 bytes (+8 pCampaignModel, +0xC
      pCommandStream, +0xB0 pLocalPlayerFaction).
  - Gap (CONFIRMED), closed by dipcounters (2026-10-16): manager +0x1C starts at 1 (both constructors,
    `0x00AE8DD0`, `0x00AE8E10`; every vanilla save holds 1). Ours now loads and saves all four counters
    and starts `treaty_breaks` at 1 (§Diplomacy manager counters).

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
| Other public order sources: education, religion, gentlemen, war results | modelled (round 4) with loaded religion shares, class bases and gentleman attributes | education / war / gentlemen formula CONFIRMED; gentlemen residents CONFIRMED (#14 loaded and saved, enter / leave in play; accompanying characters PROVISIONAL); religion table and sign PROVISIONAL | `economy::public_order` |
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
#24–#27) and keeps the four manager counters (`World::alliances_broken` +0x14, `access_abuses` +0x18,
`treaty_breaks` +0x1C, `access_cancel_marks` +0x20 of `DIPLOMACY_MANAGER` #1..#4); they are loaded and `save.rs`
writes them back (§Diplomacy manager counters); of the relationship records it writes the stances and #18.

Round 9: capture damage changes `BUILDING` #0 health and a repair is a construction item
(our writer and the original write it as item type 2, `0x00AE7010`, in the slot's `BUILDING_MANAGER`); loot changes `REGION` #12 / #14
and the classes' #12 war-results base.

The model now changes `REGION` #10 (GDP), #12 and #14 (town wealth), #15 (growth) and #17 (bankruptcy offset) every
round; `save.rs` does not write them, so a save keeps the loaded values. The accumulated trade values
(`TradePath::accumulated`, `trade::write_accumulated`) and `World::bankrupt_turns` are written into the original's
trade and economics records.

Where the original saves them (round 3, item 3; CONFIRMED from the writers and a save):
- **`World::bankrupt_turns`** = economics `+0x460`, the cannot-pay turn counter. It is the **last field (#3, u32) of
  `FACTION_ECONOMICS`** (writer `0x00BD46E0`). That record holds:
  - `history[]`, with the current index at +0x3F0 and the count at +0x3EC;
  - #1 i32 treasury (+0x3F4);
  - #2 u8[25] (+0x3F8..+0x45C, each dword through 0x00BC4F20);
  - #3 u32 (+0x460).
- **`TradePath::accumulated`** = the route's `+0x3C`. The path is
  `CAMPAIGN_MODEL/CAMPAIGN_TRADE_MANAGER/INTERNATIONAL_TRADE_ROUTES[]` (one per faction: utf16 faction key, then
  `FACTION_INTERNATIONAL_TRADE_ROUTES_ARRAY[]` of {`INTERNATIONAL_TRADE_ROUTE` v3, u32}). The route writer is
  `0x00AFD490`. A route record holds, in order:
  - u32 n, then n path points (i32, coord2d, u32 from, u32 to, bool);
  - bool;
  - u32[8], the commodity volumes;
  - **seven u32**: +0x28 route total, +0x2C commodity part, +0x30 resource part, +0x34 GDP part, +0x38 (the copy of
    +0x3C the recompute 0x00B15B50 takes and adds into +0x28; CONFIRMED), **+0x3C accumulated value** (0x00B05CC0
    adds to it), +0x40;
  - u32[] (+0x4C);
  - u32 count, then the commerce-raid entries (i32 amount, u32 saved faction id; see "Commerce-raid deductions").

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
| Diplomacy: attitude events, treaties, war / peace, allies called, per-turn update, computed factors, treaty money | CONFIRMED rules (AI decisions PROVISIONAL) | drift 1,469 steps; computed factors 506 / 506 except the leader (484) |
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

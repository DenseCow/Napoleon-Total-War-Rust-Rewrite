# Effects system fidelity (backlog §0, slot 0-F)

Worker: 0-F (branch `work/fidelity-effects`, worktree `%USERPROFILE%\Documents\NR-fidelity-mw`). Tags: CONFIRMED /
INFERRED / UNKNOWN; stand-ins PROVISIONAL / PLACEHOLDER. Ghidra: own copy `%USERPROFILE%\Documents\NR-f0c-ghidra`
(`analysis/fidelity/run_ghidra.ps1 exe <targets> <out>`, helper `ghidra_scripts/F0cDecomp.java`). Specs in our words; no
decompiled code is stored. Temp files: `target\tmp`.

## Where I am / what's next
- **PAUSED 2026-10-04 (worker count reduced).** State: branch merged with origin/main (b794116), `cargo build
  --workspace` passes; `cargo test --workspace` and `cargo clippy --workspace --all-targets` NOT yet run on the final
  state (`ntw_campaign --test effects` passes). Exact next steps: (1) run workspace test + clippy, fix anything new;
  (2) launch the game (`cargo run -p napoleon`), start a campaign, end a turn, save and reload (the start handicap and
  the #55 writer touch that path); (3) hand 0-B the §4 API and the §5.3 figures; (4) if the user can provide a vanilla
  save, rerun `effects_check` on it (§5) to compare #55, upkeep and taxes exactly; (5) the §6 open items.
- 2026-10-04: done and pushed on `work/fidelity-effects`: the effect tables (`ntw_data::effects`), the effects store
  and sums (`ntw_sim::campaign::effects`, API in §4), the sources loaded from startpos/saves (techs, difficulty, the
  saved faction containers #54/#55), the campaign-start handicap (`0x008DD090`), the save writer keeping #55,
  `effects_check` (research helper), integration tests (`ntw_campaign/tests/effects.rs`).

  normal gets e.g. land upkeep 5525 instead of 5580 and taxes 6870 instead of 6182 on turn 1 (§5.3).
- Open (PROVISIONAL, listed in §6): region +0x1DC and governorship container contents, the army container in the
  character view, the governor part (never applies in shipped campaigns), qualified kinds other than unit
  category/class/pop class/agent, the front-end difficulty choice, the minister attribute key, trait points falling.
- Value checks: the vanilla saves written by the original confirm upkeep (22/23, 4/4, 4/4 factions exact with
  effects), taxes (20/23, 3/4, 4/4) and the #55 basic sets (§5.1). The start positions' stored figures do not come
  from the game's formulas.

## 1. The effect store (CONFIRMED)
- An effect container is a sorted vector of 20-byte entries {key: 4 × u32 = (bonus type, qualifier 1, qualifier 2, bonus
  id), f32 value}; lookups binary-search the key (`0x00DEE380`) and return the value or 0 (`0x00E23DA0` float,
  `0x00E23E10` int rounded with FISTP). Adding a container merges sorted entries and sums equal keys (`0x00E04760`);
  adding one entry: `0x00E04BC0`.
- Bonus types (first key word) and their getter wrappers: 1 basic (`0x00E1EFD0` float, `0x00E1F130` int), 6
  population-class keyed (`0x00E1F050`), 0xE unit-category keyed (`0x00E1F0F0`), others for agent, building chain,
  commodity, religion, resource, shot type, ability, unit class (the `effect_bonus_value_*_junction` tables).
- The bonus ids are the engine enum at `0x0145B4A8` (see CAMPAIGN_FIDELITY.md "Effect ids"); the DB maps each effect key
  to (bonus type, bonus, qualifier) through `effect_bonus_value_basic_junction` (effect → bonus name) and the other
  `effect_bonus_value_*_junction` tables (effect → bonus name + unit category / class / pop class / ...).
- At DB load (`0x00E20760`) every effect source record (technology, trait level, ancillary, building level, government
  type, minister level, difficulty) gets its own compiled container (`0x00F88590`: each junction row's value under the
  key its effect maps to).
- Saved: only the faction's base containers (ESF `FACTION` #54 = faction +0x8C4, #55 = +0x8D4, `CAMPAIGN_BONUS_VALUES` →
  `CAMPAIGN_BONUS_VALUE_BLOCK[]` {u32 type, i32 bonus, f32 value, utf16 qualifier}; reader `0x00DF14C0`). In every shipped
  start position they hold only the agent caps (type 0, bonus 2: rake, admiral, General, gentleman); in saves #54 also
  holds scripted / event bonuses and #55 adds the difficulty handicap (§5.2). The exe builds +0x8D4 only at campaign
  start (`0x008DD090`: a faction with a player slot copies the slot's human flag +0xA8 and difficulty block +0xAC into
  +0x6E0 / +0x6E4; a faction without one (+0x514 == 0) takes the setup's difficulty with the flag off; then +0x8D4 =
  +0x8C4 + `0x00F9F970(difficulty, flag)`, clamped −2..2); a loaded save keeps its #55.

## 2. How the sums are built (CONFIRMED structure)
- **Faction** (rebuilt by `0x008B16C0`, stored at faction +0x6FC; tax tech, upkeep and others read it):
  `+0x8D4` (= `+0x8C4` base + the difficulty handicap container `0x00F9F970(difficulty, is_human)`, set up at campaign
  start by `0x008DD090`) + Σ owned regions' faction-wide building containers (region +0x198, rebuilt per region by
  `0x00A6AE40` from its building slots: only buildings at health ≥ 100 in a slot held by the region's owner add their
  effects, `0x00A62380` → `0x00A91FC0`; the holder is `REGION_SLOT` #0 `GARRISON_RESIDENCE` #0, round 6) + the technology
  manager's container (faction +0x7C4 object +0x18: researched technologies) + government (faction +0x70C, `0x008AFAB0`):
  the government type's container + for every ministerial post holder his post-level set (`0x008D9D40`) and his +0x48C.
- **Region** (computed on the fly, `0x00A67530`): region +0x188 (its buildings' local effects) + region +0x1DC + the
  faction part `0x008AFB50` (of the governing faction: the one whose governorship lists the region, normally the
  owner; INFERRED from `eur_bavaria` in the vanilla save `auto_after_c8`, see CAMPAIGN_FIDELITY.md §Public order): faction +0x6FC + the region's governorship container (`0x00BA2170`: governorship +0x14, empty when
  governorship +0xE4 is set) + the governor's post-level set and his +0x48C when the region's theatre (`0x008F4100`)
  is not the faction's home theatre (+0x734).
- **Character**: character +0x48C, the set the attribute getters read (`0x00A198D0` takes management_army / navy /
  finance / justice from it, CONFIRMED). Builder INFERRED `0x009CDF10`: the trait list's set (`0x008EDAE0`: each trait's
  current level record +0x58) plus each entry of a second list (taken to be the ancillaries). A minister or governor
  also adds his post-level set (`0x008D9D40`: post table (*(post+0xC))+0x4C, 16 bytes per level, level from
  `0x008D9D70`). The character view `0x008AFC00` (16 callers) = his army's container (army +0x98 → +0x24) if any,
  else faction +0x6FC, plus +0x48C.
- **Trait levels** (`0x008B5380`, CONFIRMED): the highest level whose threshold ≤ max(points, 0); when points fall the
  level stays at the trait's no-going-back level once that was reached; a trait whose points fall below 1 is
  removed (`0x008D21B0`). Trait entries are {trait, current level record, points} (12 bytes; list loader
  `0x0088B110`, add-points `0x008C2D30` with the anti-trait check and the max-trait count).
- Readers of the sums (from CAMPAIGN_FIDELITY.md): tax bonuses (`0x00BC75F0` faction `tax_bonus_technology` 0x7C;
  `0x00BC7540` region `tax_bonus_building` 0x2E; `0x00BC7580` faction `tax_bonus_minister` 0x6E for the home theatre, else
  the governor's `tax_bonus_character` 0x2F), upkeep (`0x008F9B10`: faction `upkeep_cost_mod_land_all` 0x3E + unit
  category bonuses), and the others listed there.

## 3. Technologies in the save (CONFIRMED layout)
`FACTION/FACTION_TECHNOLOGY_MANAGER` v4: `techs[]` {utf16 key, u32 state, f32 research progress, u32, u32[], u32}, i32.
State 0 = researched (CONFIRMED round 7: completion `0x008EED20` sets it with progress = cost), 2 = available, 4 = not
yet available; #2 = progress, #3 = the researching school slot id. Research is modelled (CAMPAIGN_FIDELITY.md §Research).

## 4. Query API for 0-B (`ntw_sim::campaign::effects`)
```text
use ntw_sim::campaign::effects::{Effects, BonusKind};
let fx = Effects::compute(&model);                 // all sums from the model (pure; recompute after changes)
fx.faction(f, "tax_bonus_technology")             // f32, faction sum (faction +0x6FC)
fx.faction_qualified(f, BonusKind::UnitCategory, "upkeep_mod", "infantry") // qualified bonus
fx.region(r, "tax_bonus_building")                // f32, region local + owner faction sum (0x00A67530)
fx.region_local(r, "tax_bonus_building")          // f32, only the region's own buildings (region +0x188)
fx.character(c, "tax_bonus_character")            // f32, traits by level + ancillaries
fx.character_total(c, "management_finance")       // f32, faction sum + character set (0x008AFC00)
fx.faction.get(&f).map_or(0, |s| s.get_int("admin_cost_mod")) // int getter (round half even, as FISTP)
```
Campaign start: `effects::apply_start_handicaps(&mut model)` (called by `begin_start_campaign`) fills
`FactionDetails.bonus_with_difficulty` (+0x8D4) from `bonus_base` (+0x8C4) and the handicap rows. Saved entries:
`effects::SavedBonus`, `saved_set`, `BONUS_NAMES` (engine ids → names).
Rules: `model.rules.effects` (`EffectRules`, filled by `ntw_campaign::rules::effect_rules_from_db`). Source data:
`FactionDetails.technologies` (key, state) and `.difficulty`; traits / ancillaries / buildings / government from the
loaded model. Keys are the engine bonus names (`effect_bonus_value_basic_junction` second column), e.g.
`tax_bonus_technology`, `upkeep_cost_mod_land_all`, `admin_cost_mod`. A missing key is 0, as in the exe. Effect values
are summed as given by the DB (percent points where the formula uses percent).

## 5. Checks against the original
Tool: `cargo run -p ntw_campaign --release --example effects_check -- <data dir> <file|vfs:campaigns/../startpos.esf>...
[--human key] [--bonus a,b]` (env `FX_BLOCKS=1` raw #54/#55, `FX_DIFF=1` handicap table, `FX_TECH=<faction>` tech
states, `FX_UNITS=<faction>` unit list). It prints per faction: unit upkeep with / without the upkeep effects against
`ECONOMICS_DATA` #5[1]/#5[2], taxes with / without the tax bonuses against #1[0], and #55 against #54 + handicap.

### 5.1 What the files can and cannot check
- **Start positions**: stored land upkeep equals the plain unit upkeep (ita 5/7, spa 4/4, egy 1/5 factions exact, eur
  stores 0): no effects were applied when they were authored, and #55 = #54 (agent caps only) because the campaign
  has not started. Stored taxes (#1[0]) do not match even the plain model (e.g. eur France 2400 vs 6182, every faction
  2–4× lower, no common factor): authoring figures, not the game's formula. They cannot check effect values.
- **Vanilla saves written by the original** (round 6, 0-B; `effects_check` on `auto_nr4_t4.save` = eur after 3 turns,
  `orig_fr_t1.save` / `orig_fr_may1811.save` = spa turn 1 and after 3 turns):
  - upkeep with effects 22/23, 4/4 and 4/4 factions exact (plain upkeep: 15/23, 0/4, 0/4);
  - taxes 20/23, 3/4 and 4/4;
  - #55: the basic entries equal #54 + the handicap row for every faction. The only difference is the chain-keyed
    building cost entries (exe type 2, e.g. `rFarm` −50 stored −58): the handicap's `building_cost_mod_*` rows feed
    building-chain-keyed entries (`Saved(2)`, id 0, qualifier = chain; the chain junction is mapped in `effect_rules_from_db`).
    Round 16 (CONFIRMED, `0x00B43300`): the construction cost reads that entry from the region effect set
    (CAMPAIGN_FIDELITY.md §Construction cost).

### 5.2 Structure confirmed by the saves (CONFIRMED)
- #54 is not only the agent caps: it also holds scripted / event bonuses (e.g. `happiness_active_lower/upper_gov_type`,
  `ai_region_resistance_modifier`). The model starts every faction sum from it.
- #55 − #54 is one handicap set per (difficulty, flag): the same for every AI faction, a different one for the human,
  with every faction's stored difficulty 0. Matches `0x008DD090`.
- Saved block entries: basic bonuses are type 1 with the engine id (`0x0145B4A8` table, 184 names, now in
  `effects::BONUS_NAMES`); type 0 entries (agent caps) carry the agent key as qualifier; type 2 entries are
  building-chain keyed (e.g. `rFarm`).
- Tech state 0 = researched: INFERRED from the start positions (France's 3 state-0 techs in eur are its start techs);
  the vanilla saves cover too few turns to see a new tech researched.

### 5.3 Turn-1 figures the effects give (eur_napoleon, France human at normal, `--human france`)
| faction | land upkeep plain → with effects | naval upkeep | taxes plain → with bonuses | main bonuses |
|---|---|---|---|---|
| France (human) | 5580 → 5525 | 2290 → 2496 | 6182 → 6870 | minister tax +4, upkeep land −1 / naval +9, gdp +3 |
| Austria (AI) | 6590 → 5461 | 0 | 5314 → 5314 | upkeep land −17 / naval −10, gdp +13 |
| Britain (AI) | 3880 → 3492 | 4420 → 2871 | 3324 → 3586 | minister tax +3, upkeep land −10 / naval −35 |
Upkeep with effects = round_half_even(upkeep × max(0, 100 + faction mod + unit category + unit class) × 0.01)
(`0x008F9B10`); taxes use `economy::{tax_efficiency, effective_tax_rate, class_taxes}` with `TaxBonuses {character:
tax_bonus_minister (home theatre), building: region-local tax_bonus_building, technology: tax_bonus_technology}`.

### 5.4 Campaign run, treasuries before / after
`NTW_TREASURY=1 cargo run -p ntw_ai --release --example determinism -- campaign 10` on origin/main (b794116) and on
this branch merged with it: every faction's treasury is identical on all 10 turns (state hashes differ: faction
details now carry the containers and the AI difficulty). Expected: no formula reads the effects yet (0-B's step).

## 6. PROVISIONAL / open
- Region +0x1DC and the governorship container (+0x14 unless governorship +0xE4) are added by the exe; contents
  UNKNOWN, not modelled. The governor's post set + his +0x48C are added only outside the home theatre: every faction
  has one governorship in the shipped campaigns (tested), so it never applies; not modelled.
- Character view (`0x008AFC00`) uses the army's container (army +0x98 → +0x24) instead of the faction sum when the
  army has one: UNKNOWN content, `character_total` uses the faction sum.
- Character +0x48C builder INFERRED (`0x009CDF10`: the trait-level sets via `0x008EDAE0` plus a second list's sets,
  taken to be the ancillaries). The trait level rule is CONFIRMED (`0x008B5380`); the no-going-back level and trait
  removal when points fall (`0x008D21B0`) are not modelled (the model never lowers trait points).
- AI factions' difficulty = the negated human difficulty (INFERRED, same as `ntw_ai`; the setup's +0x3C value is not
  traced). The front end has no difficulty choice yet, so the human keeps the file's value (0 = normal).
- Minister level: the base attribute key is INFERRED (`management`); `ministerial_effectiveness_modifiers` applied as
  read.
- Only basic, unit category, unit class, population class and agent junctions are mapped; building chain,
  commodity, religion, resource, shot type, ability and projectile keyed effects are dropped (none feed the economy
  formulas named so far).
- `turn.rs` got one line (`apply_start_handicaps` in `begin_start_campaign`); `save.rs` writes #55
  (`write_bonus_values`, only when it differs from the source).
- Speed (round 6): `Effects::compute_for(model, faction)` builds only what one faction's turn reads (its regions,
  characters and the faction sums involved); settle_round, the characters' turn start, recruitment points and the
  income wrappers use it. 3 AI turns: 13.6 s → 1.9 s, same state hashes.

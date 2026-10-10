# AI_RESEARCH: the original's battle AI and campaign AI (AI worker)

Tags: **CONFIRMED** = read from the exe (address given) or the shipped data; **INFERRED** = a strong
reading of the evidence; **UNKNOWN** = not found yet. Our stand-ins are tagged **PROVISIONAL**.
No decompiled code is reproduced here: only specs in our own words.

## 0. Where I am / what's next (updated with each push)
- DONE (round 7, sandbox worker ai2, sandbox commit 01b71af; ported to main from sandbox/main 90bfc1c):
  **RESEARCH_TECHNOLOGY** decoded and ported (§4 "RESEARCH_TECHNOLOGY", CONFIRMED structure): the
  behaviour row is in every shipped eur manager at priority 500; the step sorts its candidates by
  the exe's key descending and links one `CAI_BDI_GOAL_RESEARCH_TECHNOLOGY` goal per candidate with
  **mult 1.0, ×0.95, ×0.9025, …** in list order; a goal reserves a school for the faction and makes
  one intention (mult 1.0) only when a school was reserved; the intention starts the research there
  (`0x008EEC90`) and is dropped when the start fails. Ported as `campaign::research`
  (`goal_multipliers`) + `FactionTurn::{research_goals, start_research}` +
  `AiOrder::StartResearch` / `CampaignCommand::StartResearch`, with the candidate list and the school
  choice PROVISIONAL. eur_napoleon over 8 End Turns: 23 starts, 11 technologies under research.
- DONE: research below; `crates/ntw_ai` (battle AI v1, campaign AI v1, AI table reader);
  additive orders `crates/ntw_sim/src/battle/orders.rs`; app hook `crates/napoleon/src/battle_ai.rs`
  (`--ai on|off`, default on).
- DONE (round 2): merged main (campaign-play turn loop); `AiWorld::from_model` on the new model;
  `ntw_ai::campaign::driver` plays every AI faction at its `TurnStep::AiTurn` (bare model or the
  script host's AI hook), installed in the app (`--campaign-ai on|off`); run speed order option
  (`LandUnit::running`, `order_move_at`, charges run); ATTACK_BATTLEGROUP transitions from the exe
  (§3.2); Ghidra script `analysis/ai/ghidra_scripts/AiDecomp.java` + `run_ghidra.ps1` (own copy).
- Harness: `NAPOLEON_AI_SPEED=12 NAPOLEON_AI_TRACE=20 NAPOLEON_AI_SHOT=<png>@300 napoleon --battle`.
- DONE (round 2, Ghidra): think rhythm (§3.3 "Update rhythm"), objective constants located
  (§3.3, their readers not reached), manager/personality key path (§2.3, source UNKNOWN).
- DONE (round 3, worker ai2, branch `work/ai2`, worktree `%USERPROFILE%\Documents\NR-ai2`):
  FSM descriptor layout decoded (`fsmscan:` command); ATTACK_BATTLEGROUP targets, DEFEND_ABSTRACT,
  STOP_AND_SHOOT and GENERAL_SUPPORT transitions CONFIRMED (§3.2); STOP_AND_SHOOT and the
  DEFEND_ABSTRACT transitions implemented with tests. Objective classes and their virtuals
  decoded (melee/missile priority, minimum test, cap rule; §3.3) and ported. One alliance-AI update
  per battle tick CONFIRMED. CAI behaviour classes mapped (§4); difficulty handicap lookup
  CONFIRMED (§4 "Difficulty"). MERGE_UNITS and TAXATION added (PROVISIONAL rules).
- DONE (round 4, worker ai3, branch `work/ai3`, worktree `%USERPROFILE%\Documents\NR-ai3`):
  manager/personality keys CONFIRMED from the `FACTION` record and used (§2.3); personality
  vtable slot→tunable map (§4); desire priority rules of EXPANSION / REGION_DEFENCE /
  WAR_AND_PEACE CONFIRMED and ported as desires (§4 "Desire priorities").
  Also round 4: unit class/category enums and the class matchups (§3.3); the battlegroup tactic
  auction and every tactic's score / keep test (§3.1b), with STOP_AND_SHOOT's entry and its exit
  into the bayonet assault ported; MERGE_UNITS / TAXATION structure (§4; MERGE_UNITS is not an
  army merge); the CAI RNG; who writes the AI factions' difficulty (§4 "Difficulty"); §8 "what the
  models need"; the encounter phase (`battle::phase`) and OUTFLANK's score/keep tests for the
  cavalry wings.
- DONE (round 5, worker ai4, branch `work/ai4`, worktree `%USERPROFILE%\Documents\NR-ai4`):
  battlegroup formation (one per army) and the high-level plan vote / alliance modes CONFIRMED
  (§3.1c) and ported (`battle::plan`), replacing the PROVISIONAL attack/defend rule.
- DONE (round 5): the tactic auction (§3.1d) CONFIRMED and ported (`battle::auction`,
  `WingBidder`): OUTFLANK / DOUBLE_ENVELOPMENT / STOP_AND_SHOOT compete and exclude each other.
- DONE (round 5): the region value base (belief 0x4D) and the composite analyser CONFIRMED and
  ported (`campaign::region_value`); the original's own base values are read from the startpos /
  save (`keys::read_region_base_values`, 72 for eur, all `15000 + 25 k`). Difficulty: the battle
  consumer `0x006AD6E0` and the UI index mapping CONFIRMED; the AI's campaign value (player's or
  negated) still UNKNOWN. `logs\cai\` holds only flag images. `CAI_INTERFACE` spec in §8.
- DONE (round 6, worker ai5, branch `work/ai5`, worktree `%USERPROFILE%\Documents\NR-ai5`):
  the CAI **BDI pool processing** decoded (§4 "BDI pool processing": links, raw jitter, the
  selection order, the run loop) and ported (`campaign::bdi`); the faction turn now runs as a
  pool (`FactionTurn::run`, `Node` / `Action`). **The behaviour name → class table was shifted
  by one row** (§4 "CORRECTION"): REGION_GROUP_DEFENCE (not EXPANSION) makes the region-group
  goals, REGION_COAST_DEFENCE (not REGION_DEFENCE) has the `1, 0.5, …` rule, EXCESS_RECRUITMENT
  (not MERGE_UNITS) is the weighted new-unit recruitment, RESEARCH_TECHNOLOGY (not TAXATION) has
  step `0x00C2AD60`, WAR_AND_PEACE builds nothing. REGION_DEFENCE and EXCESS_RECRUITMENT ported
  with their CONFIRMED rules.
- DONE (round 6): TAXATION decoded and ported (always level 2, §4); battle: the outflank point
  test and side choice (`battle::outflank`), per-tactic seeds, the commit flag `+0x4A`
  (`auction::commit_allows`), the STOP_AND_SHOOT candidate-line selection (notes only: the
  candidate generator is not found); the alliance `+0x70` = defender (CONFIRMED, `set_defender`
  documented; campaign battles are not launched yet, so nothing sets it); the per-faction
  difficulty block in saves (`keys::read_difficulties`).
- NEXT (see §7): the desire / goal children (REGION_DEFENCE's desire `0x01378770`,
  RECRUIT_STRENGTH_IN_REGION, the mission factory `0x00CEEB00`, EXPANSION's component 0x175);
  MERGE_UNITS' step `0x00CD6260`; the STOP_AND_SHOOT candidate generator; OUTFLANK's sub-moves
  `0x0070B800`; the mode-3 handlers; the battle setup `+0x88` writer; difficulty (needs a save
  from a hard campaign). **Round 7 added:** RESEARCH_TECHNOLOGY's candidate key `0x00C288B0`, the
  analyser behind the goal refresh `0x00C3FF10` → `0x00AA4BF0` → `0x006649C0` and the effect test
  `0x00AAF1F0` (effect `0x27`).
- Note: the C: drive was full during round 2 (0 bytes free at one point); build with
  `CARGO_INCREMENTAL=0` to save space.

## 1. Sources
- Exe strings `analysis/worker1/strings_all_rs.tsv` (main checkout), tweakers `tweakers.tsv`,
  table names `db_table_names.txt`, row layouts `DB_BUILDERS.md`.
- Ghidra (own copy `%USERPROFILE%\Documents\NR-ai-ghidra`), `DecompTargets.java` on the functions
  named below. Output kept outside the repo.
- Shipped DB tables, dumped with `cargo run -p ntw_ai --example ai_tables_probe -- <table> [rows]`
  (`--grep <text> [prefix]` searches all VFS files).
- RTTI is useless here (the exe has no game RTTI, W1 §6); class names come from strings.

## 2. What drives the AI (data)
### 2.1 Campaign AI tables (CONFIRMED present, layouts from DB_BUILDERS)
| table | rows | content |
|---|---|---|
| `campaign_ai_personalities` | 3 | `default` (flag true), `eur_france`, `spa_spender` |
| `campaign_ai_personality_junctions` | 258 | personality, tunable, f32. `default` has ~245 tunables; the others override a few (e.g. `eur_france` LOOT_*, `spa_spender` spending biases and BAWP weights) |
| `campaign_ai_managers` | 10 | `EMPTY`, `nap_<camp>_full`, `nap_<camp>_maintainance`, `nap_eur_france`, `nap_eur_britain` |
| `campaign_ai_manager_behaviour_junctions` | 236 | manager, behaviour, priority. 30 behaviours, e.g. `nap_eur_full`: MERGE_UNITS 5000, EXPANSION 3000, REGION_DEFENCE 2000, NAVY_ATTACK 2000, REGION_GROUP_DEFENCE 1500, WAR_AND_PEACE 1000, GLOBAL_CONSTRUCTION 500, EXCESS_RECRUITMENT 500, MINIMUM_SAVING 200, DIPLOMACY_MANAGER 0. `maintainance` managers lack EXPANSION and RAID. France's manager has EXPANSION 4000; Britain's has NAVY_* and TRADE_AREA 3000 |
| `cdir_unit_balances` | 10 | config, min units, max units, group, target share, tolerance, min count: armies of 1-4 units 0.5/0.5 any; 5-10: inf 0.55, cav 0.2, art 0.25 (min 1 gun); 11-20: same, min 2 guns |
| `cdir_unit_qualities` | 434 | config, balance group, unit, quality (e.g. 12-lb foot gun 700, rocket troops 800) |
| `cdir_unit_balance_groups` / `_group_qualities` | 4 / 9 | groups and sub-group qualities (main_infantry 700, heavy_cavalry 500, ...) |
| `cdir_configs` | 91 | campaign-director key/values, mostly autoresolve modifiers (`CDIR_CVN_AUTORESOLVER_MODIFIER_*`) |
| `cdir_desire_priorities`, `cdir_faction_junctions`, `cdir_campaign_junctions` | 2 / 78 / 9 | director bookkeeping |
| `campaign_difficulty_handicap_effects` | 88 | difficulty, is_ai, effect, value (AI at normal: recruitment −5 % land, upkeep +10 %) |
| `building_units_allowed`, `units_to_exclusive_faction_permissions`, `units_to_groupings_military_permissions` | 1708 / 1012 / 207 | what a building recruits and who may recruit it |

Tunable families (default personality; full list via the probe): `BASIC_SPENDING_BIAS_*`
(construction 0.25, army 0.25, navy 0.4, diplomatic 0.1), `BASIC_INCOME_PROPORTIONAL_SAVINGS_PER_TURN`
10, `BASE_CONSTRUCTION_BIAS_*` (military 0.5, economic 0.2, education 0.2, happiness 0.05, prestige
0.05), `GRANULOID_CONSTRUCTION_BIAS_*`, `BAWP_WEIGHTING_*` (five weights, 20 each),
`RELATIONSHIP_MANAGEMENT_FRIENDS_OVER` 0.6 / `ENEMIES_UNDER` 0.1, `WAR_AND_PEACE_MANAGER_*`
(80 % / 90 % of required forces), `HLP_DISTANCE_MULTIPLIER_*` (enemy 3, neutral 1.5, friend 1.1, sea
0.5, sea-land transition 2), `MINIMUM_*STRENGTH*`, `MINIMUM_CHANCE_OF_SUCCESS` 30,
`FACTIONAL_NONCACHING_ANALYSER_EASY_WIN/BADLY_LOSE` 1.5 / 0.6, `RECRUITMENT_ARMY_CATEGORY_BASE_PROPORTIONS_*`
(inf 0.6, cav 0.2, dragoons 0.1, art 0.1), `RECRUITMENT_*`, `REGION_DEFENCE_*`, `COMPOSITE_VALUE_ANALYSER_*`,
`INVASION_*`, `LOOT_SETTLEMENT_*`, `NAVY_*`, `SURRENDER_*`, `PRIORITY_RANDOMIZATION_DESIRE` 10,
`VALUE_ANALYSIS_TIME_HORIZON` 5, `TIMEOUT_FOR_FAILED_MOBILE_TARGETS_*`.

CONFIRMED: the exe registers these tunables by name in one long compare chain (`0x00DE31E0`,
each name stored to a fixed float slot), so a mod can change any value but cannot add new keys.

### 2.2 Battle AI data
- **No battle-AI DB table exists** (CONFIRMED by the table list). The numbers are in the exe.
- Tweakers (CONFIRMED, `tweakers.tsv`): `Battle AI Exclusive` (AI v AI), `Battle AI Invert` (AI
  plays the other side), `Battle AI Formed Infantry` (HighLevelPlanner.cpp:108, "keep missile
  infantry in formation"), `Battle AI Log Melee` (writes `battle_ai_melee_log.txt`),
  `Naval AI Exclusive Boarding`, `Test AI Build`.
- `units.ai_role` (col 23, e.g. `line_infantry`) exists; how the AI uses it is UNKNOWN.

### 2.3 Manager / personality per faction: CONFIRMED (round 4)
**Source found (round 4, worker ai3).** The object at `faction→+0x168` is the campaign faction
itself (its `+0x6E0` is the human flag, `+0x6E4` the difficulty, read through the same pointer by
`0x00C3F870` / `0x00C288E0`). Its `+0x82C` (manager) and `+0x838` (personality) are UniStrings
filled by the `FACTION` ESF record reader `0x0087A190` (at `0x0087B559`): two UTF-16 strings in a
row, then for record version >= 15 two more into `+0x844` / `+0x850`, else both `"default"`
(`0x0087B599`). In the eur startpos (`FACTION` v18) they are children #48..#51, just before the two
`FACTION_FLAG_AND_COLOURS` records: France `nap_eur_france` / `eur_france` / `default` /
`default`. The earlier "occurs nowhere" result was wrong. eur_napoleon: Austria, Britain, Ottomans,
Prussia, Russia run `nap_eur_full` (Britain `nap_eur_britain`), France `nap_eur_france`, the other
35 factions `nap_eur_maintainance` (no EXPANSION / RAID); all personalities `default` except
France. Ported: `ntw_ai::campaign::keys::read_ai_keys`, `FactionAiConfig::resolve_with`,
`TurnContext::ai_keys`, `driver::install(.., keys)`, the app re-reads the startpos/save bytes.
Still open: the per-faction manager *type* (`CAI_INTERFACE_MANAGERS[]` `u32 = 11` = CAIMT_DB) is
not read; a faction with another type would not use the DB manager at all.

Earlier notes (rounds 1-3):
The startpos `CAI_INTERFACE/CAI_INTERFACE_MANAGERS[]/CAI_FACTION_MANAGER` holds per faction
`[u32, u32 = 11, learnt parameters, bool, u32, u32]`. INFERRED: 11 = `CAIMT_DB` (the CAIMT_* names
in string order: FULL, MAINTAINANCE, REBELLION, RANDOM_MOVEMENT, END_TURN, DO_NOTHING,
DO_DIPLOMACY_AND_END_TURN, MOVE_THINGS_OVERSEAS..., MOVE_THINGS_BETWEEN_REGIONS..., EPISODIC_1,
HUMAN, DB, END_TURN_ALLOW_DIPLOMACY, NUM). The manager *key* string occurs nowhere in the startpos
or packs outside the two tables. PROVISIONAL rule in `FactionAiConfig::resolve`:
`nap_<prefix>_<faction>` else `nap_<prefix>_full`; personality `<prefix>_<faction>` else `default`.

Round 2 (CONFIRMED path, source still UNKNOWN): the manager factory `0x00A87F50` switches on the
faction's manager type (case 11 → the DB manager `0x00C90960`; type 13 is first resolved through
`0x00AAA340` from three per-campaign defaults at `+0x6CC/+0x6D0/+0x6D4`). The DB manager reads its
**key from `faction→+0x168→+0x82C`** (getter `0x00C164C0`) and the personality object
(`0x00C930C0`) reads **`faction→+0x168→+0x838`** (getter `0x00C164D0`); both are looked up by name
in `campaign_ai_managers` / `campaign_ai_personalities`. What fills that `+0x168` object is
UNKNOWN: neither key occurs in any pack file outside the AI tables (ASCII and UTF-16 searched, incl.
the startpos and the scripts). Lead: `start_pos_factions` (not shipped) has two optional strings
(fields 26/27, version >= 2) that fit, so the startpos may carry them in a block not decoded yet.
The DB manager builds one behaviour object per junction row by name (`0x00C90960` compares all 30
CONFIRMED names in a chain) with the row's priority.

## 3. Battle AI structure (exe)
Source files (CONFIRMED `__FILE__` strings): `EmpireBattle\Source\AI\BattleAI.cpp`,
`AI\HighLevelPlanner\HighLevelPlanner.cpp`, `AI\MeleeManager\MeleeAnalysers\*`.

### 3.1 High-level planner and unit groups (CONFIRMED strings, logic UNKNOWN)
- Per alliance, units are put in **unit groups** with a task: `ug:reinforcement`, `ug:reinforcing
  army`, `ug:move-to-point`, `ug:withdraw`, `ug:scout`, `ug:support-group`,
  `ug:attack-enemy-battlegroup(-naval)`, `ug:defend-line`, `ug:defend-crossing`,
  `ug:block-crossing`, `ug:assault-crossing`, `ug:seize-crossing`, `ug:skirmish`, `ug:outflank`,
  `ug:double-envelopment`, `ug:stop-and-shoot`, `ug:warcry`, `ug:limbered artillery`,
  `ug:special tactic`.
- Unit states: attack, advancing, defending feature tight/loose, moving, withdraw, planner,
  firing planner, special tactic; ROUTING / SKIRMISHING / HIDDEN.
- Unit categories/classes for the planner: Category Infantry/Cavalry/Artillery/Dragoons/...;
  Class Line/Light/Grenadiers/Elite/Militia/Skirmisher infantry, Heavy/Light/Lancers/Missile
  cavalry, Foot/Horse/Fixed artillery, General.
- Tactics (`Alliance %u::AI_TACTIC_*`): ATTACK_BATTLEGROUP, DEFEND_ABSTRACT, DEFEND_LINE,
  GENERAL_SUPPORT (FSM), LIMBERED_ARTILLERY, MOVE_TO_POINT, OUTFLANK (Approaching / Outflanking),
  DOUBLE_ENVELOPMENT, REINFORCEMENT, REINFORCING_ARMY, SPECIAL_TACTIC ('Line Infantry Attack',
  'Native American Tactic', "Whitston's Flexible Manoeuvres"), STOP_AND_SHOOT (FSM).
- Encounter phases: WAITING, MOVING, DISTANT APPROACH, CLOSE APPROACH, CONTACT, INITIAL ASSAULT,
  GENERAL MELEE. Outflank directions FRONT / LEFT FLANK / REAR / RIGHT FLANK / ABOVE / FLANK.

### 3.1b Battlegroups and the tactic auction (round 4, CONFIRMED structure)
- A **battlegroup** (ctor `0x0070A980`, vtable `0x01342EA0`, 11 slots; groups are built by
  `0x0075F0C0`) creates all its tactics up front: ATTACK_BATTLEGROUP (0x82C bytes,
  `0x0070B9A0`, vtable `0x013430C8`), OUTFLANK (0x9C, `0x0070BDA0`, `0x01344B3C`),
  DOUBLE_ENVELOPMENT (0x90, `0x0070BCD0`, `0x01344BB4`), STOP_AND_SHOOT (0xC4, `0x0070C050`,
  `0x01344CA4`), LIMBERED_ARTILLERY (0x7C, `0x0070BCF0`, `0x01344A44`), GENERAL_SUPPORT (0x8EC,
  `0x0085DB60`); DEFEND_ABSTRACT (`0x0070BB90`, `0x01343164`) comes from `0x0070BC90`.
  Battlegroup `+0x48` = **encounter phase** (display `0x00765370`): 0 NONE, 1 WAITING, 2 MOVING,
  3 DISTANT APPROACH, 4 CLOSE APPROACH, 5 CONTACT, 6 INITIAL ASSAULT, 7 GENERAL MELEE.
  **Phase computation (CONFIRMED, `0x0076C0B0`):** the engagement phase `0x0076C300` if not 0,
  else the distance phase `0x0076D250` if not 0, else 2 MOVING (slot 1 `0x0076C040` then forces
  ≥ 7 when flag `+0x54`; the slot-12 variant `0x0076C090` caps at 4 unless `+0x58`). Engagement:
  with `n` units, `e` = % engaged (`0x0054EE90` or `0x00797850`), `s` = % of the others shooting
  (`0x0057A150(1)` and `0x0055AFE0`) + `e` (integer %): 7 if `e ≥ 80` or (`e ≥ 40` and `s ≥ 80`);
  6 if `e ≥ 30`; 5 if `s ≥ 50`; else 0; hysteresis 7 keeps 7 over 5/6, 6 keeps 6 over 5; `n = 0`
  gives 6. Distance (squared, battlegroup virtual `+0x0C` to the target): 5 within 175 m (kept
  to 247.5 m while 5), 4 within 300 m (kept to 424 m while 4), 3 within 750 m (kept to 1061 m
  while 3), else 0; 0 with no target (`+0x1C`). Slot 2 `0x0076C950` proposes the next phase
  (0/1→3, 3→4, 4→5, 5→6, 6→7) when every active tactic's virtual `+0x4C(next)` agrees and next
  != 7 (`0x007A7380`); its caller is not traced. Ported: `battle::phase` (engaged = melee
  contact, shooting = `aiming`, distance = line centre to enemy centre; INFERRED stand-ins).
- **Tactic auction** (battlegroup slot 4, `0x00751800`, each planner step): every tactic is reset
  (`+0x0C(0)`); active tactics (`+0x4B`) whose **keep** test (slot 13, `+0x34`) fails release their
  units (12-byte entries `{unit, tactic, used}`) and deactivate (`+0x30`). Then, repeatedly, among
  the inactive tactics the battlegroup accepts, the one with the highest **score** (slot 15,
  `+0x3C(free units)`) wins (ties: the earlier one), claims units (slots 16/17, `+0x40/+0x44`) and
  is activated when its slot 9 test passes; zero scores drop out. The rest of the units go to the
  battlegroup itself. Tactic slot 1 is the unit filter. Random draws: `0x007CD510(i, n)` /
  `0x007CD530(i, lo, hi)` read a pre-drawn value `+0x18 + 4i` of **the tactic** modulo the range.
  **Round 6 (CONFIRMED, `0x0070AED0`, the base ctor of every tactic):** each tactic draws its ten values `+0x18..+0x3C` at construction, `x = x × 0x343FD + 0x269EC3`, value `x >> 16`, from an LCG at `(*arg)→+8→+0x50` (INFERRED the battle RNG); the battlegroup ctor builds its tactics in the order of §3.1d, so ATTACK_BATTLEGROUP takes draws 1–10, OUTFLANK 11–20, DOUBLE_ENVELOPMENT 21–30, … Ported (`AllianceAi::seeds`, PROVISIONAL: from a copy of the battle RNG).
- **Scores** (slot 15, CONFIRMED): ATTACK_BATTLEGROUP `0x007B1700` = 1 when any unit is free;
  LIMBERED_ARTILLERY `0x007B1940` = 164 when a free unit is an artillery piece (`0x0055AB90`);
  STOP_AND_SHOOT `0x007B1BD0` = 164 when phase 5, the group's virtual `+0x1C` ≥ 70 (INFERRED formed
  %) and `+0x6C`, the target battlegroup (`0x0078B070`, `+0x8C != 100`) has no unit with
  `0x0055AD30`, missile strengths `A < 2 × B` with `B != 0` (`0x0079C940` = Σ `+0xBEC`, halved for
  units with flag `+0x278D`; INFERRED A = target's, B = ours), the target is within the missile
  range of some free shooter (`0x0057A150(1)`, range `0x005646A0`), no free unit is artillery and
  at least one shooter (sets `+0x98`); OUTFLANK `0x007B1980` = 100 + (seed2 % 65) when phase 4 or
  5, `0x00797690`, not the group's virtual `+0x5C`, > 2 free units, not `+0x4A`, in phase 4 also
  `seed0 % 4 != 0`, the target has > 2 units, strengths (`0x007CCD60` = Σ (melee + missile)
  ratings + ships) `A ≥ 0.6 × B`, a positive weight (8 if target `+0x99` else 3, −8/+8/+4 by
  strength and unit counts, +5 if target `+0x88 > 30`, +7 if also above ours) and an outflank
  point found (`0x0078E3E0`) at 200 m (phase 4) or 100 m (`0x007AA0D0`) on either side (angle
  `DAT_0150BD14`); it claims `seed0 % (n/2) + 1` of the free units (slot 16 `0x007523F0`).
  DOUBLE_ENVELOPMENT `0x007B1720` = 80 + (seed1 % 65) when phase 5, the target has > 4 units,
  `4A/5 ≤ B`, the weight (6 or 1 base, ±6/+4, +5/+7, +6 when `+0x40` count > 1.5 × target `+0x84`)
  > 0 and points found on **both** sides.
- **Keep tests** (slot 13): ATTACK_BATTLEGROUP `0x0075EA40` = its score on its own units > 0;
  OUTFLANK / DOUBLE_ENVELOPMENT `0x0075EC50` = phase 4..7 and one of its sub-moves still running
  (`+0x30`); STOP_AND_SHOOT `0x0075ECA0` = the group has units, at least one shooter, no shooter
  engaged (`0x0054EE90`) or flagged `+0x278D`, phase != 7, target `+0x8C != 100`, and
  `A ≤ 2 × B` (missile) and `C ≤ 4 × D` (`0x0079C1E0`, INFERRED Σ melee ratings; INFERRED C = ours,
  D = theirs). **So shooting turns into the bayonet assault when a shooter is caught in melee, the
  phase reaches GENERAL MELEE, the enemy's missile strength passes twice ours, or our melee
  strength passes four times theirs**; ATTACK_BATTLEGROUP then gets the units.
- Unit filters (slot 1): ATTACK_BATTLEGROUP `0x007D3300` takes everything except artillery pieces
  that are deployed in place (`+0xB47`, or near their position `+0xDC4→+0x60`) or of class 0 and
  artillery-category units that are not pieces; OUTFLANK `0x007D34A0` takes mounted units,
  elephants, light infantry and skirmishers that are not the general's unit and not in state 6
  (returns 2 for units `0x007D0870` prefers).
- High-level plan (`0x007D7020` → `0x007B1FB0`, every 10 updates): decoded in round 5, see
  §3.1c.
- Ported (round 4): the STOP_AND_SHOOT score and keep tests as our line's entry and exit
  (`stop_and_shoot_scores` / `stop_and_shoot_keeps`; phases and the target battlegroup are not
  modelled: the enemy army stands in for the target group; the encounter phase is ported, `battle::phase`). Not
  ported yet: the auction itself (our line / wing split stands in), DOUBLE_ENVELOPMENT, the
  phase-advance proposal of slot 2. OUTFLANK: its score and keep tests drive our cavalry wings
  (`update_cavalry`; the outflank point test, the target flags `+0x99/+0x88` and the seed source
  are PROVISIONAL).

### 3.1c High-level plan, alliance modes and battlegroup formation (round 5, CONFIRMED)
**Alliance AI object** (ctor `0x00709FB0`; update `0x007B25A0`): `+0x28` our battle alliance
(armies `+0x20/+0x24`; each army's land units `+0xF0/+0xF4`, ships `+0x108/+0x10C`), `+0x54/+0x58`
our units, `+0x150/+0x154` enemy units, `+0x174/+0x178` enemy battlegroups, `+0x330` **our unit
tracker**, `+0x3E8` **the enemy's tracker** (`0x00755E50` adds each unit with `+0x1E4` set to the
tracker of its side: lists `+0x38C` / `+0x444`, CONFIRMED), `+0x4A0` the plan object (`+4` plan,
`+8` count of updates with own losses), `+0x61C` mode (`+0x620` previous), `+0x634` update
counter, `+0x646` deploying, `+0x647` first update done, `+0x648` deployed.
- **Strength blocks** (`0x007CE410(flag)` on a tracker, 7 ints): [0] men (`+0x90` = Σ unit men
  `+0x2CC`; with flag 0 `+0x94` = men of units not passing `0x0055B0A0`), [1] Σ melee ratings
  `0x0079C1E0`, [2] Σ missile `0x0079C940`, [3] 0 (stub `0x007CB9D0`), [4] Σ melee + missile of
  category-2 (infantry) units `0x00794B40`, [5] the same for mounted units `0x007A4EB0`, [6] total
  `0x007CCD60` (Σ melee + missile, ×0.5 for `+0x278D`, + ships). Kept per update: ours `+0x590..`
  (initial copy at the first update `+0x574..`), the enemy's `+0x5C8..` (initial `+0x5AC..`), and
  a third tracker at `+0x17C` (contents UNKNOWN) with flag 0 at `+0x600..` (initial `+0x5E4..`). Tracker stats (`0x0074B570`): `+0x78/+0x7C/+0x80` =
  % of men mounted / class 8 / with `+0x278D`, `+0x84/+0x88/+0x8C` the same over the units not
  passing `0x0055B0A0`, `+0x74` their count. `0x0074B700` sets `+0x99` when more than 75 % of the
  units stand where the ground falls away on all three sides (mean slope < −0.2): INFERRED **holds
  high ground** (the OUTFLANK weight's target flag). `+0x278D` is INFERRED **routing** (light cavalry
  ×4 / lancers ×2 against it in the class matchup: pursuit).
- **Plan vote** `0x007B1FB0` (12 counters, start `v0 = 1`; the previous plan, if not 0, gets +3):
  `O` = our total, `E` = enemy total (0 read as −1); `u = (O − E)·10 / E`, `i = (E − O)·10 / O`
  (integer). `hold` = (battle alliance `+0x70` = **defender**, CONFIRMED round 6: set by the alliance setup `0x005055E0` to "alliance index == battle setup `+0x88`" (called from `0x00544A70`), and the Lua condition `BattleAllianceIsAttacker` `0x0052A280` ("the attacker from the campaign map") is true for the alliance without it while another has it; `+0x88 == −1` means no defender) or the enemy's capture flag) and
  `alliance+0x64 == 0` and not (a second army with `+0x220`) and not `0x00796B80` (the enemy's
  emplaced artillery — pieces passing `0x0053EE30` — brings more than **1.5 ×** the missile power
  of all our artillery pieces). `losing` (needs our initial melee ≥ 1): our men lost ≥ 31 % and
  more than the enemy's %, or ≥ 15 % and more than 10 × the enemy's %. `outgunned`: our missile × 2
  < theirs and our melee < their melee + missile (cancelled by the enemy capture flag).
  - `E ≤ 0`: hold → v4 += 10; else no capture flag of ours → v0 += 100; else v6 += 100, v7 += 3.
  - `E < O/2`: v8 += u, v7 += u/2, +9 on v8 when our missile < 2 × theirs.
  - `E < O`: v7 += 3 + u; v6 += 3 + u/2; hold and u < 10 → v4 += 8 − u/2; outgunned → v6 gets
    +6 + u/2 instead (+15 + u/2 when also losing).
  - `E < 4/3·O`: v4 += 3, v6 += 3, then +3 more on v4 (hold) or v6 (not); outgunned → v6 += 3
    (+12 when losing).
  - `E < 2·O`: v4 += 3 + i; outgunned → v6 += 3 (+12 losing).
  - else: v2 += i, v4 += 12; outgunned → v2 += 5.
  Winner = highest counter (ties: the lower option). Then: the enemy capture flag → 3; our capture
  flag → 9; `O ≤ 0` → 0. A holding side withdraws (→ 1) when `0x0076A230`: deployed, every army's
  `+0xD4→+0xCC` set and `+0x2D4 == 0` (UNKNOWN flags), no enemy capture flag, plan 2, our total at
  most half its initial value and more than 1200 updates (2 min). Otherwise 2 → 4; a side that does
  **not** hold turns 2/4/5 into 6 (the attacker always attacks; 9 with our capture flag); the
  capture flags again (→ 3 / → 9). A non-holding side with plan 6..8 defends (4) when `0x007536C0`
  holds (no defender in the battle; > 1 unit of ours, all generals / melee infantry (class 0x13) /
  fixed artillery; every enemy unit mounted; someone can fire). Last: no `+0x64`, our `+0x70`, no
  enemy capture flag, plan 7 and the first army's `+0xD4→+0xB8 == 0.0` (UNKNOWN) → 4.
  Constant inputs: the time-limit test `0x004613B0` returns −1 (no limit), `0x0075BC80` is always
  false (count × 2500 < 0), `0x00462CB0` / `0x00462E90` return 0: plans 5, 10, 11 never win.
  The capture flags are `0x00557240` on the alliance's list `+0x4C` (an entry tied to the alliance
  whose virtual `+8` is true); INFERRED a **settlement-capture victory condition** (the "AI BATTLE
  GTA" log prints "Victory Conditions: CAPTURE SETTLEMENT / PREVENT SETTLEMENT CAPTURE / DEFEND").
- **Modes** (`0x007D7020`; handlers `0x007C8BD0`): plan 0 → **8 search** (`0x007C8E40`: no enemy
  strength seen; a move objective to the map origin within 100 m for the first 600 updates, then
  within 400 m of the enemy tracker's centre `+0x3F0`, from 2400 updates within 500 m of a point
  from `0x0075D3A0`); 1 and 2 → **9 withdraw** (`0x007CA2B0`, objective pool `+0x58`, vtable
  `0x01342B24`); 3 → mode kept; 4 → **3 defend** (`0x007C8E20`: `0x00769450` picks the defensive
  terrain feature (terrain analyser kind 3) best for our groups and adds a defend objective of
  radius 250; else `0x007695B0` builds a line between our strongest and the enemy's strongest
  group); 5 → 11; 6, 8 → **2 attack** (`0x007C8D80`: one ATTACK objective (vtable `0x013429D8`,
  pool `+4`) per enemy battlegroup with units or ships, priority its strength + 70; in mode 1 also
  one on `+0x630` with + 100); 7 → 11 if the mode is already 11 (outside deployment), else 2;
  9 → 10; 10 → `0x007AE7B0` (distance ≤ 200 m picks 2 or 3; unreachable); 11 → 13. Modes 1/2 run
  `0x007492F0(1)`, 8/13 `0x007492F0(0)` (objective resource allocation, not decoded); in modes
  1/2/8/13 while not deploying, `0x007A70C0` (our total below its start and no enemy
  battlegroups, or the `+0x17C` tracker's total < 51 % of the enemy's) adds the search objectives
  too. Battlegroup aggression
  `0x007B5D80`: `+0x138` = `+0x50` (÷4 outside modes 1/2) + 60 / + 40 / + 20 / − 20 by own v
  target strength (> 2×, >, ≥ ½, less).
- **Deployment step** `0x0076A580` (first update): with plan 9 `0x0076B870`; otherwise per unit,
  abilities 9/10/12/13 set up (stakes etc., `0x0076AB40/0x0076A700/0x0076A7F0`), units of a
  defender with flags `+0x1AD/+0x1AC/+0x1AB` are placed in cover (`0x0076A8A0`, nearest building /
  feature edge), and light infantry, irregulars and skirmishers (classes 0x10/0x11/0x16) switch on
  mode 2 (`0x005602A0(2, 1)`, INFERRED skirmish).
- **Battlegroup formation** `0x0075F0C0` = slot 0 of the ATTACK objective (vtable `0x013429D8`;
  slot 7 returns 2): two candidate lists are created empty and never filled (dead code for a
  cluster choice), so: when an army of our alliance has land units, **one battlegroup per army**
  of the alliance, from the current count up: army 0 gets the full battlegroup (`0x0070A980`,
  0x58 bytes: the objective's target `+0x13C` and flag `+0x14C`), the others `0x0070AE30` (0x5C,
  vtable `0x01342ECC`, same base with the flag 0). When an army has ships, one naval battlegroup
  (`0x0070AB10`, 0x50) on the target or the first enemy battlegroup with ships (list `+0x3C`).
  Slot 1 (`0x007C5140`): a unit goes to the battlegroup of **its own army** (`unit+0x1EC`), else the
  first. Slot 2 (`0x007C50F0`): the first battlegroup in GENERAL MELEE (`0x007949D0(7)`), else the
  first.
- Ported (round 5): `battle::plan` (blocks, vote, withdraw and composition tests, mode map) runs
  in every think; mode 3/9 → DEFEND_ABSTRACT, 2/1 → ATTACK_BATTLEGROUP. It replaces the
  PROVISIONAL "attack when balance ≥ 0.5" rule and the stalemate breaker. PROVISIONAL there:
  the defender flag (the model has none: default false, `BattleAi::set_defender`), the capture
  flags (false), the UNKNOWN flags (`+0x64`, `+0x220`, `+0xB8`; withdraw off), emplaced artillery
  (none), tracker membership (active units), routing as `+0x278D`, withdraw played as defend, no
  switch to defend while the line is in melee. One battlegroup per army: our sides have one army.

### 3.1d The tactic auction in detail (round 5, CONFIRMED unless tagged)
- **Tactic order** (battlegroup ctor `0x0070A980`): ATTACK_BATTLEGROUP, OUTFLANK,
  DOUBLE_ENVELOPMENT, STOP_AND_SHOOT, LIMBERED_ARTILLERY, GENERAL_SUPPORT. **Type ids** (virtual
  `+0x54`): 6, 14, 15, 16, 18 (GENERAL_SUPPORT not read).
- **Auction** `0x00751800`: (1) reset every tactic; release the units of an active tactic whose
  keep test fails (entries `{unit, tactic, used}` of the land list `+0x40`; the second list
  `+0x30` is INFERRED ships) and deactivate it; (2) `n` = number of inactive tactics; while
  `n > 0`: each inactive tactic that is not disabled (`+0x49`) and that the battlegroup accepts is
  scored on the free list; the highest positive score wins (ties: earlier), a zero score does
  `n −= 1`; no winner → stop; the winner gets `+0x0C(1)`, claims (slot 16 land, slot 17 ships:
  every free ship), the claimed entries are marked used, `n −= 1`, and it is activated when its
  slot 9 passes (default `0x007CB3F0`: it has units or ships; STOP_AND_SHOOT: its flag `+0x84`);
  (3) the tactics that were active before the round claim again (top-up); (4) the battlegroup's
  default tactic (`+0x20`) claims what is left; (5) a tactic whose slot 10 holds is deactivated
  (`0x007CB840`; never for STOP_AND_SHOOT); `+0x1C` = the first active tactic.
- **Accept** (`0x007CD4B0`): with the tactic's `+0x4A` flag set and the battlegroup phase equal to
  its `+0x44` only tactics whose slot 14 is true (ATTACK_BATTLEGROUP, LIMBERED_ARTILLERY) pass
  (round 6: `+0x4A` is set, with `+0x4B` and `+0x44` = the battlegroup phase, by `0x00754780`, which OUTFLANK's / DOUBLE_ENVELOPMENT's start `0x007547D0` / `0x00754D80` and GENERAL_SUPPORT's `0x0085E220` call; no store clears it, INFERRED it stays set; ported as `auction::commit_allows`); a tactic is refused while a tactic of a paired type is active
  (`0x0075DD00`, table `0x01453450`: (14, 15), (14, 16), (15, 16): **OUTFLANK,
  DOUBLE_ENVELOPMENT and STOP_AND_SHOOT exclude each other**) and OUTFLANK / DOUBLE_ENVELOPMENT
  are refused while the other has `+0x4A` (`0x0075DD90`).
- **Claims** (slot 16): ATTACK_BATTLEGROUP (`0x00752230` → `0x007515A0`) takes the free units
  grouped into clusters (`0x007D6030(1, 40 m, 400 m)`): the strongest cluster (or the one nearest
  to the target when `+0x81C == 1`) plus every cluster within 40 m of it; OUTFLANK (`0x007523F0`,
  only while inactive): sets its point (`0x007AE2C0`), `n = free / 2`, when `n > 1` claims
  `seed0 % n + 1` units; DOUBLE_ENVELOPMENT (`0x007522A0`, inactive): `n = min(free / 2 + 1,
  free units its filter passes)`, when `n > 1` claims `seed0 % (n − 1) + 2`; STOP_AND_SHOOT
  (`0x00752270`): no units, sets `+0x84`; LIMBERED_ARTILLERY (`0x00752350`): every unit its filter
  passes. The picker `0x0075CCC0` takes units whose filter gives **2** first, then 1, each pass
  scanning the free list **from the back**. Random draws `0x007CD530(i, lo, hi)` = `seed[i] %
  (hi − lo + 1) + lo`, `0x007CD510(i, n)` = `seed[i] % (n + 1)`.
- **Scores**: OUTFLANK = `100 + seed2 % 65`, DOUBLE_ENVELOPMENT = `80 + seed1 % 65` (both
  random), STOP_AND_SHOOT 164, LIMBERED_ARTILLERY 164, ATTACK_BATTLEGROUP 1. Operand order now
  CONFIRMED from the listings: OUTFLANK needs **ours ≥ 0.6 × the target's** strength,
  DOUBLE_ENVELOPMENT **ours ≥ target × 4 / 5** (integer); weaker: ±8 (OUTFLANK) / ±6 (DE) by
  whether the target has fewer units than our group; stronger with more units +4. Class-8 term:
  +5 when the target's `+0x88` > 30, +7 when also above our `+0x7C`. DE's +6: our `+0x78`
  (mounted %) above target `+0x84 × 3 / 2`. "Our group" is a group built from the free units
  (`0x0079AD70`, its `+0x5C` count).
- **Filters**: OUTFLANK `0x007D34A0` (mounted, elephants, light infantry 0x11, skirmishers 0x16;
  not the general's unit; unit state != 6) gives 2 when the unit stands on the same side of the
  group's axis as the outflank point (`0x007D0870`), else 1; DOUBLE_ENVELOPMENT `0x007D33B0` the
  same units, always 2; STOP_AND_SHOOT 1 for every unit; LIMBERED_ARTILLERY `0x007D3400` artillery
  pieces not engaged, deployed (`+0xB47`) or near their position.
- **Outflank side and point test** (round 6, CONFIRMED; `0x007AE2C0`, `0x0078E3E0`). The angle
  `DAT_0150BD14` is a 16-bit constant **16384 = 90°** (written by the static init `0x00415BB0`).
  The reference is the target battlegroup's record (`0x0078B070`: map, 16-bit facing, centre,
  width, depth, axes). For each side (`∓90°` off the facing) the point test tries `k = 0..9`
  directions turned by `0, +1, −1, +3, −3, +5, −5, +7, −7, +9` × 1820 units (≈ 10°): the point
  `centre + dir × d` (`d` = 200 m in CLOSE APPROACH, else 100 m, `0x007AA0D0`) is pushed out in
  10 m steps while its squared distance to the target's rectangle (`0x005C3250`, 0 inside) is
  below `d²`, then pulled back by `0.2 d` (up to 4 times) while it lies outside the battle area
  (map `+0x84..+0x90`, max exclusive); a `d × d` square at the point, oriented along `dir`, must
  pass the terrain query `0x007E1BD0` / `0x007F1D40` (an obstacle test, details not decoded);
  the first passing point wins, else none. Side choice: one valid side → it; both → cost =
  bit-trick square root of the squared distance from our group's centre (`0x0079AD20`) minus
  `5 × min(h(target centre) − h(point), 15)` (`0x0064C5F0` heights; constants `0x01325D24` = 5,
  `0x01325D44` = 15); `cost(+) ≤ cost(−)` → the `+` side (2), else `−` (1); point and side stored
  at the tactic's `+0x94/+0x98` / `+0x90`. Ported: `battle::outflank` (CONFIRMED), used by
  OUTFLANK's and DOUBLE_ENVELOPMENT's scores (a point on one / both sides) and as OUTFLANK's
  destination. PROVISIONAL inputs: the target rectangle = the enemy army (mean facing, unit
  spread), the battle area = the height grid, the terrain query = inside that area.
- LIMBERED_ARTILLERY keep `0x0075EBA0`: all its units are artillery pieces and none of our
  artillery pieces is valid, not `0x0055C480` and undeployed (`+0xB47 == 0`).
- Phase proposal (slot 19 of OUTFLANK / DE, `0x007AE230`): for next phase 5..7 every sub-move must
  be in state `+0x788 == 1` and within a distance test, else it agrees.
- **Ported (round 5)**: `battle::auction` (the algorithm, exclusions, claim counts, picker) and
  `WingBidder` in `battle::mod` run every think in attack modes: OUTFLANK and DOUBLE_ENVELOPMENT
  bid with the CONFIRMED scores and claim units (light infantry and skirmishers can now join a
  wing), ATTACK_BATTLEGROUP is the default and keeps the rest (the line and the reserve cavalry),
  STOP_AND_SHOOT's active flag mirrors our line FSM so the exclusions hold both ways. PROVISIONAL:
  the default tactic's leftovers count as free at the next auction (INFERRED: else it would hold
  every unit for good); ATTACK's cluster rule (it takes every leftover unit); the outflank points
  (always found, symmetric beyond the enemy flank, nearer side, heights left out); the
  sub-move-running part of the keep test (one of the tactic's units still fights); the target
  battlegroup = the enemy army; class-8 / mounted percentages over men (`+0x204` UNKNOWN);
  DOUBLE_ENVELOPMENT's per-unit side = the side the unit stands on; the target high-ground flag
  (`+0x99`, decoded in §3.1c) is not computed (the AI plans on flat ground); in defend modes the
  round-3 cavalry rule stays.

### 3.2 Tactic FSMs (CONFIRMED from their debug-display functions)
- ATTACK_BATTLEGROUP (`0x00765540`, state at tactic `+0x828`): 0 Change Formation, 2 Reform,
  3 Move to Form-up, 4 Move to Target, 5 Outflank; it also reports "Formed=%u%%".
- DEFEND_ABSTRACT (`0x007659C0`, state `+0xAC`, tight/loose flag `+0xA5`): 0 Change Formation,
  1 Change Unit Formations, 2 Reform, 3 Defend Line; plus the formation and "%u percent".
- STOP_AND_SHOOT (`0x00766E30`): efficiency = (kills, tick) now vs the previous sample and their
  gradient; units with state `+0xB34 == 6` are left out of its list.
- **Tactic vtables** (CONFIRMED by `vt:` dumps): the display function is slot 8 of each tactic's
  primary vtable (ATTACK_BATTLEGROUP's at `0x013430C8`; a 30-slot family at `0x01344AC4` +
  `0x78·k` holds four more tactics incl. STOP_AND_SHOOT, display `0x00766E30` in slot 8). Slots
  30..35 (`+0x78..+0x8C`) are 6-byte thunks returning transition descriptors
  (`0x01453C1C..0x01453C30`; e.g. `+0x80` → `0x01453C24`). The descriptors are filled at run time
  (no static writer found).
- **ATTACK_BATTLEGROUP FSM** (CONFIRMED structure; transition targets INFERRED from what each
  condition tests and what each state's entry does). Each state has an entry thunk, an exit thunk
  and an update thunk in `0x00702940..0x00702CCA`; the update calls condition functions in order
  and returns the first transition whose condition holds:

  | state (entry fn) | update: condition → transition (vtable offset) |
  |---|---|
  | 1 (`0x007DD620`, orders unit formations) | all units done (`0x0079A6E0`) → `+0x7C` |
  | 0 Change Formation (`0x0075C660`) | formed (`0x007A6900`) → `+0x88`; formation change needed (`0x0075C6C0`) → `+0x80` |
  | 2 Reform (`0x007B5910`: formation at the group's position, facing the target; resets `+0x818`) | `+0x818 > 3`, or formed and facing within 60° (`0x007B5AF0`) → `+0x88`; not formed, `+0x80C > 160` and `+0x808 > 160` and a form-up rectangle that passes the path test (`0x007A5730`) → `+0x84` |
  | 3 Move to Form-up (`0x007A5A30`: computes the form-up point `+0x7E8`/facing `+0x800` with `0x007A5910`) | no target, or formed and at the point with its facing, or the point's rectangle fails (`0x007A5A80`) → `+0x88`; not engaged, `+0x80C <= 140` and not (formed and facing within 60°) (`0x007B5640`) → `+0x80`; outflank found (`0x007A5C80`) → `+0x8C` |
  | 4 Move to Target (`0x007A6910`) | `0x007B5640` → `+0x80`; `0x007A5730` → `+0x84`; `0x007A5C80` → `+0x8C` |
  | 5 Outflank (`0x007A6040`: target = the outflank point) | inside the group's rectangle at the point, or `0x0064F230`, or `Formed == 0` (`0x007A60A0`) → `+0x88` |

  INFERRED targets: `+0x80` Reform, `+0x84` Move to Form-up, `+0x88` Move to Target, `+0x8C`
  Outflank, `+0x7C` Change Formation. "Formed" (`0x00797100`) = units in formation
  (`0x007913E0`) and (`Formed% +0x7E0 >= 60` or no target). 60° = `0x2AAC` of a 16-bit circle.
  Outflank decision (`0x007A5C80`, once: flag `+0x823`): only if fewer than half the group's units
  are engaged and own/(own+enemy) of the `+0xBEC` ratings `<= 0.5` (unless both sides pass
  `0x0057A150(1)` for every unit); the point is searched at 1.1 × the distance to the target, at
  5°, 10° … 85°, trying one side first when `0x007ADD80() < 51` (round 6: `0x007ADD80` is the battle RNG's `percent_0_100`, W1 §12.1 CONFIRMED), first valid
  point (`0x007A6510` / `0x007A61F0`, not decoded) wins.
  Unit states (`+0xB34`) set from the tactic state by `0x007C8820`: Outflank → 1; Reform with
  Formed < 60 → 4 or 1; units with state 6/7 untouched.
  Our implementation: `ntw_ai::battle` `update_line_fsm` (Reform/Form-up/Target/Outflank with
  these rules; formed measure, engaged test and point validity PROVISIONAL).
- **Transition descriptors decoded (round 3, CONFIRMED).** Every FSM state is a static 20-byte
  descriptor `{update, entry, during, exit, name getter}`; the getter returns a global holding the
  state's name string. A tactic's transition slot (6-byte thunk in its vtable) returns a pointer
  to a global that holds the target state's descriptor. Ghidra command `fsmscan:LO:HI` in
  `AiDecomp.java` lists every descriptor with its name, entry/exit targets, the update's calls and
  the vtable slots that lead to it. Scan of `0x01330000..0x01370000`: 94 descriptors (battle AI,
  unit movement, naval boarding, artillery crews, the generals' RALLY/INSPIRE).
  ATTACK_BATTLEGROUP slots (vtable `0x013430C8`) by name: `+0x78` WAIT_FOR_LIMBERING_UNITS (the
  state the earlier notes called "1"), `+0x7C` CHANGE_FORMATION, `+0x80` REFORM, `+0x84`
  MOVE_TO_FORMUP, `+0x88` MOVE_TO_TARGET, `+0x8C` MOVE_TO_OUTFLANK: **the INFERRED targets of
  the table above are now CONFIRMED.**
- **DEFEND_ABSTRACT FSM** (CONFIRMED; vtable `0x01343164`, a second vtable `0x01343208` shares
  it; tactic = FSM object − 0x78; state number `+0xAC` written by each entry):

  | state (entry) | update: condition → target |
  |---|---|
  | CHANGE_FORMATION 0 (`0x0075C680`: picks the group formation `+0xA8` via vtable `+0x88`, orders it) | formation ready (`0x00791390`: the group has units `+0xDC` and the formation manager accepts `+0xA8`) → REFORM |
  | CHANGE_UNIT_FORMATIONS 1 (`0x0075C790`) | always (`0x00447880` returns 1) → REFORM |
  | REFORM 2 (`0x007B5A40`: clears the re-form flag `+0xA4` and counter `+0xB0`; orders the formation; every unit with ability 0x10 switched on turns it **off**) | ready and formed percent `+0x80C == 100` (`0x00797150`) → DEFEND_LINE |
  | DEFEND_LINE 3 (`0x007693D0`: every able unit (`0x0055C230`) turns ability 0x10 **on**) | `0x00462CB0` (returns 0: never) → CHANGE_UNIT_FORMATIONS; `0x007B5C00` → REFORM when the flag `+0xA4` is set, or the group changed (`0x0076D870`: a unit was removed, `+0x48`, or a unit flag `+0xB43` / `+0x1C86`), or the formation is not ready, or formed percent `< 50` |

  `+0x80C` is printed as "FORMATION=<name> (%u percent)" (CONFIRMED by the display), i.e. the
  formed percentage of the group. Ability 0x10 is also switched off before any group move
  (`0x0083FD00`), so it is a stationary mode; which one (UI name) is UNKNOWN.
- **STOP_AND_SHOOT FSM** (CONFIRMED; vtable `0x01344CA4`, tactic = FSM object − 0x88; group
  `+0x80`; firing line segment `+0xAC..+0xB8`, valid flag `+0xC0`; efficiency sample
  `+0x9C = {kills0, tick0, kills1, tick1}`):

  | state (entry) | update: condition → target |
  |---|---|
  | ADVANCING_TOWARDS_LINE `+0x78` (`0x00748D50`: computes the firing line `0x0085B530`, moves the group to its midpoint facing across it, unformed move flag 0) | line valid and the group within **20 m** (`< 400` squared) of the line's midpoint (`0x007CE370`) → FORM_ON_LINE |
  | FORM_ON_LINE `+0x7C` (`0x007912B0`: same move with the formed flag 1) | line valid and every unit has finished moving (`+0x930 == 0`, `0x00791440`) → HOLD_THE_LINE |
  | CREEP_FORWARD `+0x80` (`0x00762B70`: the creep line `0x0085BDC0`, formed move) | as FORM_ON_LINE → HOLD_THE_LINE |
  | HOLD_THE_LINE `+0x84` (`0x00794A50`: efficiency sample reset to now on both ends; every unit that can fire gets unit state `+0xB34 = 4`, others 0) | not efficient (`0x0079A720` = !`0x00796DF0`) → CREEP_FORWARD |

  Efficiency (`0x00796DF0`, CONFIRMED): a sample is `(mean of the units' kill counter +0xC9C,
  battle tick)` (`0x00762E20`; `+0xC9C` = kills is INFERRED). Not efficient when the latest sample
  is more than **200 ticks** old, or when `(kills1 − kills0) / (tick1 − tick0) < 0.01` (mean kills
  per unit per tick since the hold began); efficient while both samples are the same tick. The
  sampler that refreshes `kills1/tick1` is `0x007D9190` (period not decoded; PROVISIONAL: every
  think).
  Creep line (`0x0085BDC0`, CONFIRMED): `range` = the **shortest** missile range among the group's
  shooters (`0x0085BCE0`); if the target is farther than that, the line moves `distance − range +
  3 m` towards the target; the line keeps the group's half-width `+0x2C` across the direction.
  Firing line (`0x0085B530`): with no candidate points (`+0x10 == 0`) it is the creep line; when
  our mean range beats the target's (`0x0085BC40`), it is placed at our **longest** range from the
  target, facing it (CONFIRMED); otherwise it picks from sorted candidate points (`+0x14`, 12-byte
  entries) the one that brings the most missile power to bear (`0x0085B420`/`0x006B05B0`;
  round 6 CONFIRMED: the candidates are sorted by `0x0085B150` (std::sort, comparator `0x0085B3C0`: squared distance to the target rectangle, then the third value, ascending); walking them in that order with our shooter list, each candidate's offset is `sqrt(d²) × u` (`u` = unit vector from the target's centre to ours); every remaining shooter that would reach a target unit translated by that offset (`0x0085B420` → `0x0055B220` with the range `0x005646A0`) is consumed and its missile potential (`0x006B05B0`) added to a running total `F`, the candidate's third value to a running total `W`; a candidate that consumed a shooter is taken when `W < F`, or (with `F > 0`) when `W − F` is below the best deficit so far. Who fills the candidate list (object `+0x10/+0x14`, 12-byte `{x, y, w}`) is UNKNOWN, so this is not ported: we use the creep line there, PROVISIONAL). Missile range of a unit
  (`0x005646A0`, CONFIRMED): weapon range, ×0.8 while ability 7 is active, plus a bonus on walls.
  No transition leaves the FSM: the planner/melee manager ends it (melee objectives override).
- **GENERAL_SUPPORT FSM** (CONFIRMED, vtable `0x0134B490`): RALLY `+0x78` (entry `0x0085E7A0`:
  each general rides to a different unit chosen by `0x0085D8D0` from the units that pass a virtual
  test, to a point behind it; during `0x0085EA10`: flag `+0x84` once every general stopped; exit
  `0x0085E9B0`: every general uses ability **0x11** (rally)); update: `+0x84` → RALLY again.
  INSPIRE `+0x7C` (entry `0x0085E4D0`: each general uses ability **0x12** (inspire) on the unit
  `0x0085D640` picks); update: `+0x85` → RALLY. Not ported: the sim has no generals or abilities.
- Other FSMs found by the scan (not AI tactics; for the sim workers): unit group MARCH /
  DEFEND_LINE / DEFEND_SQUARE / ATTACK (`0x0133219C..`), movement FIND_PATH / FACE_TARGET /
  QUEUEING, artillery crew FIRE / MOVE_TO_WAYPOINT_1/2 / MOVE_TO_RELOAD_LOCATION / RELOAD
  (`0x01346E30..`), naval boarding and tacking states.
- OUTFLANK has no descriptor of this kind (its "Approaching/Outflanking" display strings are not
  referenced as FSM names; UNKNOWN how it switches).

### 3.3 Melee manager (CONFIRMED structure)
- Analysers walk the enemy alliance's units. A **valid target** (`0x0055CBD0`): unit active state
  `+0xAA0` not 0 and not 2, and it has soldiers (`+0x2CC != 0`).
- **Aggression** (`0x00749E90`): `b / (1.01 - b)` where `b` = strength balance `0x006A31E0` =
  own / (own + enemy), 1 if the enemy has nothing, 0 if we have nothing; summed over all units
  (melee term `0x006AFB70` + missile term `0x006B05B0`, ships `0x006B1530`), each term scaled by
  `1 - total casualty ratio` (`+0xCB4`), units with `+0xC2C == 3` or `+0xC28 == 7` excluded.
- **Target value** `0x007D3240`: `0.3 * missile + 0.7 * melee` rating (`+0xBEC`, `+0xBE8`); melee
  ×0.25 when an UNKNOWN target flag is set.
- **Melee base priority** `0x00755BF0` = target value (objective `+0x34`); it also stores
  `+0x40 = 0.75`, `+0x3C = 2.5`, `+0x38 = 0`. A second melee objective kind (`0x00755D10`, built by
  `0x0074A8B0`) uses value × 0.25 with `+0x40 = 0.5`, `+0x3C = 2.5`; another (`0x00755D80`, from
  `0x0070B910`) `+0x40 = 0`, `+0x3C = 1.0`; missile objectives set both to 1000.
- **Objective classes and their virtuals (round 3, CONFIRMED).** The pool objects are built in
  place: melee objective constructor `0x00709D00` sets vtable **`0x01343FE4`**; missile objective
  constructor `0x0070B700` sets vtable **`0x01344058`**. Objective fields: `+0x08` kind/state,
  `+0x34` base priority, `+0x38` accumulated potential of the attackers assigned, `+0x3C` cap,
  `+0x40` minimum, `+0x5C` target unit. Virtuals (the notes' earlier "+0x14 / +0x18" were these):
  - `+0x14` **priority of one sub-objective for one unit**. Melee `0x007D0D10`: 0 for units in
    state `+0xB34 == 7`; `p = base² × shape(sub potential, 1.7, 0.3)` (`0x006B0A00`: potential
    clamped to `0..2m`, `1 − d²/m²` below the peak `m`, `1 − (1−low)·d²/m²` above); a shooter that
    is not engaged gets 0 beyond its range (dist² > range·range) or while it is still moving to an
    order of its own; then, while the objective is not closed (`+0x08 != 2`) and has room
    (`cap − accumulated > 0`, else 0): × class matchup `0x007D23E0` (switch on the attacker's
    class code `+0x2C→+0x20`: 4, 6, 7, 9/10, 0xD, 0x11, 0x12, 0x16; category `+0x2C→+0x1C`: 1 → 0
    (no melee), 5 against 0/3 → ×2 or ×3; codes not mapped to names, UNKNOWN) × building factor
    `0x007D2FB0` (target in a building: ×4, or ×2 / ×0.25 by attacker class) × 0.5 (a sub-objective
    flag test) × **0.3 when some potential is assigned and the room left is smaller than this
    attacker's potential**; per unit state: 0/2/3 a ×0.1 test against the target's strength, 4
    needs a free target, 6/8 × `0x007CD740` (0.2, or 0 / 0.01 for state 0x11); × `0x00792820` (0.1
    when attacker and target are in different areas `0x007F1CF0`, ×0.5 by another flag); shooters
    (can fire, category != 4): target missile rating 0 → ×0.1, melee rating 0 → ×2, else
    `r = own missile / its missile`: `r ≤ own melee / its melee` → ×0.5, else ×min(1/r, 0.5);
    × **distance/turn factor `0x0079C0D0` = `1 / (pen + 1 + d/15)`** (pen beyond 5 m by the turn
    from the unit's facing (or its group's, in state 1 with > 3 units and formed > 79): ≤10° 0,
    ≤22.5° 0.5, ≤45° 2, ≤90° 3.5, else 5; state 1 doubles it and ignores targets > 105 m ahead
    unless `0x0064F230`); ×0.5 when another of our shooters has the target in range in front of it
    (`0x00796910`); shooters again: beyond 1.5 × range², target facing them within 45° and not in
    melee → ×0.001, else ×2.
    Missile `0x007D1510`: `p = base² × shape(potential, 1.0, 0.3) × class factor 0x007D2CF0`
    (class 0x12/0x16 against 0xE/0x12/0x14 ×2 or ×4; engaged attackers ×(1..10) by the units within
    160 m; 0 for some far cases); ×2 when the unit's current order already targets it; ×0.25 for a
    target in a building; ×0.1 when the target's whole frontage is behind the attacker; ×
    **`0x0079C7E0` = `1 / (0.5·(d/10 + pen) + 1)`** (pen beyond 5 m: ≤22.5° 0, ≤45° 1, ≤90° 2.5,
    else 4; state 1: 0 for targets beyond a tenth of the range ahead when a test passes); melee/missile
    ratio `0x006B0500` = `melee pot / (missile pot + 0.25 if melee < 0.3 and missile > 0.05)`: state 1
    → 0 above 2; others ×0.25 above 3; state 6 × `0x007CD790`.
  - `+0x18` **minimum test** `0x0079C730(unit, sub, priority)`: `priority / (sub+0x40)²` (the
    attacker's melee or missile rating) must reach 0.005 (unit state 1), 0.3 (state 2) or 0.0005
    (state 3); other states always pass.
  - `+0x1C` **assign**: `accumulated += sub potential` (melee `0x007CF5A0` also adds `+0x3C` of the
    sub into `+0x70` and handles shared targets; missile `0x007CF7C0`).
  - `+0x20` **fulfilled** (melee `0x0079BF40`): kind 1: `minimum ≤ accumulated` (and `+0x28`);
    kind 0: compares `accumulated × base` with `base × (0.8, or 0.6 with flag +0x74) × min(aggression,
    1) × 0x00757450` (1, 0.8 or 0.65 by the units' states) plus 0.75 × the other attackers' share.
  - Missile sub-objective (`0x00792440`): `+0x38` potential = `0x006B06F0` = `clamp((A + men ×
    dir_table + height term) / T, 0, 2) × 0.5` on the missile ratings `+0xBEC`, `+0x40` = attacker
    missile rating; kind 3.
  Ours: `rating::{priority_shape, melee_distance_factor, missile_distance_factor, meets_minimum,
  missile_potential}` and `melee_manager::capped_priority` (cap 2.5, ×0.3 rule; replaces the
  PROVISIONAL ×0.75 sharing factor). PROVISIONAL / left out: class codes (we use cavalry v infantry
  ×2/×3), buildings, areas, unit states other than 1 (OUTFLANK) and 4, the direction table and the
  height term of the missile potential, the attacker extent `+0x670`.
- **Class codes (round 4, CONFIRMED).** `unit+0x2C` is the unit type record: `+0x1C` category,
  `+0x20` class (the script condition `0x00531E50` compares both). Enums from the `units` keys:
  category `0x00EED2B0`: cavalry 0, artillery 1 (also any unknown key), infantry 2, dragoons 3,
  elephants 4, cavalry_camels 5, naval_line_of_battle 6, naval_frigate 7, naval_galley 8,
  naval_specialist 9, naval_auxiliary 10, naval_merchant 11, naval_invasion_fleet 12. Class
  `0x00EED3E0`: artillery_fixed 0, artillery_foot 1, artillery_horse 2, cavalry_camels 3,
  cavalry_heavy 4, cavalry_irregular 5, cavalry_lancers 6, cavalry_light 7, cavalry_missile 8,
  cavalry_standard 9, dragoons 10, elephants 11, general 12, infantry_berserker 13, infantry_elite
  14, infantry_grenadiers 15, infantry_irregulars 16, infantry_light 17, infantry_line 18,
  infantry_melee 19, infantry_militia 20, infantry_mob 21, infantry_skirmishers 22, naval_* 23..45
  (admiral, bomb_ketch, brig, dhow, fifth_rate, first_rate, fourth_rate, galleon, heavy_galley,
  indiaman, light_galley, lugger, medium_galley, over_first_rate, razee, rocket_ship, second_rate,
  sixth_rate, sloop, steam_ship, third_rate, xebec, transport); empty key 0x2E.
  Unit predicates: `0x0055ABF0` mounted = category 0 or 5, or 3 without ability 0xF;
  `0x0055AB90` = category artillery with `+0x1F4→+0x10 != 0`; `0x0055C1C0` = infantry, or that
  artillery case reversed, or ability 0xF (INFERRED "on foot"); `0x0053EFE0` = in a building of
  type 0x13/0x14 or garrisoned (INFERRED).
  **Melee class matchup `0x007D23E0`** (so "cavalry v infantry ×2/×3" was wrong): artillery 0;
  camels v cavalry/dragoons ×2 (×3 with `0x0055B200`); then by attacker class: heavy cavalry ×2
  v cavalry/dragoons when `0x0055B200` is false and `0x0055AC20` holds (both UNKNOWN state
  virtuals); lancers ×2 v infantry_light, ×1.5 when the target does not face the attacker;
  light cavalry (unless target flag `+0xADC→+0x278D`): ×0.1 when the target faces it, else ×1.5 v
  artillery, then ×2 with `0x0055B200`; standard cavalry / dragoons ×0.1 when the target faces
  them; berserker 0 in some case, ×2 v line/elite/melee infantry; light infantry 0.5 (0.1 while
  ability 4); line infantry 0 in some building cases (`0x0053EFE0`, ratings); skirmishers 0 while
  `0x0057A150(1)`; the general's unit (`0x0055AC40`) 0 with any enemy within 160 m, else 0.01;
  finally, with ability 6 active, ×0.5 when the target faces away, else ×2. "Faces" = the
  target's facing (cos/sin table `0x0176CFF8` by the 16-bit angle at `+0xADC→+0x64C`) · (attacker
  − target) > 0 (INFERRED sign). **Missile class factor `0x007D2CF0`**: guns × clamp(units within
  160 m of the target, 1, 10) (×10 with an UNKNOWN shot flag v artillery; 0 v mounted targets
  beyond 160 m when `0x0055C320`); light cavalry 0 when its facing opposes the target's; line
  infantry ×2 v elite/line/militia; skirmishers on foot ×2, ×4 v elite/line/militia.
  **`0x007D2FB0`** (melee): target flag `+0x278D`: lancers ×2, light cavalry ×4, others ×0.25;
  else flag `+0x278E` ×4 (the flags' meaning UNKNOWN; earlier read as "in a building").
  Ported: `ntw_ai::battle::classes` (enums, `melee_class_factor`, `missile_class_factor`) using
  the new additive `LandUnit::{unit_class, unit_category}` (set by the app's battle setup from
  the `units` row). Left out: the UNKNOWN predicates above, ability 6, the `+0x278D/E` flags.
- **Melee sub-objective** (`0x00791FD0`, CONFIRMED fields): `+0x30` attacker, `+0x34` = 0,
  `+0x38` = potential `0x0074BDD0` (→ `0x006AFC00`), `+0x40` = attacker melee rating `+0xBE8`.
- **Missile base priority** `0x00755C50` = target men × 0.01 × value; ×2 for an UNKNOWN formation
  state (0, 5, or 3 without flag 0xF); ×0.25 if the target is engaged (`0x0054EE90`, INFERRED).
- **Melee sub-objective potential** `0x006AFC00`: `(A + men_att × dir_table[dir] − (men_def − men_att)
  × terrain_delta) / T`, clamped 0..2, ×0.5 (A, T = melee ratings; dir from `0x006AD890`, table
  values UNKNOWN; the terrain values come from `0x0064C5F0`, INFERRED heights).
- **Force melee** (`0x00791FD0`): an attacker within 50 m (`< 2500`) of its target is
  force-assigned ("Force Assign Override Level"). `0x007D05F0` (should melee) uses 75 m (`5625`)
  and a facing check (quarter turn).
- **Objective types**: 0 MELEE, 3 MISSILE, 4 RETREAT. Missile targets out of range are dropped.
- **Update rhythm** (CONFIRMED counters, `0x007B25A0` = one alliance-AI update; INFERRED: one
  update per 0.1 s battle tick, the caller is virtual): every update refreshes unit positions for
  the melee manager; when `counter % 10 == 1` the planner steps `0x007D7020` and `0x0075C880` run;
  when `counter % (N + 20) == 1` (`N` = a config value at `+0x28 → +0x10`, UNKNOWN) `0x006387F0`
  runs. The melee manager (`0x007DB360`) clears and re-allocates all objectives when its own counter
  `% 30 == 0` ("Clearing all current Melee objectives"), looping `0x007496E0` until
  `0x0076D9E0` says done. Ours: think every 10 ticks, melee re-plan every 30 ticks (plus a
  PROVISIONAL early re-plan when an objective became invalid).
  Caller chain (round 3, CONFIRMED): battle phase dispatcher `0x00580640` → battle-running phase
  update `0x00583480` → `0x00582FB0` → `0x005832C0`, which calls every alliance's AI update once
  (`0x00583380`: `alliance+0xC` = the AI object, tail-jump to `0x007B25A0`); in deployment
  `0x00583820` → `0x00583100` runs `0x005832C0` once (flag `+0x32F`), only from battle tick 8 on (or
  when a flag `+0x32E` is set). The battle tick `0x0057DF40` calls the phase dispatcher once and then
  increments the tick counter `+0x58` (the "Tick %d" of the AI logs; time = tick × 0.1 s). So: **one
  alliance-AI update per battle tick (CONFIRMED)**, plus a single deployment update (per
  phase). `0x005832C0` stops updating further alliances when, in
  an UNKNOWN mode `0x0055AD60`, a unit has `+0x224 == 0` and `+0xD4→+0x38 == 0`.
- **Allocation** `0x007496E0`: repeatedly, every unassigned unit's best and second-best objective
  that meets its minimum priority is found; the unit with the largest best-minus-second difference
  is assigned; repeat until none is left; then leftover objectives take their best candidate.
- **Strength potentials** (also the `+0xBE8/+0xBEC` ratings, INFERRED):
  melee `0x00757120` = `((defence + shield) × 0.05 + armour × 0.3 + attack × 0.06 + charge × 0.04
  + bonus_vs_cav × 0.03 + class_term) × men + morale × (10 or 5) + flag bonuses (30..70)`;
  small-arms missile `0x007575A0` = `(2 × accuracy + 3 × damage + 0.3 × range) / max((100 −
  reload_skill) × 0.01 × reload_time, 0.01) × men + ability bonuses`.

## 4. Campaign AI structure (exe)
- `EmpireCampaign\Source\CAI\` (CONFIRMED): BDI architecture (beliefs, desires, intentions):
  `CAI_BDI_POOL` (BELIEFS / DESIRES / INTENTIONS / FAILED), `CAI_CENTRAL_BDI_POOL`,
  `CAI_FACTION_BDI_POOL` with ATTITUDE / FINANCE / MISSION / SAVINGS managers, a director pool
  (`CDIR_*`).
- ~60 analysers (`CAIAT_*`): threat and support, defence/invasion strength, target, region
  occupancy, high-level pathfinder (`HLP`), unit availability, unit balance (`cdir_*`), construction
  balance, taxation, technology, trade, diplomacy, friends and enemies, faction-to-faction attitude.
- Desires (`CAI_BDI_DESIRE_*`) and goals (`CAI_BDI_GOAL_*`) mirror the DB behaviours: expansion,
  region (group/coast) defence, recapture, raid, help ally at war, navy distribution/recruitment/
  repair, research, taxation, trade, merge units, recall, fort maintenance, military rank.
- Diplomacy: `CAI_DIPLOMATIC_GOAL_*` (offer payment, request access/alliance break/join war/
  payment/region/technology), `EMPIRECAMPAIGNAI::NEGOTIATION` and `DIPLOMATIC_ACTION` (Lua-bound).
- Debug outputs: `.ai_history.xml/.html`, `.ai_log.xml`, `.ai_world_state.html`, `cai_pf_debug.txt`;
  80 files under `logs\cai\` in data.pack: only faction flag JPEGs for the HTML debug log (round 5;
  no decisions).
- The startpos carries the full CAI state (`CAI_INTERFACE`, the largest block; ~23 600 beliefs,
  8 700 desires, 1 400 intentions for eur). v1 does not read it.
- **CORRECTION (round 6): the name → class table below is shifted by one row.** Read from the
  listing of `0x00C90960` (each `PUSH name; CALL 0x004F0F50; JZ next` falls through to its own
  constructor): HELP_ALLY_AT_WAR `0x00C7E370` (id 0x10C, vtable `0x0138AFB4`, step `0x00CD5C30`),
  MINIMUM_SAVING `0x00C7DFE0`, NAVY_ATTACK_BEHAVIOUR `0x00C7EAE0`, NAVY_DISTRIBUTION `0x00C7EEA0`,
  NAVY_RECRUITMENT `0x00C7F230`, NAVY_REPAIR `0x00C7F5A0`, PORT_BLOCKADE_BEHAVIOUR `0x00C7F890`,
  **REGION_COAST_DEFENCE `0x00C7D3A0`** (id 0xF1, vtable `0x0138B66C`: the step `0x00CD4F90` with
  the `1, 0.5, 0.25, …` rule below is **this** behaviour's), **REGION_DEFENCE `0x00C7CC30`** (id
  0x11B, vtable `0x0138B7D0`, step `0x00CD4AE0`, refresh `0x00D0E090`), **REGION_GROUP_DEFENCE
  `0x00D25340`** (id 0xCE, vtable `0x01391F28`: the step `0x00D65650` below, making
  `CAI_BDI_GOAL_REGION_GROUP_DEFENCE` goals, is **this** behaviour's), **EXPANSION_BEHAVIOUR
  `0x00C7DE30`** (id 0xCF, vtable `0x0138BC78`, step `0x00CD5B10`, refresh `0x00D0E5D0`),
  REGIONAL_DEVELOPMENT `0x00D26900` (step: empty stub `0x00A790F0`), **RESEARCH_TECHNOLOGY
  `0x00BDFF00`** (id 0x10F, vtable `0x01382578`; step `0x00C2AD60` makes
  `CAI_BDI_GOAL_RESEARCH_TECHNOLOGY` goals, vtable `0x013825B4`; what round 4 called TAXATION),
  **TAXATION `0x00BE04A0`** (id 0x139, vtable `0x01383710`, step `0x00C2BBB0`, refresh
  `0x00C5EF60`), TRADE_AREA `0x00BE0E50`, TRADE `0x00BE1000`, TRADE_ROUTE_RAIDING `0x00BE1160`,
  MILITARY_RANK `0x00C7E5D0`, RAID `0x00C7FBD0`, RECALL `0x00D26540`, RECAPTURE `0x00C7FEF0`,
  CULL_EXCESS_FORCES `0x00C7C910`, NAVY_STRENGTH_MANAGER `0x00C7F6F0`, FORT_MAINTAINANCE
  `0x00C806F0` (step `0x00CD8D70`), **GLOBAL_CONSTRUCTION `0x00C80B20`** (id 0x165, vtable
  `0x0138BDB0`; step = the empty stub, its work is in the refresh slot 9 `0x00C5EE30`, not
  decoded), MISSIONARY `0x00C7E900` (step `0x00CD6C10`), **EXCESS_RECRUITMENT `0x00D26BB0`** (the
  weighted new-unit recruitment, step `0x00D68380`, see "EXCESS_RECRUITMENT" below), **MERGE_UNITS
  `0x00C7E4A0`** (id 0x177, vtable `0x0138BE70`, step `0x00CD6260`: the per-faction "count > 3"
  rule that round 4 filed under WAR_AND_PEACE), and **WAR_AND_PEACE and DIPLOMACY_MANAGER build
  nothing** (their compares fall through to the loop end). After the rows the manager always adds
  `0x00C74990(10.0)` (id 0xDC, vtable `0x013891C0`, step `0x00CCD420`) to the intention list. The
  "round 3" table below and the round-4 notes that use it (EXPANSION, REGION_DEFENCE,
  WAR_AND_PEACE, MERGE_UNITS, TAXATION, GLOBAL_CONSTRUCTION, EXCESS_RECRUITMENT) are kept for the
  record, read with this mapping.
  - **REGION_DEFENCE** (round 6, CONFIRMED): refresh `0x00D0E090` lists **every region of the
    faction** (`0x0047B080` list) with its integer value (belief `0x00A74200(region)` →
    `0x00A63FD0`), total = running integer sum (1 when 0); unchanged list → no work. Step
    `0x00CD4AE0`: finds or creates one desire per listed region (0x144 bytes, vtable `0x01378770`),
    drops the desires of regions no longer listed (`0x00D04710`), then links each with **mult =
    `value × (1 − k) × N / total + k`**, `k` = personality `+0x290`
    **BASIC_DESIRES_DEFEND_REGION_GOAL_BASE_COMPONENT_PROPORTION** (25 in `default`), `N` = number
    of desires (the same formula as REGION_GROUP_DEFENCE's).
  - **EXPANSION_BEHAVIOUR** (round 6, CONFIRMED structure): refresh `0x00D0E5D0` stores a count
    (`0x00AAF550` on the faction) and sets state 0 only when it **grew**; the step `0x00CD5B10`
    then builds one object from `0x00C7DB50(0x00701AC0())` and `0x00C720B0` and **finishes** (state
    2). What it builds is not decoded.
- **Behaviour classes (round 3, CONFIRMED).** The DB manager `0x00C90960` compares the junction
  row's behaviour name in a nested chain; the constructors (in the chain's reverse order) are:
  WAR_AND_PEACE `0x00C7E4A0` (0xFC bytes, vtable `0x0138BE70`), MERGE_UNITS `0x00D26BB0` (0x110),
  EXCESS_RECRUITMENT `0x00C7E900` (0x114, vtable `0x0138BDEC`), MISSIONARY `0x00C80B20`,
  GLOBAL_CONSTRUCTION `0x00C806F0` (0x114, vtable `0x0138BC3C`), FORT_MAINTAINANCE `0x00C7F6F0`,
  NAVY_STRENGTH_MANAGER `0x00C7C910`, CULL_EXCESS_FORCES `0x00C7FEF0`, RECAPTURE `0x00D26540`,
  RECALL `0x00C7FBD0`, RAID `0x00C7E5D0`, MILITARY_RANK `0x00BE1160`, TRADE_ROUTE_RAIDING
  `0x00BE1000`, TRADE `0x00BE0E50`, TRADE_AREA `0x00BE04A0`, TAXATION `0x00BDFF00`, RESEARCH
  `0x00D26900`, REGIONAL_DEVELOPMENT `0x00C7DE30`, EXPANSION `0x00D25340` (vtable `0x01391F28`),
  REGION_GROUP_DEFENCE `0x00C7CC30`, REGION_DEFENCE `0x00C7D3A0` (vtable `0x0138B66C`),
  REGION_COAST_DEFENCE `0x00C7F890`, PORT_BLOCKADE `0x00C7F5A0`, NAVY_REPAIR `0x00C7F230`,
  NAVY_RECRUITMENT `0x00C7EEA0`; the last few (NAVY_DISTRIBUTION / NAVY_ATTACK / MINIMUM_SAVING /
  HELP_ALLY) map to `0x00C7EAE0`, `0x00C7DFE0`, `0x00C7E370`, `0x00C74990(10.0)` in an order not
  yet pinned down (INFERRED). Every behaviour derives from `0x00C7C610`, gets an id string
  (`0x00C88B40(n)`, e.g. 0x177 WAR_AND_PEACE, 0xF1 REGION_DEFENCE, 0xCE EXPANSION, 0x162
  GLOBAL_CONSTRUCTION, 0x167 EXCESS_RECRUITMENT). Their vtables have 15 slots; **slot 12 (`+0x30`)
  is the per-turn step** (WAR_AND_PEACE `0x00CD6260`, REGION_DEFENCE `0x00CD4F90`, EXPANSION
  `0x00D65650`, GLOBAL_CONSTRUCTION `0x00CD8D70` (4.7 KB), EXCESS_RECRUITMENT `0x00CD6C10`), slot 9
  (`+0x24`) builds its target list (EXPANSION `0x00D90A60`, REGION_DEFENCE `0x00D0E2F0`).
  The behaviours are thin: they create BDI **desires** and set their priorities with
  `0x00CB2560(desire, 0, priority, flag)`; the work is in the desires, goals and analysers.
  - EXPANSION (CONFIRMED structure): slot 9 asks an analyser (`0x00A88780`) for candidate regions,
    each with an integer value (`0x00A63FD0`), and keeps `(region, value)` plus the total (1 when 0);
    slot 12 makes one desire (0x124 bytes, vtable `0x01378800`) per region not yet desired and sets
    each desire's priority to `value / total × (1 − k) × N + k`, `N` = number of desires, `k` = a
    personality value read through virtual `+0x28C` (which tunable is UNKNOWN).
  - WAR_AND_PEACE (CONFIRMED structure): for every other faction that passes three tests
    (`0x00C3F4B0`, not `0x00C3F620`, `0x00C3FD60`), takes the largest of a per-faction count list
    (`0x00C5C4D0`); when it is above 3 it creates a desire with priority 1.0 (meaning of the count
    UNKNOWN).
- **Personality object (round 4, CONFIRMED).** `0x00C930C0` (vtable `0x0138AB30`) holds a pointer
  to the personality record + 0x10 (`+4`); the tunable chain `0x00DE31E0` stores each name at a
  fixed offset of that block (HLP_DISTANCE_MULTIPLIER_FOR_NEUTRAL at 0, `..._FOR_ENEMY` 4, …, 238
  names). Every vtable slot is a getter of one offset (a few cast to int), so a personality
  virtual names its tunable. The personality is reached as `behaviour→+0x1C()→+0x88→+0x174`
  (`0x00B2B7B0`, `0x0047B710`). Slots used so far: `+0x218..+0x240`
  COMPOSITE_VALUE_ANALYSER_REGION_GROUP_{LOST, REDUCED, SPLIT, NEW, INCREASED, MERGED}_MULTIPLIER,
  REGION_{LOSS_CERTAIN, LOSS_VERY_LIKELY, LOSS_LIKELY, INVADED, CAN_WIN}_MULTIPLIER;
  `+0x288` BASIC_DESIRES_REGION_DEVELOPMENT_GOAL_BASE_COMPONENT_PROPORTION; **`+0x28C`
  BASIC_DESIRES_DEFEND_REGION_GROUPS_GOAL_BASE_COMPONENT_PROPORTION** (the EXPANSION `k`);
  `+0x290` BASIC_DESIRES_DEFEND_REGION_GOAL_BASE_COMPONENT_PROPORTION; `+0x294`
  PRIORITY_RANDOMIZATION_DESIRE; `+0x298` PRIORITY_RANDOMIZATION_INTENTION. (Rebuild the full
  slot→name table with `lst:0x00DE31E0:0x00DE52F3` and `vtat:0x0138AB30:170`.)
- **Desire priorities (round 4, CONFIRMED).** `0x00CB2560(desire, add, mult, slot)` adds a link
  `{source behaviour, add, mult}` to the desire (or updates an existing one, `0x00D09C40`); a
  link's value is `behaviour priority (source virtual +0x18) × mult + add` (`0x00CEFC50`); the
  desire's total is its base `+0x1C` plus all links, and its final priority `+0xEC` (slot 6
  getter) is `(1 + r) × total` with `r` = `+0xE8` (`0x00D0CB60`; writer UNKNOWN, INFERRED to be
  the PRIORITY_RANDOMIZATION_DESIRE jitter). So each behaviour's DB priority scales its desires.
  - EXPANSION (`0x00D65650`): `mult = value / total × (1 − k) × N + k` (verified in the listing at
    `0x00D658DC..0x00D6596A`), `k` = BASIC_DESIRES_DEFEND_REGION_GROUPS_GOAL_BASE_COMPONENT_PROPORTION
    = **25** in `default`, so `mult = 25 − 24 × value / mean`: the most valuable targets get the
    lowest (even negative) priorities. INFERRED: a data quirk (the name says proportion, 0..1);
    how negative priorities are handled downstream is UNKNOWN. Total = integer sum (1 when 0).
    Candidates (`0x00D90A60`): the target analyser `0x00A88780` lists objects whose `+0x138` is
    our faction and not flagged `+0xF8`; each gets value `+0xFC` of analyser 0x57 (vtable
    `0x01378664`, `0x00AB9280`) = the sum over a region list (`0x00A88A60`, INFERRED the region
    group) of analyser 0x56 values.
  - Region value analyser 0x56 (vtable `0x01378630`, update `0x00AB8BC0`), **decoded round 5
    (CONFIRMED)**. Base `v` = `+0xFC` of belief **0x4D** (`0x00A75560(region)`, ctor `0x00A40DF0`,
    vtable `0x013780BC`, update slot 9 `0x00ABD5E0`): with `S` = region `+0x1A0` (INFERRED the
    settlement), `v = 15000 + 25 × (floor(0.12 × max(0, S+0xE8 − S+0xBC)) + trunc(0.2 × (S+0xCC +
    S+0xBC)))` (the three fields' meanings UNKNOWN; the 25× and 15000 suggest money-scale values).
    The same update counts the region's slots by kind (`0x00A6FE10`: 2, 3, 4, 5, 6, other) into
    `+0x104..+0x128` and sums their beliefs' values into `+0x12C` (not part of `v`).
    (`0x00D5E350` is a different belief, 0x7A, created on the way; not read for the value.)
    Then, with `G` = the region-group belief 0x52 (`0x00A75460`, ctor `0x00A321C0`) when the
    region has one (`0x008CF5F0`) and `G`'s virtual `+8(faction)` holds (**own branch**):
    `s` = `G`'s change state for the faction (`0x00A634D0`: list `+0x124/+0x128` of
    `(faction, state)`, **5 when absent**): 0 REDUCED / 1 SPLIT → `v × (1 + ((m−1) − (m−1)^(n+1)) /
    (2 − m))` (`0x01285310` = CRT `powf`; a geometric series), `n` = regions of `G`'s list passing
    `0x00898B20` (`0x00A2C170`); 2 LOST → `× m_lost`. Loss likelihood from the region's counts
    `L = +0x104→+0x44`, `A = +0x40`, `B = +0x48`: `L == 4`: `A == 0 && B == 0` → LOSS_CERTAIN, `A ==
    0 && B ≥ 1` → VERY_LIKELY, else (`A < 2 && B > 0`) → LIKELY; `L == 3`: `A == 0 && B ≥ 1` →
    VERY_LIKELY, else the LIKELY test. Capital (`0x00A8B5A0`: faction `+0x72C` is this region,
    INFERRED) → ×5. **Other branch**: 3 INCREASED / 4 MERGED → the same series with that
    multiplier but the divisor `2 − m_reduced` (as the exe reads it); 5 NEW → `× m_new`; then `L <
    2` → CAN_WIN. Both: faction `+0x724` and `0x009749B0(S)` → ×3 (UNKNOWN); the settlement's list
    `0x0047B490()+8` and that of some region of the faction's list `+0x778` not empty → +5000
    (UNKNOWN). Default multipliers: LOST 1.0, REDUCED 1.3, SPLIT 1.4, NEW 1.0, INCREASED 1.3,
    MERGED 1.4, LOSS_CERTAIN 0.5, VERY_LIKELY 0.8, LIKELY 0.9, CAN_WIN 1.1 (INVADED 1.1 and
    OUTNUMBERS 1.1 are not read here). Ported: `campaign::region_value` (`base`, `composite`);
    PROVISIONAL inputs (see `FactionTurn::region_value`).
    **The original's own base values are stored in the startpos / saves** (round 5, CONFIRMED):
    each belief of `CAI_INTERFACE/CAI_BDI_POOL/CAI_BDI_POOL_BELIEFS[]` starts with a
    `CAI_BDI_COMPONENT_PROPERTY_SET` whose fields #1 / #4 are the belief type (77 = 0x4D here);
    the region base value beliefs end with `{CAI_BASE_VALUE #0 i32 value}` and
    `{CAI_REGION_BASE_VALUE #0 u32 region component id, #1 i32[11] the slot counts +0x104..}`;
    `CAI_WORLD/CAI_WORLD_REGIONS[]` items give the component id (item value #2) and the key
    (`CAI_REGION #10`). eur_napoleon: 72 values, all `15000 + 25 k` (e.g. france 50450, england
    49800, austria 39250, spain 38900, gibraltar 16900), which confirms the formula's form. The
    settlement records hold no money fields, so `+0xBC/+0xCC/+0xE8` are runtime values (UNKNOWN).
    Ported: `keys::read_region_base_values`, passed as `TurnContext::region_base_values`
    (`driver::install_with`; the app reads it with the AI keys); `region_value` uses the stored
    value when present.
  - REGION_DEFENCE (`0x00CD4F90`): candidates sorted by value descending (insertion, ties keep
    order), multipliers `1, 0.5, 0.25, …`. Candidate list `0x00D0E2F0`: none at all when
    `0x00C288F0` (human faction: difficulty > 0; AI faction: difficulty < 0) or when no entry of
    the faction's relation list `+0x14C` has `+0x168 == 0`; else each region of `+0x104` with
    `0x00C53FB0` (`+0x190→+0xC == 1`) and `0x00C43C70 > 0` gets the analyser 0x56 value.
  - WAR_AND_PEACE (`0x00CD6260`): for each entry of the faction's list `+0x14C` with `+0x168` set,
    `+0x160 != 0` and `0x00C3FD60`, the largest `(B − A) / max(1, B / 6)` over its `+0x164` items
    (`A`, `B` = `item→+0x100→+0x4C/+0x50`); above 3 → one desire, mult 1.0.
  Ported: `ntw_ai::campaign::desires` (multipliers, link) and `FactionTurn::army_desires`
  (REGION_DEFENCE + EXPANSION desires served in priority order). PROVISIONAL there: the region
  value (our snapshot value × 100), the candidate filters, the jitter draw, and serving desires
  directly instead of through goals/intentions.
- **CAI RNG (round 4, CONFIRMED).** `0x00A9DA10(lo, hi)` (float) and `0x00A9D9C0(lo, hi)` (int)
  step the LCG `x = x × 0x343FD + 0x269EC3` held at `+0x71C` of their object and use `x >> 16`
  (float: `× 1.5259022e-05 × (hi − lo) + lo`; int: `(span + 1) × (x >> 16) / 0xFFFF`, capped) —
  the same LCG and draws as `ntw_sim::rng::CaRng`. Which object holds that state (the campaign
  model's RNG or a CAI-own one) is UNKNOWN.
- **EXCESS_RECRUITMENT = new-unit recruitment (round 6, CONFIRMED; round 4 had the structure, filed under MERGE_UNITS by the shifted table).** The
  behaviour (ctor `0x00D26BB0`, id 0x16D, vtable `0x01378884`, step `0x00D68380`) creates
  **`CAI_BDI_UNIT_RECRUITMENT_NEW` intentions** (`0x00C8C1F0`, 0x154 bytes, vtable `0x0138A1B0`,
  type 0x100; record name from its save `0x00CA7230`), added to the **intention** list
  (`0x00CB2C10`) and linked with mult 1.0, kept in the behaviour's list `+0x104..`. The entries
  (a faction map walked with `0x00A676E0/F0`) are regions: per entry the belief
  **`CAI_UNIT_AVAILABILITY_ANALYSIS_REGION`** (`0x00D5F720` → ctor `0x00D3D530`, type 0x96, vtable
  `0x0138F8D8`; options at `+0xFC→+0x2C`, count `+0x28`, 16 bytes each with costs `+4` (unit budget)
  and `+8` (money)) and **`CAI_REGION_DEFENCE_STRENGTH_ANALYSIS`** (`0x00A748A0` → ctor
  `0x00A41010`, type 0x62, vtable `0x0137A954`; `x` = its int via `0x0047B090`); entries without
  options are skipped. Weights: `w = x²`, `w ← 1 − w × (1/Σw)` (with `Σw = 0` this is NaN: the walk
  then stops at the first entry), normalised by their sum (`1/n` each when the sum is 0). Budgets:
  units = the pool analyser's virtual `+0x60` (UNKNOWN), money = `0x00A60F00()` (INFERRED treasury)
  minus `0x00BBE910()` and `0x00AAF5D0()` (UNKNOWN committed amounts). Up to `2n` iterations while
  both budgets are > 0: `u = CAI float(0, 1)`, walk the weights (`u −= w` while `u > 0`; past the
  end: nothing this iteration), option = `CAI int(0, count − 1)` (`0x00A9D9C0`), subtract its two
  costs, create the intention for (region, option). So regions with **little defence strength are
  likelier** to recruit. The army-merge missions (`CAI_BDIM_MERGE_UNITS`, `CAI_BDIM_MULTI_MERGE_AT`)
  come from elsewhere (UNKNOWN). Ported: `FactionTurn::recruitment_draws` / `recruit_unit`.
- **RESEARCH_TECHNOLOGY — CONFIRMED structure (round 7, worker ai2).** The row is in every
  shipped manager that has it: `nap_eur_full`, `nap_eur_france`, `nap_eur_britain`,
  `nap_eur_maintainance`, `nap_spa_full`, `nap_spa_maintainance`, `nap_egy_full`, all at priority
  **500** beside `NAVY_STRENGTH_MANAGER` 1750 (read from the shipped table with `ai_tables_probe
  campaign_ai_manager_behaviour_junctions`); `nap_ita_full` / `nap_ita_maintainance` have neither.
  So in eur_napoleon **every AI faction researches** — before round 7 our turn never did.
  - **Behaviour** (ctor `0x00BDFF00`, id 0x10F, vtable `0x01382578`). Its **step `0x00C2AD60`**
    (3661 bytes) does, in order: (a) walk our own map (`0x00A676E0/0x00A676F0`), and for each entry
    its sub-list (`0x00C1BC40/0x00C1BC50`); every item that passes `0x00C40310` (which calls
    `0x00A63FD0`, the integer value getter, then `0x00AAF1F0`) is collected into a list sorted by
    `0x00C288D0` (→ `0x00C288B0`) **descending** (an insertion sort that inserts only when
    `key > key_at`, so equal keys keep their order), skipping the key `-1`; (b) the same for every
    entry of the relations list `0x00C50B50/0x00C50B80` whose `+0x160` is 0 (`0x00C3F620`) and whose
    `+0x188` is 0 (`0x00C3F4E0`), sorted by `0x00C4CCB0(8)` — that is **character attribute 8**
    (`0x009C7610(8)`, the `character+0x394` attribute array) — again descending, skipping `-NAN`;
    (c) for each entry of list (a): if `0x00CF0D30(item+0x34)` (its dirty / already-desired flag) is
    set the item is **removed from the behaviour's own list** (`+0x110`/`+0x114`) and skipped;
    otherwise an existing goal of this class for that item (`0x00CF6F10` key match in the
    behaviour's `+0x40`/`+0x44`, state `0x00CB4480() != 2`) is reused, else a new 0x124-byte goal
    (`0x00B87FB0` + `0x00BF8810`, vtable **`0x013825B4`**, type 0x110) is added to the desire list
    `0x00CB2980`; (d) the behaviour's old goal list is dropped (`0x00CEFB00` + `0x00D04710`) and
    replaced by the fresh one; (e) **the link multipliers**: for each goal in list order
    `0x00CB2560(goal, 0, m, 0)` with `m` starting at `0x3F800000` and multiplied by **0.95** after
    every entry — CONFIRMED, so the goals are `1.0, 0.95, 0.9025, …`. The tail of the function (the
    141 lines past the dump) still walks list (b).
  - **The candidates are gentlemen (CONFIRMED).** The goal's slot 3 `0x00C44BD0` dirties
    `behaviour+0x1EC`, our faction and `goal+0x10C + 0x34`, so `goal+0x10C` is a **character**; slot
    10 `0x00C54DB0` calls `character->virtual+0xC()` and dirties its `+8`. INFERRED from
    `0x00C40310` / `0x00C4CCB0(8)` that a candidate is a gentleman who can research
    (`0x00AAF1F0`: `*(character+0x1E0)+8 != 0` — his own technology list — and
    `0x00E1F130(0x27) > 0`, some positive effect).
  - **Goal class `CAI_BDI_GOAL_RESEARCH_TECHNOLOGY`** (vtable `0x013825B4`): slot 9 (refresh)
    `0x00C5F2C0` → `0x00C3FF10` = `0x00A63FD0()` then **`0x00AA4BF0()`** (the first entry of the
    analyser's list, via `0x006649C0`; non-zero = there is a technology); when it is 0 the goal is
    left finished and nothing happens. Slot 11 `0x00CE56A0` wraps the analyser's value.
  - **Deliberation `0x00C2E350` (slot 12, CONFIRMED):** virtual `+0x1C`, then
    `0x00A74B50(goal+0x10C)` — reserve a school: when the faction has none registered
    (`0x00A87130`) it makes belief type **`0x69`** (`0x00A3B080`, vtable `PTR_FUN_0137A4CC`,
    `+0xFC` = the gentleman) and registers it with `0x00A614A0`; when the gentleman already has one
    (`0x00A79DC0`, a hash lookup on `key ^ 0x4A545EED`) the **old school is replaced**
    (`*(entry+8)+0xC = new`) and the faction's school list `+0x124/+0x128` is kept duplicate-free
    (`0x00CB2770`); the return value is the reserved school. **If it is 0, no intention is made.**
    Otherwise one intention (id **0xE5**, vtable `0x013828A0`, 0x114 bytes) with
    `(0, goal+0x108, goal+0x10C, school)` — technology, gentleman, school — is found on the pool's
    intention list (`+0x54`/`+0x58`, `0x00CF6F10` key) or created and added with `0x00CB2C10`, then
    linked **`0x00CB2560(intention, 0, 1.0, 0)`**. Then state 1.
  - **Intention action (slot 12 `0x00CE4720`, CONFIRMED):** `0x00AA4B80(+0x108, +0x10C, +0x110)`:
    our faction, then `0x00C1BF00(school)`, and when `0x00A8B2D0(...)` is false
    **`0x008EEC90(technology, school)`** — the research start (`ntw_sim::campaign::research`,
    `start_research`) — true on success; on failure the intention is dropped (`0x00D0A1D0`), on
    success removed from the list (`0x00D095C0`). Slot 1 `0x00CFF6C0` and slot 18 `0x00CFF810`
    release the technology (0x34 bytes), the gentleman and the school references.
  - **UNKNOWN / PROVISIONAL (ported as stand-ins).** The candidate sort key `0x00C288B0` is the
    pointer chase `*(this+0xFC) → +0x1E0 → +8 → +0x10` (+0x10) and is not decoded, so **our
    candidates are the faction's available technologies in research-cost order** (cheapest first)
    instead of one per gentleman; the analyser behind the refresh (`0x006649C0`) is not read; the
    school is the free school with the best rate for the technology's thread rather than the one in
    the chosen gentleman's settlement; no researcher-attribute bonus, no technology stealing, no
    `research_rate_mod_army_tech`.
  - **Ported (round 7):** `ntw_ai::campaign::research::goal_multipliers` (the CONFIRMED 0.95 decay)
    with its unit test; `FactionTurn::research_goals` / `available_technologies` / `start_research` /
    `best_free_school`; `Node::ResearchTechnology` → `Action::Research` → `AiOrder::StartResearch` →
    `CampaignCommand::StartResearch`; the snapshot grew `AiRegion::schools` (`AiSchool`: slot, slot
    id, the three thread rates of `ResearchParts::rate`, the technology in work), `AiFaction::
    technologies` (the `FACTION_TECHNOLOGY_MANAGER` states) and `AiWorld::technologies` (cost +
    thread). Test `ai_researches_at_its_schools` (eur_napoleon, 8 End Turns: 23 starts, every one a
    valid model command, no school twice, 11 technologies under research at the end).
- **RESEARCH_TECHNOLOGY (round 4 notes, superseded by the block above).** Step `0x00C2AD60` builds two sorted
  lists: own items of the faction list `+0x114` (each item's sub-list, sorted by `0x00C288D0`, −1
  skipped) and, for every other faction entry of `+0x14C` that has `+0x160 == 0` and not
  `0x00C3F4E0`, its items sorted by `0x00C4CCB0(8)`; then makes **`CAI_BDI_GOAL_RESEARCH_TECHNOLOGY`**
  goals (vtable `0x013825B4`, type 0x110, record name from its save `0x00BFEA10`) through
  `0x00CB2980` / `0x00BF8810`; a goal's deliberation (`0x00C2E350`) makes one intention (0x114
  bytes, vtable `0x013828A0`, type 0xE5, ctor `0x00C8B690`) linked with mult 1.0. Not ported (the
  model has no research yet).
- **TAXATION (round 6, CONFIRMED structure).** Ctor `0x00BE04A0` (id 0x139, vtable `0x01383710`).
  Refresh `0x00C5EF60`: state 0 once its two intention lists (`+0x10C`, `+0x120`) are empty. Step
  `0x00C2BBB0`: drops its old intentions, then for each item of the faction's list `+0x1A8`
  (INFERRED tax domains): (a) for each region of that item's analysis (belief `0x00A74CF0`, list
  `+0x11C`) whose exemption flag (`+0xE4`) differs from "is in the analysis' list `+0x114`"
  (`0x00A8B880`), one **`CAI_BDI_TAX_EXEMPT_REGION`** intention (id 0xE4, vtable `0x01383790`,
  ctor `0x00C8C040(region, exempt)`); (b) for class 0 and class 1 (`0x00C56250(class)`, the
  current level), when the level is not **2**, one **`CAI_BDI_SET_TAX_LEVEL`** intention (id 0xE3,
  vtable `0x0138374C`, ctor `0x00C8BB00(item, class, 2)`); all on the intention list, mult 1.0.
  So **the AI always sets tax level 2**. INFERRED: level 2 = the third of the five
  `taxes_levels` by rate ("normal"). The exemption analysis is not decoded. Ported:
  `FactionTurn::taxation` (level 2 for both classes; no exemptions).
- **Startpos BDI state.** The startpos `CAI_INTERFACE` stores the BDI pools with record names such
  as `CAI_BDI_DESIRE_DEFEND_REGIONS(_INFO)`, `CAI_BDI_DESIRE_DEFEND_REGION_GROUPS(_INFO)`,
  `CAI_BDI_GOAL_REGION_DEFENCE_*`, `CAI_BDI_WAR_AND_PEACE_MANAGER`, `CAI_BDI_UNIT_RECRUITMENT_NEW`
  (`esf_dump schema`); reading them would give the original's own desires at turn 1 (not done).
- **Difficulty (round 3, CONFIRMED lookup).** `0x008DD090` (per faction, at campaign start) reads
  the faction's 16-byte difficulty block `+0x6E4..` (copied from the player-slot record `+0xAC`) and
  its flag `+0x6E0` (from the slot `+0xA8`), and applies `campaign_difficulty_handicap_effects` rows
  through `0x00F9F970`: difficulty clamped to −2..2, the bool column selects one of two lists, the
  faction's `+0x6E0` picks which. `+0x6E0` is tested all over the faction code and partitions
  factions in `0x0095A810`: INFERRED **is human**. The preference help strings say "Positive gives
  advantage. -2 is vhard, -1 is hard, 0 normal, 1 easy" (campaign, battle and autoresolve each).
  Read that way the rows make sense: the human uses `(d, true)` (penalties on hard, bonuses on
  easy) and the AI rows `(·, false)` are bonuses that grow from −1 to 2, which fits AI factions
  using the **negated** player difficulty (INFERRED; the AI factions' `+0x6E4` writer is not found:
  `0x0096A250` sets the block per faction by key). Battle difficulty: INFERRED to feed the per-army
  attack bonus level (`0x006AD6E0`, +4/+8, W1 §12.9); not traced. Autoresolve:
  `campaign_variables` `autoresolve_{easy_difficulty_human_advantage 0.1, hard_difficulty_AI_advantage
  0.08, very_hard_difficulty_AI_advantage 0.2, easy_campaign_AI_percent_reduction 0.05,
  hard_campaign_AI_percent_increase 0.2, very_hard_campaign_AI_percent_increase 0.35}` (CONFIRMED
  values; how they apply UNKNOWN).
  **Round 4 (who writes the AI factions' difficulty, CONFIRMED):** `0x008DD090` has two paths.
  A faction **without a player slot** (`0x008CEEF0`: faction `+0x514 == 0`, i.e. every AI
  faction) takes `+0x6E4 / +0x6E8` from the campaign setup object's `+0x3C / +0x40` (passed down
  from `0x00872550` → `0x00915E20` → `0x0095A810`) and applies the handicap list with the flag
  forced to **0** (`0x00F9F970(+0x6E4, 0)` = the AI rows). A faction **with** a player slot copies
  its flags `+0x6E0..+0x6E2` and the 16-byte block from the slot (`+0xA8..`, `+0xAC..`) and uses
  its own flag. The only other writer of the block is the command handler `0x00937C40` →
  `0x008EE7D0` → `0x0096A250` / `0x008EE800` (deserialises a faction key and the block; skipped
  when the command's `+4` flag is set), INFERRED the front end's / multiplayer "set difficulty".
  Block layout INFERRED: `+0x6E4` campaign difficulty, `+0x6E8` battle difficulty (the setup's
  `+0x3C/+0x40` pair). Which value the setup holds for the AI (the player's own value or its
  negation) is still UNKNOWN; the data only makes sense for the AI with the negation (no
  `(−2, false)` rows exist; `(2, false)` has the biggest bonuses). REGION_DEFENCE also reads the
  difficulty (`0x00C288F0`: human → `d > 0`, AI → `d < 0`): when it holds, the faction makes no
  REGION_DEFENCE desires at all (§4 "Desire priorities").
  Battle difficulty: not traced further (W1 §12.9's per-army attack bonus level `0x006AD6E0`
  remains the INFERRED consumer).
  **Round 5.** (a) The battle consumer `0x006AD6E0` (CONFIRMED): for a unit, its army (land
  `unit+0x1F0→+0x1EC`, ship `+0x1F8→+0x43C`) gives a flag `+0x224` and a level `+0x234` (−1 =
  none); with the flag clear and a level set, the level is 0 for the first 6 battle ticks, and when
  the unit's alliance has more units (land + ships, over every army) than the others, it drops by
  `(ours − theirs) / 5`, clamped to 0..2. W1 §12.9: flag clear → +4 attack at level 1, +8 at 2;
  flag set → +4 at level 1 only. INFERRED: `+0x224` = human army, `+0x234` = the AI's battle
  difficulty bonus level, **taken away as the AI outnumbers the enemy** (one level per 5 extra
  units). The writer of `+0x234` was not found (`0x0057FFE0` / `0x005A4440` write a different
  class's `+0x234`). (b) The front end stores the battle difficulty preference as
  **`1 − index`** of its own difficulty setting (`0x009E1DD0`: `battle_difficulty = 1 −
  setup+0x2C→+4`), so the UI index is 0 easy, 1 normal, 2 hard, 3 very hard (CONFIRMED mapping).
  (c) The preferences are registered by index in a table `0x014A06D8` (`campaign_difficulty` 0x22,
  `battle_difficulty` 0x20) and read by name; no direct reader of the campaign setup's
  `+0x3C/+0x40` was reached, so **whether the AI factions get the player's value or its negation
  stays UNKNOWN** (the handicap rows still only make sense with the negation).
  Not ported: the attack bonus is a combat rule of the battle model (§8).
  **Round 6.** The 16-byte block is stored per faction in saves and startpos:
  `FACTION/CAMPAIGN_PLAYER_SETUP` v3 = `[{CAMPAIGN_VICTORY_CONDITIONS},
  {CAMPAIGN_PLAYER_SETUP_INGAME_MODIFIABLES v2: i32, i32, i32, bool}, utf16 faction key, bool,
  bool, bool]`; the first bool after the key is true only for the human's faction in the user's
  saves (INFERRED `+0x6E0`). Every faction carries its own block, so a save of a campaign started
  on hard / very hard would settle the "player's value or its negation" question at once; all
  user saves and the startpos hold 0 (normal) everywhere. Reader: `keys::read_difficulties`
  (not used by the turn yet: on a new campaign the original overwrites the AI blocks from the
  setup at start, whose value is the open question). **User check:** a save from a hard campaign.
- **BDI pool processing (round 6, worker ai5, CONFIRMED unless tagged).** This answers "how desire
  priorities are consumed".
  - **Components.** Every behaviour, desire, goal and intention is a BDI component (base ctor
    `0x00C7BC90`): `+0x1C` base priority (the behaviour's DB priority; 0 for most goals), `+0x20`
    total, `+0xE8` jitter `r`, `+0xEC` final priority (getter slot 6 `0x00CEFC40`), `+0xF0` state
    (**starts at 1**), `+0xF8` dirty flag (**starts set**), `+0xF9` kept-when-dead flag. Lists per
    link slot `k` (0 or 1, 0x14 bytes each): ancestors at `+0x24 + 0x14k` (count `+0x30`), descendants
    at `+0x4C + 0x14k` (count `+0x58`), both transitive; outgoing links `+0xA8/+0xAC`, incoming
    `+0x94/+0x98`.
  - **Links** (`0x00CB2560(target, add, mult, slot)` on the source; link object `0x00C87B90`
    `{vtable, target, source, add, mult, slot}`, which is exactly the startpos record
    `CAI_BDI_COMPONENT_BLOCK_OWNS {owner, owned, f32, f32, u32}`): a new link adds the source and
    its ancestors to the target's (and the target's descendants') ancestor list and the target and
    its descendants to the source's (and its ancestors') descendant list; an existing link is
    updated in place (`0x00D09C40`). Value `0x00CEFC50` = `source final × mult + add`. Total
    `0x00D0CB60` = base + Σ link values (in link order); when it changes, `final = (1 + r) ×
    total` and every target of this component's links is recomputed (`0x00D0C760`). A removed link
    (`0x00D047C0`) recomputes too.
  - **Jitter** (`0x00CB5CF0` / `0x00CB6AD0`): when a component joins the pool's desire list
    (`0x00CB2980`, behaviours, desires, goals) or intention list (`0x00CB2C10`), and again for every
    desire and intention by pool slot 11 (`0x00D03D50`; its caller is not traced, INFERRED the new
    turn), `r = CAI RNG float(−j, j)` (`0x00A9DA10`) with `j` = pool slot 12 `0x00CC94F0` =
    personality `+0x294` **PRIORITY_RANDOMIZATION_DESIRE** for desires and slot 13 `0x00CF0A70` =
    `+0x298` **PRIORITY_RANDOMIZATION_INTENTION** for intentions, **used raw** (the getters return
    the stored float, the tunable chain `0x00DE4B3B` stores it unscaled): with the shipped `default`
    10 and 0, **a desire's priority is multiplied by `1 + U(−10, 10)`** (it can turn negative);
    intentions are not jittered. `j = 0` while the pool's flag `+0x80` is set: the pool ctor
    `0x00C8E290` sets it, the pool loader `0x00C8EFA0` (startpos / save) clears it at its end.
    The RNG object is `pool+0x84 → +0x84` (the CAI RNG of §4 "CAI RNG").
  - **Faction pool** (vtable `0x013890BC`, at manager `+8`; the DB manager `0x00C90960` adds one
    component per junction row, in row order, with `0x00CB2980`, and a 10.0 component `0x00C74990`
    with `0x00CB2C10`): lists beliefs `+0x2C/+0x30`, desires `+0x40/+0x44`, intentions
    `+0x54/+0x58`, failed `+0x6C/+0x70` (the save `0x00CA5490` writes them in that order).
  - **Run** (pool slot 10 `0x00D01910`): step counter `+0x74 = 0`; each step (slot 16
    `0x00CB42A0`) increments it and stops the run at **6000** steps or when the budget `+0x7C`
    (decremented once per run; writer UNKNOWN) is 0. Loop: (1) beliefs pass `0x00D019A0` (dead
    components, virtual `+0x28` false, are removed and deleted unless `+0xF9`); (2) desire pass
    `0x00D01B10`; when it did nothing, (3) intention pass (slot 14 `0x00D024D0`, same rules, and it
    also stops while the pool flags `+0x8C` / `+0x8D` are set); when that did nothing too, the run
    ends.
  - **Selection** (desire and intention passes, the same comparator): dead entries are removed;
    among the living, walking the list in order, a candidate `c` replaces the best so far `b` when:
    `c` is (state 1 and dirty) and `b` is not (state 1 and dirty) or `b` is a descendant of `c`
    (slot 0); else if both are (state 1 and dirty): `c` is not a descendant of `b` and `c`'s
    priority is strictly higher; otherwise (`c` not state 1 + dirty): never over a (state 1 +
    dirty) `b`; with different states the **lower state** wins; with equal states and different
    dirty flags the dirty one; with equal flags: `b` a descendant of `c` → `c`; `c` a descendant
    of `b` → `b`; else strictly higher priority (ties keep the earlier). So: refreshes first,
    **ancestors before descendants**, then state 0 (to deliberate) before 1 (done), then
    **priority**.
  - **Acting on the best** (state != 2): dirty → re-deliberate (`0x00CCCF70`: dirty children first,
    then virtual `+0x24` = slot 9 (refresh; e.g. EXPANSION's target list, `0x00D916B0` sets state
    0 unless 2), then the dirty flag clears) and the pass restarts; state 0 → virtual `+0x30` =
    **slot 12, the deliberation / action** (it creates and links children and sets state 1
    `0x00D0A0F0(1)`), one per pass. State 2 = finished / failed (`0x00D0A0F0(2)` drops its links
    and children).
  - **So priorities only order the work**: every behaviour, desire and goal is deliberated, the
    higher (jittered) priority first among those not related by ancestry, and all deliberation
    runs before any intention acts; intentions then act one by one in priority order (and take
    armies and money first). A negative EXPANSION multiplier just puts that goal last.
  - **EXPANSION_BEHAVIOUR's targets are its own region groups** (correction of round 4): its step
    `0x00D65650` creates **`CAI_BDI_GOAL_REGION_GROUP_DEFENCE`** goals (0x124 bytes, vtable
    `0x01378800`, type id 0xD8; record name from its save `0x00D448E0`), one per candidate, where
    the candidates (`0x00A88780`) are pool beliefs whose `+0x138` is the faction (INFERRED: the
    faction's region groups; value = Σ of their regions' 0x56 values). The goal (slot 3
    `0x00D7E2D0`) makes a request object (`0x00C93480`, 0x154 bytes, vtable `0x0138910C`, type 7)
    and its deliberation (slot 12 `0x00D6A2E0`) links that object to targets from three lists
    through the mission factory `0x00CEEB00` (a switch over 11+ mission kinds, not decoded) and,
    for each region `(region, n)` of the group with `n != 0`, finds or creates a
    **`CAI_BDI_GOAL_RECRUIT_STRENGTH_IN_REGION`** goal (0x124 bytes, vtable `0x0138E4E4`, type
    0xDA, ctor `0x00D27B70`) linked with **mult `0.5 + 0.5^(i+1)`** (1, 0.75, 0.625, …) in list
    order and given the count `max(n, request virtual +0x3C)`; `n == 0` removes it. INFERRED: a
    group is a connected block of the faction's regions (the composite analyser's LOST / SPLIT /
    MERGED states). Where invasions come from (the mission kinds of `0x00CEEB00`) is UNKNOWN.

### Deal evaluation (worker deal-ai, 2026-10-10)
How the AI recipient answers a proposed deal. Ported pieces: `ntw_sim::campaign::deal_value`
(tests `technology_value_follows_the_traced_formula`, `a_fair_trade_passes_and_a_lopsided_demand_fails`,
`inflation_is_clamped_and_truncated_when_applied`, and the round-3 tests below). `CampaignModel::ai_refuses_deal`
answers a deal of technologies as the exe (`ai_accepts_technologies`); a deal with regions keeps the
PLACEHOLDER rule (the AI gives no region or technology in it) until the region value is traced.

- **Entry** `0x00C49BE0` (`CCQ_DIPLOMACY_PROPOSE_DEAL` executor `0x00933690`; the AI's counter-offer
  `0x00CC58C0` calls it with flag 0): unless the recipient (`+0x1C`) is human and `0x00C1E240`
  turns a deal without demands into a payment, it stores the flag at negotiation `+0x2B8`, posts the
  UI message, and when the **recipient is not human** runs the AI's evaluator `0x00AA5ED0(negotiation,
  flag)` (`this` = the AI object). CONFIRMED.
- **Records** (negotiation `+0x2A4`, `0x00BF5A60`): 0 trade, 1 access, 2 access cancel, 3 alliance,
  4 **regions** (`+0x94`, vtable `0x01381DE0`), 5 **technology** (`+0xE0`, vtable `0x01381E34`),
  6 state gift, 7 payments, 8 protector, 9 peace (`+0x184`), 10 war, 11 join war, 12 break trade,
  13 break alliance. Record virtual `+0x2C` = has items, `+0x38` = value, `+0x3C` = apply,
  `+0x44` = propose. CONFIRMED.
- **Evaluator `0x00AA5ED0`** (CONFIRMED order): (1) builds two lists of the AI's diplomatic goals
  toward the proposer (`0x00CC13D0` + `0x00CCCCA0`, filtered by `0x00D0C460` and copied through
  `0x00CF8400`; `0x00CC0280` + `0x00CCB750`; traced below, "Goal lists" and "Weights"); (2) for each record 0..13 with items:
  `0x00A62400` adds the record's goal to the evaluation (below); for record 7 the payment amount
  (slot 2 of its value) is kept; if the recipient's `diplomacy_options` toward the proposer for that
  record index (relationship `+0x528` → `0x00B64C50` → `0x00B27FE0(index)`) is 1 or 3 (accept
  forbidden; the index equals the option id, map `0x01459080`) → **decline**; (3) `0x00A75CD0` →
  `0x00465180` always returns 0; (4) **accept** (`AcceptCampaignNegotiationDeal` `0x00C114B0`) when
  `fair` (`0x00CE5750`) and the AI strategy's budget (`0x00A87F50` object → `0x00CBBA80` virtual `+0x54` = the diplomatic pot, below "Budget") ≥ the
  payment, or when `good` (`0x00CEFA40`) and the payment ≤ the **recipient's**
  treasury (`0x004631B0` = `+0x1C`, economy `+0xAC` → `0x00BCAFE0`); (5) else, with flag 0 and
  `0x00D0C560` false, when fewer than 10 counter-offers were made (`0x00C1EC20`) the counter-offer
  builder `0x00CC58C0` may re-propose (returns true: no decline); (6) else **decline**
  (`DeclineCampaignNegotiationDeal` `0x00C1F210`).
- **Per record `0x00A62400`**: records 2, 6, 10 add nothing; 0, 3, 8, 9 add the matching existing goal
  (`0x00CB21C0(goal, 1.0)`); 1, 4, 5, 11, 12, 13 build a goal (`0x00C8E210`: type, record copy,
  weight, …; goal `+0xBC` weight, `+0xC8` value triple) whose **weight = Σ weights of the AI's goals
  of the same type whose item matches an item of the deal** (first list against one side's items, the
  second against the other's), value = the record's virtual `+0x38`, then `0x00CB21C0(goal, 1.0)`;
  7 builds one with weight 0.
- **Evaluation `0x00CB21C0`** (struct: triple `+0`, bonus `+0x0C`, eagerness `+0x10`, count `+0x14`):
  triple += goal value; bonus += weight × type weight (`0x00D018B0`: 500 for 0/2/5/12/13, 250 for 1,
  1000 for 3/8/11, 2500 for 4/9, else 1); eagerness += (w ≥ 0 ? gain × (w + p − 1) : given × (w + 1
  − p)) × p; count += 1. `fair` (`0x00CE5750`): given ≤ 1.05 × (bonus + gain) and cost ≤ 1.05 ×
  (bonus + gain) (f32, `0x0137C258`); `good` (`0x00CEFA40`): 2 × given and 2 × cost ≤ bonus + gain.
  Triple slots are read as unsigned. CONFIRMED (disassembly).
- **Technology value** (record virtual `+0x38` = `0x00C13360`): Σ offered (flag 1) then demanded
  (flag 0) of `0x00A36B20`, × trunc(inflation) (`0x00C42FB0`: the float is CVTTSS2SI'd before it
  multiplies). `0x00A36B20`: 500 + trunc(10 × PointsRequired^1.1) (`powf` `0x01285310`, constants 1.1
  / −10.0; **the old note "500 + 10 × cost" missed the power**), ×2 when exactly one faction (list
  campaign `+0x110`, entries with `+0x194` set) has it researched (state 0, `0x008F3DB0`), ÷ (traded
  + 1)² (proposer's entry `+0x28`, `0x008F4990`) unless demanded by a human proposer (`0x00C3F870`
  = wrapper `+0x168` → faction `+0x6E0`); offered → (v, 0, 0), demanded → (0, v, v). CONFIRMED.
- **Inflation** (campaign `+0x1014`, `0x008A9920` at each round end after the calendar moves on):
  `clamp(net / max(1, first), 1, 3)`, net = Σ factions of last turn's income (categories 5..11,
  `0x00BBE970`) − expenses (18..24, `0x00BBE910`) (`0x0096D2E0`), read as unsigned; `first`
  (`+0x1010`) = the first round end's net, set once while 0; ctors write 1.0. CONFIRMED formula; kept by the model
  (`CampaignModel::deal_inflation`, saved as CAMPAIGN_MODEL #21/#22, below "Inflation in the save").
- **Region value** (record virtual `+0x38` = `0x00C131C0`): from the proposer's side (wrapper ==
  proposer, `0x00898B20`) `0x00A364B0(offered, demanded, proposer)`, × trunc(inflation), then
  `0x00C4D140`: slot 2 × 1.5^(max(m, 1) − 1), m = 0 unless the proposer is human and the peace record
  is empty, then m = demanded count + proposer faction `+0x938` (UNKNOWN counter). `0x00A364B0`:
  for each offered region, v = `0x00AA1E90(region, recipient)`; v goes to slot 2 when the region is
  `0x00C3E0C0` or the strategy's `+0x34` virtual `+0x94` holds, else to slot 0 when `0x00A79050`
  ≥ 0, or > −6 and the region is in the neighbour list (`+0x20`/`+0x24`) of one of a faction's regions (list `+0x77C`; which faction: not traced)
; for each demanded region: slot 1 += `0x00AA1E90(region, proposer…)` (unless the
  same tests), slot 2 += `0x00AA1E90(region, recipient)`. `0x00AA1E90` = belief 0x4D base
  (`0x00A75560`, ported `region_value::base`) with the group-change and personality factors
  (virtuals `+0x21C`, `+0x220`, `+0x22C`), ×2 tests, ×1.5 when allied regions are near. INFERRED
  reading of the branches; NOT ported.
- **Who scores the goals (round 2, CONFIRMED).** The AI object of the evaluator is the faction's
  manager (`0x00A87F50`); its BDI core is manager `+8` (`0x005D6450`), built by `0x00C8E290`:
  `+0x34` the component list, `+0x90` `0x00C904F0`, `+0x94` `0x00C8FB10`, `+0x98` the **finance
  component** `0x00C8FC20` (id 0xE0, vtable `0x0138AF40`), `+0x9C` WAR_AND_PEACE (`0x00BE6560`),
  `+0xA0` **DIPLOMACY_MANAGER** (`0x00C7FF80`, vtable `0x0138BEAC`; made from its junction row or by
  default, so every manager has one), `+0xA4..+0x164` the analysers, `+0xDC` the **diplomatic
  attitude object** (`0x00D2A270`, vtable `0x0138EBB0`; in `mp_eur_napoleon` France gets
  `0x00D30180` and Britain `0x00D3E990`, which share every slot used here). `0x00CCCCA0` (list 1)
  and `0x00CCB750` (list 2) call virtual `+0x38` / `+0x34` of every live component of `+0x34`
  except the DIPLOMACY_MANAGER, then the DIPLOMACY_MANAGER's. Every behaviour and the other core
  components have `0x00462B80` (`RET 0xC`, no-op) in both slots (all 30 behaviour vtables and the
  five components read), so the weights come only from the DIPLOMACY_MANAGER: `+0x38` =
  `0x00CCB810` (list 1), `+0x34` = `0x00CCB210` (list 2) (both made functions 2026-10-10).
- **Goal lists (CONFIRMED).** A goal (200 bytes, `0x00C8E210`): `+0` record index, `+4` a deal
  (`InitNegotiationDeal` layout: `+0x10`/`+0x20` regions demanded/offered, `+0x30`/`+0x40`
  technologies demanded/offered), `+0xBC` weight (clamped to [−1, 1] when the last argument is
  set), `+0xC4` / `+0xC5` flags; list 2 elements and the evaluator's copies of list 1 are 0xD4 bytes
  (`0x00C8E080`: goal + the value triple at `+0xC8`, the value computed on the mirrored deal by
  `0x00CF8400` → `0x00CC3E00`; the goal keeps its own deal). Each goal is built only when
  `0x00CCB150(AI, proposer, index)` passes: neither side's `diplomacy_options` for the index forbids
  it (AI→proposer not 2 or 3, proposer→AI not 1 or 3). All are made with weight 0.
  - **List 1** (`0x00CC13D0(proposer, out, core)`, what the AI would ask of the proposer): trade,
    alliance, access (by turns), protector, peace, join war, break trade/alliance as possible; **one
    region goal per region of the proposer's CAI region list** (CAI faction `+0x114`) that is not its
    owner's capital (`0x00A8B5A0`, the check `tradeable_regions` uses) and whose CAI region `+0x12C`
    is set with its `+0x34` object's virtual `+0x94` false (INFERRED the siege test of
    `0x00C5C040`), region in the goal deal's `+0x10`; **one technology goal per technology the
    proposer has researched (state 0) and the AI has in state 1, 2 or 3** (`0x008F4F10(this =
    proposer, out, AI)`; `0x008F3AC0` = tree entry state 1/2/3), technology in `+0x30`.
  - **List 2** (`0x00CC0280`, what the AI would give): the same with the sides swapped: **the AI's
    own regions** (same filters) in `+0x20`, **the technologies the AI has researched and the
    proposer has in state 1/2/3** in `+0x40`.
- **Weights (CONFIRMED, `0x00CCB810` / `0x00CCB210`, constants read).** Only goals of weight 0 are
  scored (all of them here: the "types already weighted ≥ 1" filter `0x00CE6430` and the
  {4,5,6,7}-only rule apply only when a goal already has weight ≥ 1, never from the evaluator).
  List 1: region **−1**; technology (one item) = attitude `+0x2A4(tech)` (`0x00D63230`), below −1 →
  −1, then **min(w, 0.5)**; trade = `+0x2A8(proposer)` unless `0x00A89580`, alliance = `+0x2AC`
  unless `0x00A63A40`, peace = `+0x2B0(proposer, 0)` (these three to [−1, 0.9]); access −1 (flag
  cleared); state gift and payments 0; the rest −1. List 2: region **−1**; technology (one item) =
  attitude `+0x2A0(tech)` (`0x00D63120`), below −1 → −1, then **min(w, −0.5)**; protector
  `+0x29C`, join war / break trade / break alliance `+0x2C4/+0x2C8/+0x2CC(proposer, third)` with
  caps −0.1 / −0.25 / −0.5 by the third party's relation; the rest as list 1. The region goals'
  `+0xC5` flag is `0x00CF1410` (not read by the evaluation).
  - `0x00D63120(tech)` (CONFIRMED): over the CAI world's faction list (`+0x120`, count `+0x11C`),
    `n` = entries with a technology manager (`+0x194`) that have the technology researched, `t` =
    all entries / 8 (at least 1); returns `2 × min(n, t) / t − 1` (floats).
  - `0x00D63230(tech)`: the faction's research-need belief (`0x00A74AD0`, class `0x00A3AFE0`, refresh
    `0x00ABB340`) array `+0x114` indexed by the technology's category (`0x00C5B950`: "admin" 0,
    "economy" 1, "military" 2, else 3; 3 categories, `0x004CDC50`). Refresh: five need scores, each a
    power of two `2^k`, `k` ≤ 8: `M` = 2^min(enemies, 8) (`0x00ABE100`), `D` = allies − enemies,
    1 when < 0, 256 when > 8, else 2^D (`0x00A98460`) — enemies and allies are the relation
    belief's (`0x00A74A20`, refresh `0x00ABAFB0`) lists `+0x40` (stance 0, war) and `+0x2C` (stance 2,
    allied) over the other factions; `E` (`0x00A9B810`, economy history of the faction
    economy `+4`: count `+0x3E8` (≤ 5), ring index `+0x3EC`, 100-byte turns; 16 when ≤ 2 turns; else
    `I`, `X` = income / expenses summed over the last count − 2 turns, `s` = trunc(2I × 0.125), `k`
    = 8 − the steps of `s` needed to reach 2I − X); `P1` (`0x00A64460`: (Σ region `+0x15C` >> 1) +
    (relation entries with `+0x168` and `+0x160 ≠ 0` >> 2)) and `P2` (`0x00A8D6E0`: (Σ over own
    regions of their `+0x140` list entries with `0x00C3F480` >> 1) + (entries with `+0x168 == 0`
    and `+0x160 ≠ 0` >> 2)), each 256 above 8. Then `need[0] = need[1] = D + E`, `need[2] = M +
    max(P1, P2)`, `h = (Σ need) >> 1` and `value[i] = min(need[i], h) / h`. CONFIRMED formula;
    the CAI fields `+0x15C`, `+0x140`/`0x00C3F480`, `+0x160`/`+0x168` of the relation list `+0x14C`
    are UNKNOWN, and the belief is cached (refresh schedule not traced).
- **Matching (CONFIRMED, `0x00A62400` disassembly).** Regions: weight = Σ weights of the list-1
  goals whose `+0x10` first region is a region the proposer **offers** (deal `+0x20`) + Σ of the
  list-2 goals whose `+0x20` first region is one it **demands** (deal `+0x10`); technologies the
  same with `+0x30`/`+0x40`. So a region either side gives counts −1 each when it is in the giver's
  goal list, a technology offered to the AI counts its need weight (0..0.5), one asked of it its
  spread weight (−1..−0.5). The record's value is its virtual `+0x38` (`0x00C60D60`), the goal is
  added with scale 1.0 and no clamp.
- **Budget (CONFIRMED `0x00AAF570`).** Finance component `+0x140` = the diplomatic pot. Its refresh
  (`0x00D10650`, when a compared belief changed): with treasury > 0, the four pots `+0x128`,
  `+0x130`, `+0x138`, `+0x140` = personality spending biases (`0x00CC2D50`: virtuals `+0x140`,
  `+0x138`, `+0x13C`, `+0x144`; ×0.6, and +0.4 on `+0x138`, when virtual `+0x218`) × treasury,
  capped for recently used pots (`+0x164..+0x17C` within 2 turns, limits `+0x168..+0x180`, the
  excess spread over the others); treasury ≤ 0 → all 0. Spending (`0x00CC53D0`, virtual `+0x40`)
  takes the cost from the pot of its kind (kind 8 from `+0x140`) and can leave it negative. With
  no payment in the deal the fair path needs `+0x140 ≥ 0`.
- **Inflation in the save (CONFIRMED).** `CAMPAIGN_MODEL` #21 u32 = `first` (`+0x1010`) and #22 f32 =
  the factor (`+0x1014`), just before the force caps #23/#24: the loader `0x00872550` reads both
  when the record version is above 4 (`0x00873B60`), the writer `0x008EBAB0` writes them as types 8
  and 0x0A (`0x008EC094`, `0x008EC0DE`). Evidence saves: every Europe save (ours and the
  original's) has #21 = 113816 and #22 = 1.0 (the startpos value; `first` is set only while 0);
  every original Spain save (`orig_fr_t1`, `orig_fr_may1811`, `auto_orig_spa_0245`) has #21 =
  4294948416 (−18880 read as unsigned) and #22 = 1.0. The nets are converted as unsigned (fix-up
  table `0x01318130`), so a negative world net makes the ratio ~4·10⁹ / first: the factor jumps to
  3 in a campaign whose `first` is positive (Europe) and stays ~1 for ever in one whose `first` is
  negative (Spain). **Kept 1:1** (not an ORIGINAL BUG by the rule): the net is produced by a signed
  subtraction (`0x0096D2E0` returns int), but every reader of it and of `first` (`0x008A9920`, the
  loader, the writer's ESF type u32) treats it as unsigned; nothing in the exe reads it signed.
- **Round-end update (CONFIRMED, round 3).** `0x008A9920`, after `UpdateCampaignCalendarToNextTurn`:
  `net` = `SumFactionsLastTurnNetIncome` (`0x0096D2E0`, `this` = the world at campaign `+0xF5C`): over
  the world's `+0x2C`/`+0x30` list (`FACTION_ARRAY`; the rebel faction is `+0x1C`, loaded from
  `REBEL_FACTION` at `0x0090C1E6`), each faction's last economics record (`0x00BABE00`) categories
  5..11 (`0x00BBE970`) minus 18..24 (`0x00BBE910`); `first` takes it while 0; factor =
  clamp(net / max(1, first), 1, 3). The economics history: a ring of 10 records of 25 ints
  (economy `+0x04`, count `+0x3EC`, index `+0x3F0`); `0x00BABE30` adds the turn's record at every
  round end, also when the faction cannot pay; the saver `0x00BD46E0` writes the newest `count`
  oldest first. Ported: `World::economy_history` (loaded, saved, replaces `last_income`),
  `CampaignModel::deal_inflation` (CAMPAIGN_MODEL #21/#22, updated in `TurnStep::RoundEnd`); tests
  `the_round_end_records_the_economy_history`, `inflation_takes_the_first_net_once`,
  `deal_inflation_and_economy_history_round_trip` (real saves). PROVISIONAL: our records hold
  categories 5, 7, 11, 19, 20 only (the model's income and upkeep).
- **Research-need inputs (round 3).** CAI faction `+0x14C` = `CAI_FACTION` #3, its mobiles; a mobile's
  `+0x168` = `CAI_RESOURCE_MOBILE` #6 bool, set at creation (`0x00C15370` ← `0x00A8E590` /
  `0x00A8DC10`) from the force's virtual `+0x38` ≠ 0, the same test that sorts land / naval in
  `0x008B2150` (so 1 = land; agents' mobiles are made with 1; every save checked: agents true, small
  fleets false); `+0x160` = the count of its unit list #5. CONFIRMED. CAI region `+0x15C` = the count
  of `CAI_REGION` #6 (`+0x150` list), filled by `0x00C13A50` from `0x00C140C0`, which the fort mirror
  builder `0x00A8DB00` calls with the CAI region at the fort's position: **the forts in the region**
  (CONFIRMED; empty in every save checked). CAI region `+0x140` = the `CAI_REGION` #3 list (the
  region's slot garrisonables, e.g. Gibraltar 2, Sevilla 3); `0x00C3F480` tests the slot entity's
  port object `+0x1E4` (the port node test of PATHFINDING_PORTS.md §3): **the port slots**
  (INFERRED mapping to the model's `RegionSlot::port`). The relation lists: the CAI world's factions
  except itself, both not the rebels (`IsFactionWithoutRecord`, those go to `+0x48`), the other in
  the game (`+0x824` clear), sorted by the other's relationship stance (`+0x790 → +0x10`: 2 → `+0x2C`
  allies, 0 → `+0x40` enemies). The economic need `0x00A9B810`: `I` = categories 0..12
  (`0x00BC7860`), `X` = 13..24 (`0x00BC7840`) over the newest `min(count, 5) − 2` records. The
  belief refreshes when read while dirty (`0x00A74AD0` → `0x00CF1170` `+0xF8` → `0x00CCCF70`).
  A technology's category is its key's prefix (`0x00C5B950` with `0x004F4480` = "starts with"):
  admin / economy / military → 0 / 1 / 2, else 3 — **ORIGINAL BUG**: 3 indexes past the belief's
  3-float array (`0x00A3AFE0` allocates `0x004CDC50` = 3) in `0x00A75CC0`; ours gives such a
  technology no need (no shipped key hits it).
- **Diplomatic pot spending (CONFIRMED structure).** `0x00CC53D0` (finance virtual `+0x40`) is told
  of an executed BDI intention (`0x00A6FE10` of its argument) and takes its cost (`0x00948970`) from
  the pot of the intention's kind (`0x004A5140`): 0/1 → `+0x130`, 2/3 → `+0x138`, 4..7 → `+0x128`,
  8 → `+0x140` (diplomatic), 9 → all four in proportion (all 0 when the cost exceeds their sum), 10 →
  none. Only the AI's own paid diplomatic intentions spend from `+0x140`.
- **Wired (round 3).** `CampaignModel::ai_refuses_deal` evaluates a deal of technologies as the exe
  (`ai_accepts_technologies`: the `diplomacy_options` refusal, the goal weights with
  `research_need` / `technology_spread`, the value at the campaign's inflation, `fair` / `good`).
  PROVISIONAL there: the budget is taken as ≥ 0 (our AI has no kind-8 intentions); a refused deal is
  declined where the exe may counter-offer (`0x00CC58C0`); the records the model does not hold
  (trade, payments, …) are not evaluated; the port-slot mapping above. A deal with regions keeps the
  PLACEHOLDER rule (the AI gives no region or technology in it).
- **Resume point:** the region value: `0x00C131C0` → `0x00A755E0` → `0x00A364B0` (per region
  `0x00AA1E90(region, faction, …)`, the `0x00C3E0C0` / CAI region `+0x12C → +0x34` virtual `+0x94`
  tests, `0x00A79050` attitude with the neighbour list) and `0x00C4D140` (faction `+0x938`).
  `0x00AA1E90` reads belief 0x4D (`0x00A75560`, settlement fields `+0xE8`/`+0xBC`/`+0xCC` UNKNOWN),
  belief `0x00A75460`, `0x00A63FD0`, the group-change state `0x00A634D0`, personality virtuals
  `+0x21C`/`+0x220`/`+0x22C`, `0x00A2C170`, `0x00C3E120`, `0x00C43AD0`, `0x00C40570` and the ×1.5
  allied-neighbour test; then evaluate record 4 and drop the PLACEHOLDER. The counter-offer
  `0x00CC58C0` stays its own item.

## 5. What the campaign scripts tell the AI (CONFIRMED calls, `analysis/worker3/lua_api.txt`)
- `force_diplomacy(a, b, option, offer, accept)` (714 calls): options `war`, `peace`, `alliance`,
  `break_alliance`, `military access`, `cancel military access`, `trade agreement`, `break_trade`,
  `join_war`, `payments`, `protectorate`, `regions`, `state_gift`, `technology`.
- `force_declare_war`, `force_make_peace`, `force_make_protectorate`, `force_make_trade_agreement`.
- `add/remove_restricted_unit_record`, `add/remove_restricted_building_level_record`.
- `set_campaign_ai_force_all_factions_boardering_humans_to_have_invasion_behaviour(bool)`,
  `disable_shopping_for_ai_under_shroud`, `disable/enable_movement_for_character`.
- Debug strings show more AI switches: "Add the raid/force culling/fort maintenance behaviour at a
  high priority to the named faction", `cai_attack_selected`, `cai_automanage`,
  `CCQ_CAI_EPISODE1_ATTACK/DEFEND`.
- Battle scripts drive AI units through unit controllers: `attack_unit`, `fire_at_will`, `halt`,
  `goto_location*`, `skirmish`, `defend_building`, `release_control` (back to the AI),
  `set_under_ai_control`.
`ntw_ai::campaign::ScriptHints` carries these (the caller copies them from
`ntw_script::ScriptState`: `restricted_units`, `restricted_buildings`, `diplomacy_options`).

## 6. Our implementation and what is PROVISIONAL
### Battle AI v1 (`crates/ntw_ai/src/battle/`)
Per alliance: line group (ATTACK_BATTLEGROUP Reform → Move to Form-up → Move to Target, or
DEFEND_LINE Reform → Defend Line), musket stop-and-shoot with the efficiency gradient, cavalry
wings (OUTFLANK Approaching/Outflanking) and the melee manager (CONFIRMED allocation, priorities as
in §3.3). Reactions: shaken units fall back behind steady ones (RETREAT), units turn to face an
enemy on their flank/rear, cavalry is drawn to enemies already in contact with our units.
PROVISIONAL: every distance/timer in `AiParams` except the 50 m force distance; formation
geometry (GroupFormations.bin not read); planner rule (attack when balance ≥ 0.5, defend otherwise,
stalemate breaker); assault rule; the direction table; the minimum melee potential (0.55);
class_term, morale factor and flag bonuses of the ratings; the artillery missile term.
Round 2: ATTACK_BATTLEGROUP transitions and thresholds from §3.2 (formed 60 %, 60° facing, 140 m /
160 m, once-per-attack outflank 5..85° at 1.1×); melee re-plan every 30 updates (CONFIRMED) with a
PROVISIONAL early re-plan for invalid objectives; PROVISIONAL pursuit radius 300 m (no map edge);
charges run at `battle_entities` run speed (`LandUnit::running`, PROVISIONAL that a charge runs the
whole way). Still PROVISIONAL: our "formed" measure (shape cohesion), the engaged test, outflank
point validity (100 m clearance) and side choice, the Reform timeout (AI update = battle tick is
CONFIRMED in round 3).
Round 3: STOP_AND_SHOOT is its own line tactic with the CONFIRMED FSM (ADVANCING_TOWARDS_LINE →
FORM_ON_LINE → HOLD_THE_LINE ⇄ CREEP_FORWARD; 20 m arrival, firing line at our longest range when we
outrange the target, creep = shortest range − 3 m, efficiency sampler and 200-tick / 0.01 rule).
DEFEND_ABSTRACT uses the CONFIRMED transitions (REFORM → DEFEND_LINE at 100 % formed; back to REFORM
when a unit leaves the line or below 50 %). PROVISIONAL: the trigger MOVE_TO_TARGET → STOP_AND_SHOOT
(target within longest range + 20 m), the firing line when the enemy outranges us (creep line
instead of the candidate-point search), the exit to the bayonet assault (muskets empty; a failed
hold when at least even or when creeping gains nothing), the 1 m arrival tolerance of line moves,
our formed percent (units within 8 m of their slot), the target battlegroup = the enemy centre.
Model limits: no formations, the AI plans on flat ground (the sim has heights).
Round 4: STOP_AND_SHOOT is entered by its CONFIRMED planner score tests and left (into the
bayonet assault) by its CONFIRMED keep tests (§3.1b), replacing the PROVISIONAL "+20 m" trigger,
the "muskets empty" exit and the "failed hold when at least even" assault; an inefficient hold now
only creeps (CONFIRMED FSM). Class matchups use the CONFIRMED class/category enums (§3.3 "Class
codes"; `battle::classes`) instead of "cavalry v infantry ×2/×3", and the missile class factor is
applied. PROVISIONAL / left out there: the orientation of the strength comparisons (INFERRED),
"formed ≥ 70" read as our formed fraction, the in-range test standing in for phase CONTACT, the
enemy army standing in for the target battlegroup, the UNKNOWN unit predicates and ability tests
of the class matchup, and units without `units` keys (tests) mapped from the model's flags.
### Campaign AI v1 (`crates/ntw_ai/src/campaign/`)
Manager behaviours ordered by DB priority (jittered by PRIORITY_RANDOMIZATION_DESIRE with the given
RNG); implemented: WAR_AND_PEACE, REGION_DEFENCE, EXPANSION_BEHAVIOUR,
GLOBAL_CONSTRUCTION_BEHAVIOUR, EXCESS_RECRUITMENT_BEHAVIOUR. Budgets from the spending biases.
Round 2: runs inside the turn loop (`driver`), uses the campaign RNG, the model's own recruitable
units / recruitment points / build options, and AttackForce / EnterSettlement / ConstructBuilding.
PROVISIONAL: manager/personality assignment; savings = % of gross income, upkeep guard (new units
only while upkeep <= income × (1 − savings %)); AI battles are autoresolved at once; how each tunable is combined;
army strength = Σ cdir quality × men/max; adjacency = 4 nearest settlements; threat radius 25 and
garrison radius 5 map units (median nearest-settlement distance is 27.5); one recruit and one
construction per region per turn; attitude terms; no separate peace while an ally fights.
Round 3: MERGE_UNITS (smallest land armies first join the nearest reachable own army at least as
big when both fit in 20 units) and TAXATION (per class, the highest `taxes_levels` level that keeps
that class's public order >= 0 in every owned region, else the lowest) on the model's
`MergeForces` / `SetTaxLevel`; both rules PROVISIONAL (the original's are BDI desires, not decoded).
AI handicaps: (superseded, 0-B 2026-10-09) the model now applies them as faction effects
(`ntw_sim::campaign::effects::apply_start_handicaps`, `0x008DD090`), so the AI reads its recruit prices,
handicap included, from the model's recruitable entries (`world::AiRecruitable`); the AI's own handicap
lookup (`CampaignAiData::{handicap, handicap_effects, ai_handicap}`) is removed.
Round 4: each faction runs its own stored manager and personality (CONFIRMED source, §2.3; the
naming rule is only a fallback). REGION_DEFENCE and EXPANSION are BDI desires with the CONFIRMED
priority rules (`campaign::desires`), served highest priority first. PROVISIONAL there: serving
desires directly (no goals / intentions), the region value (snapshot value × 100), the candidate
filters (threatened own regions; enemy regions within 2 × threat radius), the jitter draw.
MERGE_UNITS and TAXATION keep their round-3 PROVISIONAL rules (the original's MERGE_UNITS is not an
army merge, §4).
Round 5: the region value is the CONFIRMED composite analyser (`campaign::region_value`) on the
original's own stored base value when the startpos / save has one (`TurnContext::
region_base_values`), else on the formula with a GDP stand-in. Still PROVISIONAL: serving desires
directly (no goals / intentions), the candidate filters, the jitter draw, the region-group
states and loss counts (none in the model), MERGE_UNITS / TAXATION rules.
Round 6: the turn is a BDI pool (`campaign::bdi`, CONFIRMED processing and jitter; `Node`):
one component per manager row except WAR_AND_PEACE / DIPLOMACY_MANAGER, plus the 10.0
intention; REGION_DEFENCE makes one desire per own region (CONFIRMED multipliers),
REGION_GROUP_DEFENCE one goal per own region group (CONFIRMED multipliers) with
RECRUIT_STRENGTH_IN_REGION children (CONFIRMED `0.5 + 0.5^(i+1)`), EXCESS_RECRUITMENT the
weighted recruitment draws (CONFIRMED); intentions act in priority order. PROVISIONAL there: the
region groups (connected blocks under the nearest-settlements adjacency), which group regions
get recruit goals (border regions), a REGION_DEFENCE desire acts only while its region is
threatened (defend / recruit), invasion targets under EXPANSION_BEHAVIOUR (its real child, id
0x175, is not decoded), the recruitment inputs (`x` = our strength within the garrison radius,
unit budget = free recruitment capacity, money = our recruitment budget), diplomacy (our v1 rule
for every AI faction, since the WAR_AND_PEACE row builds nothing), GLOBAL_CONSTRUCTION /
MERGE_UNITS keep their v1 rules as intentions (TAXATION: CONFIRMED level 2, its tax domain = the
faction and no exemptions are PROVISIONAL), rows are added in name order (the
original: junction row order), no BDI state is kept between turns (the original keeps its pool
and re-jitters it, pool slot 11).
### Campaign AI, round 7
`RESEARCH_TECHNOLOGY` is a behaviour again: one `Node::ResearchTechnology` goal per candidate
technology with the CONFIRMED `1.0 × 0.95^k` link multipliers (`campaign::research`), each goal makes
one intention, and the intention starts the research at the best free school of ours
(`AiOrder::StartResearch` → the model's `start_research`). PROVISIONAL: the candidate list (available
technologies in research-cost order, not one per gentleman) and the school choice (best rate for the
technology's thread, not the gentleman's own settlement). Everything else about it is as §4
"RESEARCH_TECHNOLOGY" says.

### Battle AI, round 5
The high-level plan (`battle::plan`, CONFIRMED vote and modes) picks attack or defence every
think; the tactic auction (`battle::auction`, CONFIRMED algorithm) gives units to OUTFLANK /
DOUBLE_ENVELOPMENT with their CONFIRMED scores and claims, and STOP_AND_SHOOT is excluded while
they run. PROVISIONAL: the defender flag (default false: the AI attacks), the capture flags,
withdraw (off, played as defend), the ATTACK cluster rule, the outflank points and sides, the
sub-move-running test, the target battlegroup = the enemy army.

### Battle AI, round 6
The outflank points come from the CONFIRMED point test and side choice (`battle::outflank`):
OUTFLANK scores only with a point, DOUBLE_ENVELOPMENT only with points on both sides, and
OUTFLANK's units ride to its point. Each tactic has its own ten seeds (CONFIRMED layout), and the
commit flag `+0x4A` keeps a started OUTFLANK / DOUBLE_ENVELOPMENT out for the rest of its phase
and shuts out its partner (`auction::commit_allows`). PROVISIONAL: the target rectangle (the
enemy army: mean facing, spread), the battle area (the height grid) and terrain query (inside it),
the seeds drawn from a copy of the battle RNG, OUTFLANK's units spread around the point (the
sub-move objects are not decoded), DOUBLE_ENVELOPMENT's geometry.

## 7. UNKNOWN (next Ghidra targets)
1. RESOLVED round 3: DEFEND_ABSTRACT / STOP_AND_SHOOT transitions and the descriptors (§3.2).
   Round 4: the STOP_AND_SHOOT entry/exit are the planner's score/keep tests (§3.1b). RESOLVED
   round 6: the outflank point test `0x0078E3E0` and side choice (§3.1d), `0x007ADD80` (the battle
   RNG's `percent_0_100`), the candidate-line selection of `0x0085B530` (§3.2). Still open: who
   fills the STOP_AND_SHOOT candidate list (finder `+0x10/+0x14`); the terrain query
   `0x007E1BD0` / `0x007F1D40`; `0x007A6510`/`0x007A61F0` (ATTACK's own outflank points);
   ability 0x10; OUTFLANK's sub-moves (slot 11 `0x00754CC0` / start `0x007547D0` build a
   0x790-byte move object `0x0070B800` per side, using both outflank points; "Approaching /
   Outflanking" live there).
2. RESOLVED round 5: battlegroup formation, the plan vote and modes (§3.1c), the auction's
   details and the strength orientation of OUTFLANK / DOUBLE_ENVELOPMENT (§3.1d). RESOLVED round
   6: the seeds (each tactic draws ten at construction, `0x0070AED0`), the tactic flag `+0x4A`
   (set at a tactic's start, `0x00754780`). Open: whether the seeds' LCG (`(*arg)→+8→+0x50`) is the
   battle RNG; `0x0079C1E0` (INFERRED Σ melee ratings); the battle setup's `+0x88` writer (the
   alliance `+0x70` = defender is RESOLVED round 6, §3.1c),
   `+0x64`, the second army's `+0x220`, the first army's `+0xD4→+0xB8` / `+0xCC`, `+0x2D4`; the
   capture-flag list `+0x4C` (INFERRED victory conditions); ATTACK's clusters `0x007D6030`; the
   mode 3 handlers (`0x00769450` feature choice, `0x007695B0` line) and `0x007492F0` (objective
   resources); the `+0x17C` tracker.
3. RESOLVED round 4: class and category enums (§3.3 "Class codes"). Open: the predicates
   `0x0055B200`, `0x0055AC20` (state virtuals), `0x0055AD30`, abilities 4/6/0xF, the target flags
   `+0xADC→+0x278D/+0x278E`, unit AI states 2/3/5/6/7, the "area" test `0x007F1CF0`.
4. The direction table read by `0x00DAC7A0`; class_term of the melee rating.
5. RESOLVED round 3: one alliance-AI update per battle tick (CONFIRMED caller chain, §3.3);
   still open: `0x006387F0` period `N`.
6. CAI: RESOLVED round 4: manager/personality keys (§2.3), the personality slot map. RESOLVED
   round 5: the region value base (belief 0x4D) and the composite analyser's states (§4).
   RESOLVED round 6: how priorities are consumed (§4 "BDI pool processing"), the jitter writer,
   the behaviour → class table (corrected), REGION_DEFENCE, EXCESS_RECRUITMENT (entries and
   options), TAXATION (always level 2). **RESOLVED round 7 (structure):** RESEARCH_TECHNOLOGY — the
   behaviour, its goal → intention chain, the school reservation and the 0.95 link decay (§4
   "RESEARCH_TECHNOLOGY"). Open: its candidate key `0x00C288B0`, the analyser behind the goal
   refresh (`0x00C3FF10` → `0x00AA4BF0` → `0x006649C0`) and the effect test `0x00AAF1F0` (effect
   `0x27`); the tail of `0x00C2AD60` (the second list); the tax-exemption analysis (`0x00A74CF0`); MERGE_UNITS' step
   `0x00CD6260` (faction list `+0x14C`, item `+0x100→+0x4C/+0x50`); EXPANSION's spawned
   component (id 0x175, vtable `0x0138BCFC`, step `0x00CD5A50`); GLOBAL_CONSTRUCTION's real
   source of construction desires; the children of REGION_DEFENCE's desire (`0x01378770`) and of
   RECRUIT_STRENGTH_IN_REGION; the mission factory `0x00CEEB00` (kinds); the region-group
   beliefs (`0x00A88780`, `+0x138`); the pool budget `+0x7C` and who calls pool slots 10 / 11;
   the recruitment budgets (analyser `+0x60`, `0x00BBE910`, `0x00AAF5D0`); the settlement fields
   `+0xBC/+0xCC/+0xE8` of the base value; the counts `+0x104→+0x40/+0x44/+0x48`; `0x00898B20`,
   `0x009749B0`, the +5000 lists.
7. Difficulty: the campaign setup's `+0x3C/+0x40` values for AI factions (player value or its
   negation; the prefs are read by name, no reader reached); the writer of the battle army's
   `+0x234` level. RESOLVED round 5: the battle consumer `0x006AD6E0` and the UI index mapping
   (§4 "Difficulty"). Round 6: the per-faction block is stored in every save (`CAMPAIGN_PLAYER_SETUP_INGAME_MODIFIABLES`, `keys::read_difficulties`), so **a save from a campaign begun on hard or very hard settles the question** (user check); the setup's writer is still not reached.
8. For when the AI resumes (from 0-B, round 7):
   - Build planning uses the raw DB cost (`ntw_ai` data `AiBuildingInfo::cost`). The game charges
     `CampaignModel::construction_cost`: the chain-keyed cost entry of the region's effect set (CONFIRMED), e.g.
     −50 % on every chain in `spa_napoleon` (CAMPAIGN_FIDELITY.md §Construction cost).
   - Research is now modelled (`ntw_sim::campaign::research`, CAMPAIGN_FIDELITY.md §Research) and
     **picked by the AI since round 7** (RESEARCH_TECHNOLOGY, §4): every turn it starts a
     technology at its best free school (`AiOrder::StartResearch`). PROVISIONAL there: the candidate
     list (research-cost order, not the exe's per-gentleman analyser) and the school choice. The
     army / navy rate mods (`research_rate_mod_army_tech` / `_navy_tech`) are still not applied.
   - Buildings and units now need their technologies (`building_tech_ok` / `unit_tech_ok`), so the AI
     must not plan blocked options (`can_build` and `recruitable_units` already filter them).
   - (round 9) **Capture choice:** after taking a settlement the AI's occupy / loot / liberate choice comes from
     `0x00AAABA0` → the AI object's virtual `+0x2F8` (args: settlement, faction, capturing army, the preview struct
     with all three options; not decoded). The model applies **occupy** for every AI capture (PROVISIONAL,
     `capture::settle_capture`). The AI needs: the three `CapturePreview` options (money, town wealth after, damage,
     public-order reduction, liberation target) and its own war / treasury state; then call
     `resolve_capture` with its choice instead of the default.
   - (round 9) **Repairs:** the AI repairs through `0x00AA4910` (called from `0x00CE9A60`): when a building can be
     repaired (`0x00B1A6B0`) and paid for (`0x00B16430`, the AI's cost capped at its treasury) it calls the repair
     order `0x00B66260`. The model's stand-in repairs every damaged building an AI faction can pay the full cost
     of, at its turn start (`capture::ai_repairs`, PROVISIONAL). Which buildings `0x00CE9A60` picks, and when, is not
     read.
   - (round 9) **Diplomacy:** the model applies a deal once it is accepted (`CampaignCommand::Diplomacy { a, b, action }`,
     `treaties::DiplomaticAction`: alliance, break alliance, trade, break trade, embargo, military access and its
     cancellation, state gift, regular payment, protectorate; `DeclareWar` / `MakePeace` now run the relationship
     rules). Acceptance is the AI's (the negotiation evaluation, the `diplomacy_options` permissions #18, the table
     `0x01459080`). What it needs from the model: the attitude total and category (`Relationship::attitude_total`,
     `CampaignModel::attitude_category`, thresholds from `diplomatic_relations_attitudes`), the war balances #7 / #10 /
     #11 / #12 / #13 (kept up to date by the per-turn update), the treaty-break counters (`World::treaty_breaks`,
     `alliances_broken`), friendship #15, the alliance commitment #6, and the gift formula (`state_gift`). Calling
     allies into a war (`0x00B268B0`: AI allies decide, human allies get an offer) is not done by the model.
   - (round 10) **Allies called into a war** are now done by the model (`CampaignModel::call_allies`): the AI ally's
     join decision (`0x00AAA950` → `0x00AAA920`, an AI virtual) is PROVISIONAL "join unless also allied to the enemy".
     Joining is `join_war(x, enemy, ally)`, refusing breaks the alliance. A human ally is not called (the offer UI).

## 8. What the models need (for the manager's backlog)
The AI decodes and ports more than the models can show yet. Items for other backlog sections:
- **Battle model (§3):** formations and a formed percentage (`GroupFormations.bin`); abilities
  (0x10 stationary mode, 0x11 rally, 0x12 inspire, 4, 6, 0xF dismount); a general's unit
  (`general` class, GENERAL_SUPPORT); buildings / garrisons; artillery limbering and deployed
  positions (LIMBERED_ARTILLERY); squares; routing / shattered target flags (`+0x278D/E` are
  candidates); heights in AI planning; unit AI states; a melee-strength and missile-strength
  rating per unit that matches `+0xBE8/+0xBEC`.
- **Battle setup (§3):** the `units` class/category keys are now on `LandUnit` (set by the app);
  battle difficulty per army (`+0x6E8` INFERRED) and its consumer: the per-army attack bonus
  (`0x006AD6E0`: AI armies +4 / +8 attack at level 1 / 2, one level less per 5 units the AI side
  has over the enemy, none in the first 6 ticks; human armies +4 at level 1) belongs in the
  battle model's melee; **attacker / defender per side**: the battle setup's defender alliance index
  (`+0x88`, −1 = none; CONFIRMED round 6) must come from the campaign's pending battle (the
  defending side of `PendingBattle`) when campaign battles are launched (not wired yet: the app
  autoresolves), then `BattleAi::set_defender(side, true)`; historical battle XMLs have no such field
  (INFERRED none: both sides attack); the battle `playable_area` (dimension, centre) of the XMLs is
  the battle area the outflank point test clamps to (`battle::outflank::Area`, today the height
  grid);
  settlement-capture victory conditions (the plan's
  capture flags, mode 10); visibility (the trackers only count units with `+0x1E4`, so a hidden
  enemy triggers the search mode 8); map edges for withdraw (mode 9).
- **Campaign model (§5):** handicap effects applied in the economy (recruitment / upkeep / GDP /
  research / policing modifiers from `campaign_difficulty_handicap_effects`); the per-faction
  difficulty block and human flag; the CAI manager type per faction (`CAI_INTERFACE_MANAGERS`);
  region groups (theatres) and their change states (lost / reduced / split / new / increased /
  merged) for the composite value analyser; diplomacy offers (WAR_AND_PEACE, HELP_ALLY);
  research (RESEARCH desires); agents (MISSIONARY, assassins); fleets and naval invasions (NAVY_*,
  PORT_BLOCKADE, invasion goals); sieges; region taxation exemptions (TAX_EXEMPT_REGIONS); trade
  routes / trade areas (TRADE_*).
- **Campaign data (§1):** reading the startpos/save `CAI_INTERFACE` BDI pools (desires, goals,
  intentions, beliefs) would let the AI start from the original's own state. Spec so far (round 5,
  `esf_dump schema` on eur_napoleon): `CAI_INTERFACE` v13 = `[u32, {CAI_BDI_COMPONENT_PROPERTY_SET},
  (component block), {CAI_WORLD}, {CAI_BDI_POOL}, {CAI_CENTRAL_BDI_POOL}, [CAI_INTERFACE_MANAGERS],
  …, {CAI_INTERFACE_DIRECTOR_BDI_POOL}, {CAI_INVASION_TRACKING_SYSTEM_INFORMATION}]`; every BDI
  component (world objects and pool entries) repeats a block `{PROPERTY_SET v1 [u32 ?, u32 type,
  u32 ?, bool, u32 type]}, u32 id, f32 ×4, u32, bool, u32[] ×2, u32, u32, u32[] ×2,
  [CAI_BDI_COMPONENT_BLOCK_OWNS {u32 owner, u32 owned, f32, f32, u32}], u32, u32, u32[] ×4 (the
  last two: owners and owned ids), bool, u32, bool` followed by the class records (e.g. a region
  base value: `{CAI_BASE_VALUE i32}{CAI_REGION_BASE_VALUE u32 region id, i32[11]}`).
  `CAI_BDI_POOL` v1 = `[bool, [CAI_BDI_POOL_BELIEFS] (2613), [CAI_BDI_POOL_DESIRES],
  [CAI_BDI_POOL_INTENTIONS], [CAI_BDI_POOL_FAILED], u32, bool, u32]`; `CAI_WORLD` lists theatres,
  factions (42), regions (101 incl. sea/theatre areas; `CAI_REGION #10` key), settlements (72),
  slots, units, characters, trade routes, … each with the component block. The AI worker reads the
  region base values itself (`keys::read_region_base_values`); a full BDI pool reader (desire and
  intention records) belongs in the ESF loader (campaign-data).

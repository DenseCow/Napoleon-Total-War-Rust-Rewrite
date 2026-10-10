# Characters and agents fidelity (backlog §0, slot 0-G)

Worker: 0-G (branch `work/fidelity-characters`, worktree `%USERPROFILE%\Documents\NR-fidelity-mw`). Tags: CONFIRMED /
INFERRED / UNKNOWN; stand-ins PROVISIONAL / PLACEHOLDER. Ghidra: own copy `NR-f0c-ghidra` (`run_ghidra.ps1`, helper
`ghidra_scripts/F0cDecomp.java`). Specs in our words; no decompiled code is stored. Evidence: vanilla saves only
(read-only copies in `target\tmp\vanilla` of `NR-save-compat\target\evidence`: `orig_fr_t1` = Peninsular campaign,
France, turn 1, March 1811; `orig_fr_may1811` = the same campaign at turn 4; `auto_orig_spa_0245` = same as t1).
Effects are read through the effects store (`EFFECTS_FIDELITY.md` §4).

## Where I am / what's next
- 2026-10-04 (takeover by the pathfinding worker; branch `work/fidelity-characters` restarted from origin/main
  b988db2, worktree `%USERPROFILE%\Documents\NR-characters`, Ghidra copy `NR-ai-ghidra` with `AiDecomp.java` from
  NR-ai6): **who takes command when a commander goes** traced and ported (§5b): the original sorts the force's units
  (General / admiral first, then rank, category, `units` #21, age, `units` #7, serial); the first unit's character
  takes over, else a new colonel / captain joins that unit; a force without units goes. Also from the exe: a dying
  General loses his own unit first; +0x52C (`CHARACTER` #34) characters skip the death check and come back when
  killed (escape in the model, PROVISIONAL). Faction-leader succession (royal family) and vacated posts (the
  government's new-minister slot) are decoded in structure but not modelled (§5b). Checks below.
- Round 3 (2026-10-04, on origin/main 976b0a0): the royal family, succession and vacated posts traced and ported
  (§5c): the FAMILY record loaded and written, the yearly family pass, the succession, a new minister at once for a
  post whose holder dies (the leader's post of a monarchy goes to the family's successor). The leader exemption is
  gone. Writer changes (save.rs, coordinate with the save worker): minister templates, `CHARACTER` #8, post holders,
  FAMILY, model-named new characters.
- Round 4 (2026-10-04): agent actions (§7): the chance table, the roll and outcomes, assassination and duel chances
  and orders ported as commands; spy / sabotage chances decoded; ministers' dismissal and appointment with the spare
  list ported as commands.
- Round 5 (2026-10-04): diplomatic reactions of agent actions (the config triples and factor setters), the duel
  ending (CONFIRMED: the loser dies, a scripted spare check UNKNOWN; duel counters `CHARACTER` #36 / #37), army sabotage
  (action points to 0 next turn, guerilla casualties) and building sabotage (health damage) as commands, technology
  stealing in the research step; spying, conversion and exchange documented with what they need (§7).
- Round 6 (2026-10-04): the campaign sight model (§10): the sight grid and quad trees, every sight source (characters
  with their saved radius `CHARACTER` #17, settlements, regions, trade route segments), the shrouds loaded and kept
  per turn, hidden characters and the exposed lists (`CHARACTER` #22, `EXPOSED_CHARACTERS`), the pathfinder's hidden
  obstacles (mode 5), spying ported as a command; `exchange` decoded (a forces order). Writer requests in §10.
- Round 7 (2026-10-04, closing 0-G):
  - **Hidden armies:** the stealth test is ported and gives every saved hidden flag (118 of 118 commanders in 4
    vanilla saves). It uses the ground types (`regions.esf` `groundtypes` with `campaign_ground_types` #2) and the
    unit flag `unit_stats_land` #69.
  - **Hidden flag refresh:** at campaign creation, at each turn start and after moves.
  - **Spotting pass:** ported (§10).
  - **Shroud timing:** reset at the turn start, added to after moves. INFERRED from 10 saves; the time-boxed trace
    did not reach the tree writes.
  - **Protectorate sight-sharing:** CONFIRMED by the only protectorate in the start positions.
  - **Recruitment pools:** they run, and `HireGeneral` hires with the exe's cost and the culture's general unit (§8).
  - **Turn-end counters:** ported and read by the script conditions (§6).
  - **`CharacterCreated` and `CharacterPromoted`:** now fired. Together they have 104 handlers in
    `export_triggers.lua`.
  - **Conversion:** confirmed to be the region religion model (0-B's `religion.rs`). Missionaries convert passively
    by their rank; there is no conversion order or chance (the chance table `0x00951F80` has no conversion entry).
  - **Closing summary:** §11.
- Round 8 (2026-10-04, branch `characters-ai6`, on origin/main e8c4bdd; closing the residuals, §12): the shroud's
  visible tree is rebuilt at the faction's turn END (`0x008BD0F0` → `0x00B66EF0`) and only grows in the turn
  (CONFIRMED from the tree ops; the model's reset moved to `FactionEnd`); the "queued reveal areas" are the human's
  spy-network discs (`0x008F9A00`: `subterfuge` > 0 and 3+ idle turns; `CharacterBuildsSpyNetwork`, message 250);
  the pool step's phase (faction +0x940, after the spotting pass); historical candidates (`historical_characters`
  rows due by faction / type / year, not in `HISTORICAL_CHARACTER_MANAGER`'s created list, picked by
  `uniform_below`); hiring into an army / `HireAdmiral` onto a fleet; `CharacterPromoted` fires on the field
  promotion of a unit's commander (`PromoteUnit`), not on hiring; the duel loser of an ordinary outcome flees
  (`CHARACTER` #27) instead of dying; the +0x52C copy stands where he fell with his counters reset;
  `IsFactionLeaderFemale` from the family; the two episodic force-success switches (`EPISODIC_RESTRICTIONS` #20 /
  #22); 13 agent script events fired at the original's sites. Writer: `CHARACTER` #14 / #15 / #27 / #28..#30.
  Probe for the one PROVISIONAL left (the promotion cost): `analysis/fidelity/debugger/0g_promotion_probe.cdb.txt`.
- Next: nothing open in 0-G (§11 table, §12).
- Checks (rounds 3 to 7, the same results each time): `cargo build --workspace`, `cargo test --workspace` pass; clippy 97 warnings as on main, none in
  the changed files; game runs (main menu, options, `--campaign --campaign-faction france --campaign-end-turn 3`,
  `--campaign-demo-embark`): no panic; `ai_turns_save 26`: 0 `save_check` violations.
- 2026-10-04: traits and ancillaries are gained in play: the exe's rules are in `ntw_sim::campaign::characters`
  (§1, §2), the trigger scripts run through `ntw_script` with `effect.trait` / `ancillary` / `remove_*` and 25
  character conditions implemented (§4). The yearly natural-death pass and ancillary expiry run at the year's last
  round end (§5). Tests: `ntw_campaign/tests/characters.rs`, `ntw_data` `character_tables`, unit tests. Run:
  `cargo run -p ntw_ai --release --example character_run -- 26` (France, eur): about 4 traits and 3 ancillaries
  gained per turn world-wide; the year end runs the death pass.
- Also read: the recruitment pool and how new characters are made (§8, pools loaded and candidates excluded from
  gains), death in battle and the other kill reasons (§9).
- (Round 1's list, now folded into the Next item above: the pool running needs refill timers, candidate creation and
  the recruit command; the event facts are battle results, research, building, duels.)
- Checks (2026-10-04, merged with origin/main 8ca922f): `cargo build --workspace`, `cargo test --workspace` pass;
  `cargo clippy --workspace --all-targets` adds no warnings in the files of this slot; game run `napoleon --campaign
  --campaign-faction france --campaign-end-turn 4 --screenshot ...`: 4 End Turns, 0 script errors, no panic;
  `character_run 26`: traits / ancillaries gained every turn, the 1805 year end runs the death pass.
- Save (done by save-compat, SAVE_COMPAT.md §23): traits with points and ancillaries are written for every character,
  the dead are dropped through the removal cascade, the pools are written from the model; a 26-turn save has 0
  `save_check` violations.

## Resolved / open
| topic | state |
|---|---|
| trait gain: `effect.trait` handler, chance roll, exclusions | CONFIRMED (§1), modelled |
| trait points, antitraits, max traits, eviction, levels | CONFIRMED (§1), modelled |
| ancillary gain: chance, date, uniqueness, agent/subculture/exclusion, max, eviction | CONFIRMED (§2), modelled |
| binding RNGs (own LCG states, start 0x61266, not saved) | CONFIRMED (§1), modelled (restart per model: PROVISIONAL) |
| trigger conditions | 25 of ~80 implemented from the exe descriptions (§4); battle/duel/research/building ones open |
| when the trigger events fire | CharacterTurnEnd, CharacterCompletedBattle, BuildingCompleted, ResearchCompleted, CharacterCreated (every new character), CharacterPromoted (the field promotion `PromoteUnit`, CONFIRMED site `0x00A1A300`), and the 13 espionage / duel / spy-network events at their CONFIRMED sites (§12) fire from the model |
| per-character turn counters (turns at home / at sea / in enemy lands, idle) | CONFIRMED (§6), modelled; the script conditions read them |
| ageing and natural death | CONFIRMED (§5), modelled (every character draws, leaders too, §5c) |
| ancillary expiry by end year | CONFIRMED (§5), modelled |
| battle death of generals | reason 1 = killed with his unit / force (§9); modelled, commander included |
| who takes command when a commander goes | CONFIRMED (§5b): the original's unit order; the unit's character or a new colonel / captain; modelled (2 keys PROVISIONAL) |
| force without units when its commander goes | CONFIRMED (§5b): removed, modelled |
| +0x52C characters (`CHARACTER` #34) | CONFIRMED: skip the death check, rebuilt when killed as a copy where he stood, counters reset, linked to his force (§5b, §12); modelled (riding along not kept: no riders in the model) |
| royal family (FAMILY) yearly pass | CONFIRMED (§5c), modelled (new members' names / portraits PLACEHOLDER) |
| faction leader succession | CONFIRMED (§5c), modelled (claimant faction UNKNOWN, not modelled) |
| vacated posts | CONFIRMED code, trigger INFERRED (§5c): a new minister at once; modelled |
| recruitment pool (who appears, when) | CONFIRMED (§8, §12), modelled: refill at the faction turn start after the spotting pass, historical candidates first (table rows due by faction / type / year, the created list), generic ones else; `HireGeneral { into }` and `HireAdmiral` with the exe's cost; `PromoteUnit` (cost PROVISIONAL) |
| new character age, appeared turn | CONFIRMED (§8): age 21..40, +0x4EC = turns elapsed |
| portraits of new characters (pool hires, field promotions) | CONFIRMED (§14), modelled: the portrait allocator loaded (one campaign-RNG step per deck) and saved, drawn by agent folder and age; builder ORIGINAL BUG (last picture never dealt) recorded, no builder in ours yet; ministers / family / trained agents still from templates |
| agent actions and success formulas | CONFIRMED (§7, §10, §12): assassination, duel (the loser of an ordinary outcome flees, `CHARACTER` #27), army and building sabotage, spying, technology stealing ported with their script events; diplomatic reactions ported; the episodic force-success switches honoured; conversion with 0-B (region religion); exchange decoded (a forces order, §10) |
| campaign sight (grid, sources, shroud) | CONFIRMED (§10, §12), modelled: loaded, added to after each walk and at the turn start, rebuilt at the faction's turn end (timing CONFIRMED from the tree ops), the human's spy-network discs as sources; gates hidden obstacles; protectorate sharing CONFIRMED; trade-route lists frozen at load (routes built in play are the trade area's) |
| hidden characters, exposed lists, spotting | CONFIRMED (§10, §12), modelled: knows / expose, the stealth test (equals all 118 saved commander flags of 4 vanilla saves; a duel loser who fled, `CHARACTER` #27, cannot hide), the hidden flag refresh, the spotting pass; used by pathing and agent actions |
| minister dismissal / appointment, spare ministers | CONFIRMED (§7), ported (commands) |
| governor appointment | open (governorships are posts; the same commands apply, not checked) |

## 1. Traits (CONFIRMED unless tagged)
- Triggers live in `data.pack/export_triggers.lua` (555 `effect.trait` calls), loaded by the loose
  `data/all_scripted.lua` with the ancillary, historic character and mission exports. Each trigger appends a handler to
  an `events.<Event>` list; the handler tests `conditions.*` and calls `effect.trait(key, "agent", points, chance,
  context)` (every call uses scope "agent", points 1..10, chance 1..100). Two calls in one trigger roll independently.
- Script binding `trait` (handler `0x008F5070`; registered "trait key, points to add, chance of adding"): draws r =
  `percent_0_100` (`0x007ADD80`: MSVC LCG, high 16 bits, values < 88 redrawn, `% 101`, so uniform 0..100) and goes on
  only if r ≤ chance (a bool tweak at +0x5C forces success). For a character context it then needs: the character's
  +0x520 flag clear, the global "non-scripted traits disabled" flag (+0x116, `set_non_scripted_traits_disabled`) clear,
  and the character not being a governor whose theatre is the faction's home theatre (+0x734). It queues
  `CCQ_ADD_CHARACTER_TRAIT_POINTS`; the executor `0x008A9410` calls the add routine `0x008C2D30(trait, points, fire
  events, faction +0x514)`. Region and unit contexts use `CCQ_ADD_REGION_TRAIT_POINTS` / `CCQ_ADD_UNIT_TRAIT_POINTS`
  (not used by the shipped triggers). A character not yet in play (a recruitment-pool candidate, context vfunc +0x70)
  gets the trait appended to a pending list instead (`0x008E2DD0`).
- Add routine `0x008C2D30` (points p > 0):
  1. Antitraits: every held trait E whose antitrait list (`trait_to_antitraits` rows (E, T)) contains the new trait T
     loses p points (`0x008D21B0`/`0x008B9EE0`); if E's points fall below 1, E is removed and the leftover |E.points|
     continues as p; if E absorbs everything nothing is added.
  2. T already held: points += p and the level is recomputed (`0x008CA0E0`).
  3. T new: if the character already holds `max_traits` traits (tweak, default 6, "Define how many traits a character
     can have at any one time"), the held entries are sorted by (`character_traits` #3, then points) ascending and the
     first is evicted when its #3 is lower than T's, or equal with fewer points than p; then, if there is room, T is
     added with max(0, p) points.
  4. The character's trait effect set is rebuilt (`0x008EDAE0`).
- Level (`0x008B5380`): highest level whose threshold ≤ max(points, 0); none below the first threshold (e.g. Soult's
  `C_Sausage_Vice` at 1 point in `orig_fr_may1811`); falling points never drop below the trait's no-going-back level
  (`character_traits` #1) once reached.
- `character_traits` = {key, #1 no-going-back level, #2 bool (INFERRED hidden), #3 i32 eviction priority, #4 category};
  `trait_info` maps each trait to its scope ("agent").

## 2. Ancillaries (CONFIRMED unless tagged)
- Triggers in `export_ancillaries.lua` (272 `effect.ancillary(key, chance, context)`, 2 `remove_ancillary`).
- Binding `ancillary` (`0x008AAC30`): same roll, but goes on only if r < chance (strict: chance 1 = 1/101); same
  exclusions with the character's +0x521 flag and the global +0x117 flag; queues `CCQ_ADD_ANCILLARY` (executor
  `0x009316C0` → add core `0x00A180C0`).
- Add core `0x00A180C0`, in order: the ancillary's type (#2) must be `character`; current year in [start year #7, end
  year #8) ("failed_date_check"); world-unique ancillaries (record +0x21) must not exist anywhere and faction-unique
  ones (+0x22) not in the faction (uniqueness monitors, `0x008AABF0`); then `0x009D1B10`: the character's agent type
  must be in `ancillary_to_included_agents` ("failed_character_type") and his subculture in
  `ancillary_included_subcultures` ("failed_culture_type"); then `0x009D1B90`: not already held, not excluded by
  `ancillary_to_excluded_ancillaries` against a held one, and if he holds `max_ancillaries` (tweak, default 3, "How many
  ancillaries a character can have at any one time") the new one's #6 must be higher than the lowest held one's #6,
  which is then removed (`0x009C80C0`). The column-to-flag mapping of the three bools (#3..#5) is INFERRED (#4 world
  unique, #5 faction unique).

## 3. Script bindings
Every condition and effect the campaign scripts can call is registered in the static-init code 0x00418000..0x00423000
by six pushes {handler, category, example, description, argument text, name} and a call to `0x00DF15C0` (233
conditions) or `0x00DF18C0` (7 effects). Recipe: `insnr:0x00418000:0x00423000:` over that range, keep the calls with
six pushes, resolve the strings with `strat:`. The resulting table is bulk exe text, kept local
(`target	mpmpaign_script_bindings.tsv`, not committed). Handlers used here: §4.

## 4. Conditions and actions in the model (`ntw_script::characters`)
- Actions: `effect.trait(key, scope, points, chance)` → `characters::roll_trait`; `effect.ancillary(key, chance)` →
  `roll_ancillary`; `remove_trait` / `remove_ancillary`. Results are written to `ScriptState::character_log`.
- Conditions implemented (character context; meaning = the exe's registered description, handler in brackets):
  `CharacterType` (0x0089E0D0: the agent type string, compared case-sensitively by `0x004F1F30`, so the scripts'
  `"Catholic_Missionary"` and `"gent"` never match, as in the original), `CharacterCultureType` (faction subculture →
  culture), `CharacterFactionName`, `CharacterHasAncillary`, `CharacterHasTrait`, `CharacterTrait` (0x0089DED0: the
  held trait's **points**, 0 if not held: entry +8), `FactionLeadersTrait` (same for the leader), `DateInRange` (start ≤
  year ≤ end), `CharacterMinisterialPosition`, `CharacterHoldsPost` (a minister with a post), `IsTheatreGovernor`,
  `IsFactionLeader`, `IsFactionLeaderFemale` (false: no gender in the model, PROVISIONAL), `FactionGovernmentType`,
  `CharacterFactionHasTechType`, `FactionwideAncillaryTypeExists`, `WorldwideAncillaryTypeExists`,
  `CharacterFactionGeneralCount` / `AdmiralCount`, `CharacterAttribute`, `CharacterForename` / `Surname`,
  `InSettlement`, `OnAWarFooting`, `IsGuerrillaGeneral`.
- Still logging stubs (false / 0): the battle facts (`CharacterWonBattle`, `CharacterWasAttacker`, `BattleResult`,
  `CampaignBattleType`, percentages, `CommanderFoughtIn*`, `CharacterRouted`, ...), duels, `ResearchType`,
  `CharacterBuildingConstructed`, `IsBuildingInChain`, `CharacterInRegion`, `CharacterInEnemyLands`, the turn counters
  (§6), `NoActionThisTurn`, `InsurrectionCrushed`, `FactionDestroyedByCharacterFaction`, and a few faction ones.

## 5. Ageing and natural death (CONFIRMED unless tagged)
- When: the round end (`0x008A9920`) passes "this is the year's last turn" (`turn_in_year + 1 == turns_per_year`) to
  the per-faction pass `0x00948CF0` → `0x008BC650`; with the flag set, every character of the faction goes through
  `0x009DA190`, so the check runs **once a year**, before the calendar moves on.
- Who: characters that appeared more than 2 turns ago (+0x4EC + 2 < turns elapsed) and without the flag +0x52C.
- Age = current year − birth year (`0x009C9C70`; a dead character's age uses his death year +0x6C).
- Chance (`0x009D5950`): a 101-entry table built once (`0x009D0740`): per age band the percent of characters who die in
  it, spread evenly over the band's ages: 51–55 3, 56–60 7, 61–65 17, 66–70 27, 71–75 16, 76–80 12, 81–85 8,
  86–90 6, 91–95 3, 96–100 1 (sum 100); then `0x009DBD40` turns each entry into a hazard: density / Σ density from that
  age on (so age 100 → 1). One draw u = next16 / 65535 on the campaign RNG (world +0xFB8) per character below 101: he
  dies if hazard > 0 and u ≤ hazard; from 101 he always dies. The dead are removed after the faction's pass
  (`0x00A0C6F0`, reason 3 = natural causes; message `character_dies_natural_causes`).
- Ancillary expiry (`0x009D2E50`, same yearly pass): ancillaries whose end year ≤ the current year are removed.
- +0x52C is `CHARACTER` #34 (loader `0x00991520`; true for exactly one General per major power in the vanilla saves,
  5 per eur save: Napoleon for France): such a character is skipped **without a draw**, and when killed he is
  rebuilt as a copy of himself (§5b). Modelled: `CharacterDetails::returns_after_death`.
- Before the kill, a dying **General** with a unit link (character +0x2B0; INFERRED = `CHARACTER` #5, his own unit)
  has that unit deleted (`0x008BC650`; admirals, colonels and others keep theirs). Modelled.
- Model: `characters::yearly_character_pass` from `TurnStep::RoundEnd`; a force he led goes to a successor (§5b).
  Every character draws (leaders too); a post the dead held gets a new holder at once (§5c). PROVISIONAL: loaded
  characters count as old (+0x4EC is not in the save).

## 5b. When a commander goes: successor, empty forces, returning commanders, leaders and posts (2026-10-04)
- **Empty force** (CONFIRMED, destructor `0x0099D2D0`): when the dying character's force (+0x2A8 = `CHARACTER` #4)
  has no units left (its count, vt+8, is 0), the force's remaining characters are killed (reason 1, `0x008BA4E0`)
  and the force is removed. Model: `CampaignModel::command_vacated` removes it.
- **Successor** (CONFIRMED code; the trigger for natural death INFERRED: the destructor tells the character's
  observers, and the unit-side observer `0x008DB3E0`, reached through a vtable, re-picks when the dying character is
  its force's commander). The pick `0x008B8E50` → `0x008ED2C0` is also called directly after a unit merge that takes
  the commander's unit (`0x008D93D0`, which kills him with reason 1), a force split (`0x008B9030`, both halves) and
  `0x00A16110`. It sorts the force's units (`0x00869120` with `0x00899450` → `0x008CF620` on keys from the unit
  classes' slot +0x60: `0x008CB4A0` land, `0x008CB570` naval) and takes the first:
  1. a unit whose character is a General (army; agent type 0) / admiral (navy; type 1) first;
  2. the higher rank of the unit's character: character +0xA4 + 4·{0 land, 1 navy}, compared unsigned (INFERRED: the
     saved `AgentAttributes` in order, whose first two are `command_land`, `command_sea`; 14 attributes, 14 slots);
     0 for a unit without a character;
  3. the lower unit category, `UNIT_RECORD` +0x1C from `units` #2 (`0x00EED2B0`: cavalry 0, artillery 1, infantry 2,
     dragoons 3, elephants 4 (counted as 0), camels 5, line of battle 6, frigate 7, galley 8, specialist 9, auxiliary
     10, merchant 11, invasion fleet 12);
  4. land only: `units` #21 (record +0x78, meaning UNKNOWN) clear before set;
  5. the older unit: unit +0x58..+0x60, three fields of the world date copied when the unit is made (`0x0088C710`);
  6. the higher `units` #7 (record +0x38, meaning UNKNOWN, about 0.8 × cost);
  7. the lower unit serial (unit +4, a running counter).
  If that unit has a character he takes command (whatever his type: a colonel too), moved to the old commander's
  position (`0x00963840`); if not, the unit class's slot +4 makes one: land `0x008B7EF0` a **colonel**, naval
  `0x008B7F60` a **captain** (only when the ship has none), both through `0x00990EF0` (agent type 2 / 3 into the agent
  name table at 0x0145D9E0: General, admiral, colonel, captain, gentleman, Eastern_Scholar, the missionaries, rake,
  assassin, minister, bandit, pirate), linked as the force's commander (`0x008CFBD0`).
  Model: `CampaignModel::commander_unit` (keys 1-4 and 6 as above; 5 skipped and 7 = the unit's place in the force:
  PROVISIONAL, the model keeps no raise date or serial) and `command_vacated`; used by `character_dies` (natural
  death), `character_falls` (battle: a lost unit's character falls, the commander included, and what is left gets a
  successor) and the escape path. Tests: `campaign::tests` (`commander_pick_follows_the_originals_unit_order`,
  `a_general_riding_with_the_force_takes_command`, `a_fleet_gets_a_captain_on_its_first_ship_in_order`,
  `a_returning_commander_escapes_and_his_force_is_handed_on`, `a_general_dying_of_old_age_loses_his_own_unit_first`),
  `save_compat` (`character_changes_and_deaths_are_written`). A 26-turn AI run (`ai_turns_save 26`) saves with 0
  `save_check` violations.
- **Returning commanders** (+0x52C, CONFIRMED code): when such a character is destroyed, a 0x550-byte copy is built from
  him (`0x0098FE60`), linked to his force if it still exists (`0x008B02E0`, `0x008CF9D0`) and placed (`0x009D3B60(0,
  ..)`); `0x00A0B980` (+0x52C and two global flags) chooses the wounded message (`0x009D2240`). INFERRED: Napoleon and
  the other famous commanders are wounded and come back. Round 8 (§12, CONFIRMED): the copy keeps his details and
  counters except the idle turns, the hidden flag and the duel wound, is linked to his force as a rider when it
  still has units, and enters the world where he stood. Model: he stays where he fell with those fields reset, his
  force gets a successor (riding along is not kept: the model has no riders).
- Faction leaders, the royal family and vacated posts: §5c (round 3). Correction to round 2: character +0x4E8 and
  `0x00A0C810` / `0x009DA300` are not about posts (they set a "near a settlement" state and its name); the post side
  is the government's own, §5c.

## 5c. The royal family, succession and vacated posts (2026-10-04, round 3)
- **FAMILY record** (CONFIRMED, loader `0x0087D490`, member loader `0x008C2390`, saver `0x00890490`): ten
  `FAMILY::MONARCHY_INFO_CHARACTER` (0x84 bytes each at family +4): 0 the leader, 1 the spouse, 2..5 the leader's
  children, 6..9 relatives who inherit when no child does; then a u8 (the heir child's index: the loader flags child
  #u8 when below 4) and `ORDINAL_PAIR[]` {name, count}. Member fields: #0 name loc (+0x00/+0x0C), #1 male (+0x19),
  #2 present (+0x1A), #3 children count (+0x1B), #4 0..2 (+0x58, meaning UNKNOWN), #5 age (+0x5C), #6 regnal number
  (+0x2C), #7..#10 his age at each child's birth (+0x1C..+0x28), #11 portrait (+0x30), #12 religion (+0x60), #13 the
  owner's id (+0x64), #14 married (the owner's id, 0 = unmarried; +0x68), #15 a name loc present when #14 is not 0
  (+0x6C). Vanilla saves: Christian VII #6 = 7, George III 3; relatives aged 60+ have four children born at 46..49 (the
  fertility wrap below). Family +0x544 "daughters may inherit" is set false by the loader (where else: UNKNOWN).
- **Yearly family pass** (`0x008BC9C0`, first in the year-end faction pass `0x008BC650`, for a living faction with a
  family; CONFIRMED order, all draws on the campaign RNG): leader age +1; each present child age +1 and his year
  (`0x008B45E0`: unmarried, marries when 25 + int_range(0, 10) < age; married with < 4 children, percent_0_100 <
  fertility → a child, remembering his age); a child turning 16 draws #4 (uniform_below(3)). Unmarried leader (or
  spouse gone): marries when 25 + int_range(0, 10) < age, the spouse the other sex aged age − (king) / + (queen)
  uniform_below(5), #4 drawn. Married: spouse age +1; < 4 children: percent_0_100 < fertility → a newborn (a boy when
  percent_0_100 > 50). Fertility (`0x008B3AD0`) = (135 − 3 × age) as u16 / (children + 1), so past 45 it wraps to a
  huge number and a child comes every year. Each named relative: age +1 and his year. Death checks (the character
  death table): spouse, present children, named relatives (a dead relative's slot is closed up). With n < 4
  relatives: percent_0_100 ≤ trunc(2^−n × 100) adds one aged int_range(0, 34) + 16 (a man unless daughters may
  inherit). In a republic, the leader's own death check runs the succession.
- **Succession** (`0x008B9500`, CONFIRMED): the first of the leader's children flagged as heir (a daughter when
  daughters may not inherit passes it to the first son); else the first relative (one aged 16..50 is made first when
  there is none), the others moving up. Regnal number from the ordinal list (+1 for a known name, else the name is
  added). A married successor gets a spouse (a king's wife: age − uniform_below(5), at least 16; a queen's husband:
  age + the draw), keeping the #15 name if any, with his children count; his children are made anew (newborns aged
  his age − his age at their birth), the rest cleared; child 0 becomes the heir. Then the king / queen message
  (0xB2 / 0xB4) except in a republic. When the heir came from outside the children the original may name a claimant
  faction (major, same religion, not at war, not a republic): effect UNKNOWN, not modelled.
- **Vacated posts** (CONFIRMED code, trigger INFERRED): the government (vtable 0x01354558, posts at +0xC4, type object
  at +0xCC) has per type a slot +0x10 "new minister for a post": absolute monarchy `0x008B7070` (built inline by
  `0x008B3B50`), constitutional `0x008B7340`, republic `0x008B75C0`. `0x008E05E0` (reached only through a table; it
  posts the leader-death messages 0x58 / 0x59 / 0x5A, `faction_leader_dies_*`) calls `0x008E4610`, which makes the new
  minister through slot +0x10 and puts him in the post (`0x009CB1E0`). For the leader's post (post index 0,
  `0x00655240`) of a monarchy: the succession, then a minister named after the successor (forename = his family name
  loc, surname empty) aged his age; for any other post, and a republic's leader: a new minister aged 21 +
  int_range(0, 30) (absolute monarchy) or 25 + int_range(0, 30) (the other two). `0x008BF310` fills every empty post
  the same way (new campaign `0x008CB820`, government change `0x008B3B50`, a republic's election `0x008BBFB0`, new
  governorships `0x008B5420`); `0x008BF280` keeps a pool of 5 spare ministers (the dismiss / appoint commands
  `0x008EB9D0` / `0x008F3760` draw from it). Vanilla saves: no living faction has a vacant post.
- **Model** (`ntw_sim::campaign::family`): `Family` loaded into `FactionDetails::family` and written back
  (`save::write_family`); `family_year` in the yearly pass (living faction = owns a region, INFERRED); `succession`;
  `CampaignModel::refill_post` from `character_dies` for every post the dead held (the leader exemption is gone; post
  holders now fall in battle too, only +0x52C characters escape). The writer: new ministers from a `minister` template,
  `CHARACTER` #8 and the post holders from the model, a new leader keeps the family name (the namer skips him), birth
  dates of new characters. PLACEHOLDER: new family members take the name and portrait of a filled member of the same
  sex (the original draws a royal or generic name, `0x008AA5E0`, and a portrait by age and sex, `0x008EF630`; those
  draws are not on the model's RNG); no messages; the spare-minister pool and government changes are not modelled.
  Tests: `family::tests` (6), `ntw_campaign/tests/characters.rs` (`families_load`,
  `a_dead_monarch_is_succeeded_and_posts_are_refilled`: Austria's emperor and a minister die, the first relative
  succeeds, the save has 0 violations and reads back the same family and holders). `ai_turns_save 26` (one year end):
  0 violations; one Prussian post refilled with a new minister (name drawn by the writer).

## 6. Per-character turn counters (CONFIRMED, modelled in round 7)
At the faction's turn end (`0x008BD0F0` → `0x009DA210` per character, before `CharacterTurnEnd`):
- +0x508 (`CHARACTER` #29) turns in enemy lands: + 1 when the region at his position (`0x00A1BDF0`) belongs to a faction
  at war with his (`0x008CE9B0`), else 0 (`0x00A28DD0`);
- +0x50C (#30) turns at home: + 1 when it belongs to his faction (`0x00898B20`), else 0 (`0x00A28E20`);
- +0x504 (#28) turns at sea: 0 for a land character (virtual +0x24) or one in a port (location state `0x009D3F20` = 2),
  else + 1 (`0x00A28D90`);
- +0x4C5 (#14) = he did not act (his action points equal the turn's start value, `0x00951540`), and then +0x4C8 (#15)
  idle turns + 1; +0x4FC (#25) = 0.

Field numbers CONFIRMED from the loader `0x00991520` (read order #14 bool +0x4C5, #15 u32 +0x4C8, #16 bool +0x4CC,
#17 f32 +0x2EC, #18..#21 u32, #22 bool +0x4E0, #23 bool +0x4E8, #24 u32 +0x4EC, #25 bool +0x4FC, #26 u32, #27 bool
+0x510, #28..#30 u32 +0x504 / +0x508 / +0x50C, ..., #34 bool +0x52C). The round-1 note had home and enemy lands the other
way round. Model: `CharacterDetails::turns_at_home` / `turns_in_enemy_lands` / `turns_at_sea` / `no_action` /
`idle_turns`, loaded, updated by `turn_end_counters` in the faction end step; the script conditions
`CharacterTurnsAtHome` / `AtSea` / `InEnemyLands` / `NoActionThisTurn` read them. PROVISIONAL: "in a port" is a naval
character within 1 unit of a port slot. Test: `campaign::tests::turn_end_counters_follow_the_exe`.

## 7. Agent actions (round 4, 2026-10-04)
- **Abilities and attributes** (CONFIRMED): the ability numbers are the order of the table at 0x0145D9B0
  (`can_assassinate` 0, `can_convert` 1, `can_build_religious` 2, `can_build_fort` 3, `can_sabotage` 4, `can_spy` 5,
  `can_duel` 6, `can_receive_duel` 7, `can_research` 8, `can_attack_land` 9, `can_attack_naval` 10,
  `can_sabotage_army` 11); the attribute numbers the order of the saved `AgentAttributes` (character +0x394: 0
  command_land .. 5 subterfuge .. 13 trade). Ability level (`0x009C7610`, character +0x3CC + 8 × n) = the saved level,
  plus, when it is not negative, its linked attribute floored at 0 (rake: `can_assassinate` 1 + subterfuge 2 = 3).
  Character types by +0x2C of the agent record (the name table 0x0145D9E0): 0 General, 1 admiral, 2 colonel, 3 captain,
  4 gentleman, 5 Eastern_Scholar, 6..10 missionaries, 11 rake, 12 assassin, 13 minister, 14 bandit, 15 pirate, 16
  guerilla. Rank (`0x00A198D0`): the type's main attribute (`agents` column 8) plus situation bonuses (generals: theatre
  and army make-up; admirals: fleet; missionaries: theatre zeal; ministers: their post), −1..9.
- **Chance table** (`0x00951F80`, per ability; against a character / a building / a settlement): assassinate
  `0x009225D0` / – / –; duel `0x00922940`; spy `0x00922850` / – / `0x00922A70`; sabotage – / `0x00922C90` /
  `0x00922DF0`; sabotage army `0x00922BA0`. All end clamped 5..95.
- **Assassination chance** (`0x009225D0`, CONFIRMED): not against colonels or captains, only by spy types (rake,
  assassin, guerilla). A = the agent's `can_assassinate` + his `subterfuge_assassination`, 0..9; T = the target's rank,
  0..9; C = the protector's rank (0 if the protector is the target) + his `subterfuge_counterspying`, 0..9, where the
  protector (`0x00A04910`) is the target's faction's character with the most subterfuge (> 0) among the target, his
  force / settlement and the spies of his region. Against a spy 76.92308 / (A + T + C) × A; against a gentleman,
  scholar or missionary 76.92308 / (A + T + C) × (A + 0.5); against a General A / (A + T + C + 1 + 0.1 × his army's
  units) × 100; others 0; then + the target's `security_versus_assassination`.
- **Duel chance** (`0x00922940`, CONFIRMED): the challenger needs `can_duel`, the target `can_duel` or
  `can_receive_duel`; with the weapon skills P (challenger) and Q (target), 0..9: 50 if both are 0, else P / (P + Q) ×
  100. The AI target picks the weapon worse for the challenger (`0x00AAAC30`, a coin flip on an RNG of its own at a tie).
- **The roll** (`0x00920B70` / `0x00920C60`, CONFIRMED): skill = the agent's attribute of the action (subterfuge for an
  assassination or a building sabotage, the weapon for a duel, the index 14 past the table, so −1, for an army
  sabotage); r = percent_0_100 on the campaign RNG; r ≤ chance: critical success (0) when r < 25 + 4 × skill, else
  success (1); r > chance: failure (2, detected and escapes) when r < trunc(100 − (100 − chance) × 0.15), else critical
  failure (3, detected and executed). Round 4 called 0 "success" and 1 "noticed"; the duel ending (0 = the critical
  success message) and the sabotage damage (more for 0) fix the names (round 5).
- **Diplomatic reactions** (round 5, CONFIRMED): the attitude factor setters `0x00B11ED0` .. `0x00B12010` set a factor's
  {value, drift, limit} and make it limited (`0x00B69640`) from the diplomacy config (world +0xFAC, built by
  `0x00B52070` over defaults written at start by `0x0042F110`; no `diplomacy_attitudes` table ships, so the defaults
  apply). Config triples {limit, drift, value} in key order (abandoned_ally_in_war (0, 2, −25), abused_military_access
  (0, 1, −30), alliance (80, 1, 30), ...); the agent ones: assassination (0, 2, −50), sabotage (0, 2, −20), spying (0,
  2, −20), third_party_assassination (0, 1, −5), third_party_sabotage (0, 1, −2), third_party_spying (0, 1, −2).
  Factors: 14 `assasination_attempt`, 18 `sabotage_attempt`, 19 `spying_attempt` (relationship +8 + 0x28 × index).
  - Detected attempt (outcomes 2 and 3; `0x00B0D870` assassination, `0x00B67FD0` sabotage): unless the victim is
    destroyed, the victim's attitude to the culprit gets the victim triple; every other faction (neither) gets the
    third-party triple towards the culprit.
  - Unseen critical success (outcome 0; `0x00B0D700`, `0x00B67E60`): the victim blames the faction it likes least
    (lowest attitude total `0x00B0DB60`, ties to the lower id, the culprit skipped unless it is the first looked at),
    which gets the victim triple; then every faction but the culprit and the blamed one gives the blamed one the
    third-party triple (the victim included, overwriting). Whose list is searched is INFERRED (the decompiled call
    loses the object). Outcome 1 has no reaction.
  - Detected agents are also added to the victim's `EXPOSED_CHARACTERS` (`0x008A8B30`, when their subterfuge ≥ 1 or a
    flag +0x4E0): modelled since round 6 (§10).
  - The "trait events" of round 4 are messages: `0x00A245A0` posts message 0x13D / 0x13F, `0x00A15A70` 246 / 248.
- **Duel ending** (round 5, CONFIRMED, `0x008BFD90`, slot 0 of the pending duel object at vtable 0x013572A0,
  "pending_duel"): the winner's +0x538 (`CHARACTER` #37, duels won) and the loser's +0x534 (#36, duels lost) go up by 1;
  the loser dies (kill reason 4 when the weapon flag +0x15 is set, i.e. pistols, else 7) on outcomes 0 and 3; on
  outcomes 1 and 2 he first tries to flee (`0x00A18460`, round 8, §12: 0xDA is the flight order's id, not a script
  event; +0x510 `CHARACTER` #27 is set, the order goes to his force or his region's settlement; issued → he lives
  wounded (+0x512, cleared on arrival by `0x0094EA00`), not issued → he dies). The world event +0x798 is the generic
  agent-action result event; its listener `0x00A14CF0` only builds messages (`0x009DC0C0` picks them by outcome and
  agent type: `duel_gent_versus_*_killed` / `_injured`). CONFIRMED.
- **Army sabotage** (`army_sabotage`, `0x0094DA30`, CONFIRMED): chance `0x00922BA0` = R / ((units + C) × 0.33 + R) ×
  100 with R the agent's rank and C the force commander's, 5..95; needs `can_sabotage_army`. Outcomes 0 and 1: the force
  is marked (+0xE8, `0x008EB660`); at its commander's next turn start his action points (and those of an army it
  carries) are set to 0 and the mark cleared (`0x008F2290` → `0x00A2A140` → LOCOMOTABLE +0x1C = 0); the mark is not
  saved (the army loader `0x00870FD0` clears it). A guerilla (type 16) also kills rank × 10 × the unit scale men: passes
  over the units in an order shuffled on the campaign RNG (k = 2..n: j = next16 % k, swapped with k − 1 unless j =
  k − 1); a unit at 10 % strength or more loses int_range(0, min(left, men / 2)). Outcome 0 also blames (sabotage
  triple); outcomes 2 and 3 are detected; 3 the agent dies.
- **Building sabotage** (`0x0094DEF0`, CONFIRMED): chance `0x00922C90`: S = rank + `subterfuge_sabotage` (0..9), C =
  the owner's best spy in the region: rank + `subterfuge_counterspying` (0..9), k from the chain record +0x18 =
  `building_chains` #2 parsed (`0x004F3720`; empty = 0, so most chains have k = 3; sArmy 1 and tFactory 2 have k = 1;
  other numbers 0.5); S / (round((level + 1) × k) + S + C) × 100, 5..95. Roll with ability 4 and subterfuge s.
  Outcome 0: the building's health × (1 − min(4 s + 63, 99.9) %), truncated (`0x00B452A0`), and blame; outcome 1:
  × (1 − min(4 s + 38, 99.9) %) and a call `0x0047FD40` (UNKNOWN); outcomes 2 and 3 detected, 3 the agent dies. A
  building below 100 health gives no effects.
- **Technology stealing** (in the research step `0x008DD450`, CONFIRMED structure): at a school that is still
  researching, the gentlemen standing there are grouped by faction (summed `research`, at most 12); each in turn draws
  next16 / 65535 on the campaign RNG against `0x008F32C0` = clamp((skill + 1) × 0.5 / √cost, 0.04, 0.9) (0.9 when the
  cost is 0): at or below it his faction gets the technology being researched there (`0x008CDCB0`) and the stealing
  stops for this step; above it he is thrown out (`0x00A25330`, message 246, moved to a free spot within 100,
  `0x00B372A0`). INFERRED: only foreign gentlemen take part.
- **Spying** (round 6: ported, see §10 for the outcomes; the chance below was the round-4 reading, §10 has the
  settlement / force one exactly): spy on a character `0x00922850` (S = rank + `subterfuge_spying`,
  counter-spy via `0x008C5FA0`; S × 100 / (T + S + C)), on a settlement or force `0x00922A70` (S / (S + f + C) × 100,
  f = 0.5 for a settlement or fort, half a force size, else 1; target roles INFERRED). The orders are
  `agent_join_force` (`0x00905650`, executed by `0x0094C500` with the character roll) and a settlement order
  (`0x00907040`, `0x0094CCB0` with the settlement roll); their effect is what the agent reveals (the shroud and
  `spying_data_level`, `0x009AA5E0`) and the network message `spy_network_established`: needs the visibility model
  (not in this slot). Sabotage of a settlement (`0x00922DF0`, from the defences and `0x00922F70`) is decoded in its
  chance only.
- **Conversion** (open): missionaries act through the region effect `conversion` (bonus 170) and the table
  `religion_conversion_mods`; the region religion breakdown does not change in the model at all, so conversion waits
  for the region religion model (0-B's area). `0x008C55D0` (gentlemen and, in the Peninsular campaign, missionaries
  adding public order: (rank − 1 + variable 103) / variable 103 + `gentleman_happiness_bonus`) belongs to public order.
- **Exchange** (`exchange`, `0x0094D660`): decoded in round 6 (§10): the unit exchange between two forces, not an
  agent action.
- **Model** (`ntw_sim::campaign::agents`): `assassination_chance`, `duel_chance`, `duel_weapon`,
  `army_sabotage_chance`, `building_sabotage_chance`, `steal_chance`, `roll` / `classify`, `protector`, `rank`,
  `ability`; `detected_misdeed` / `blamed_misdeed`; `guerilla_casualties`; `steal_step` (called from the research
  step). Commands `Assassinate`, `Duel`, `SabotageArmy { agent, force }`, `SabotageBuilding { agent, region, slot }`
  (the agent walks there and acts on arrival, once per turn: `World::agents_acted`); `World::sabotaged`; event
  `AgentActionResolved` (target `None` for a building). `CharacterDetails::duels_won` / `duels_lost` loaded from
  `CHARACTER` #37 / #36. PROVISIONAL: the duel spare check (never spares), exposure, messages, a human duel target
  uses the AI weapon rule, the protectors' region spies, the rank's situation bonuses, the guerilla unit scale (1.0),
  the thrown-out gentleman's spot (the region's settlement), the victim's relationship order for blame (target id
  order), a guard that ends the guerilla casualties when a pass kills nobody.
- **What the AI and the interface need** (other slots):
  - AI: choose targets and issue `Assassinate` (spies against generals, gentlemen, missionaries and spies; read
    `assassination_chance`), `Duel` (gentlemen; `duel_chance`), `SabotageArmy` (`army_sabotage_chance`),
    `SabotageBuilding` (`building_sabotage_chance`); move gentlemen into foreign schools (a `MoveCharacter` to the
    slot's position) to steal; `DismissMinister` / `AppointMinister` with `spare_ministers`. The original's AI choice
    logic is the CAI's (`0x00C34330` .. `0x00C351B0` read the chance table through `0x009225B0`), not decoded here.
  - Interface: target lists with chances (the original's popups `OpenAgentActionPopup` / `OpenDuelPopup` show "Chance"
    from the same functions), a weapon choice for a human duel target (today the AI rule decides), the "ChanceToSteal"
    display (`steal_chance`), minister swap / dismiss buttons, and result messages from `AgentActionResolved`
    (outcome → `spy_detected_escape` 246, `spy_detected_execute` 248, `spy_successful_sabotage` 252,
    `spy_successful_army_sabotage` 251, `duel_*`).
  - Save writer (request): `CHARACTER` #36 / #37 from `duels_lost` / `duels_won`; building health is already written.
- **Ministers** (round 4, `ntw_sim::campaign::family`): spare ministers = the faction's ministers without a post;
  every faction of the vanilla saves has exactly 5 (CONFIRMED, 27 factions). `CampaignCommand::DismissMinister`
  (`0x008EB9D0`, CONFIRMED): he leaves; an absolute monarchy (government slot +4 false) seats a random spare
  (int_range(0, spares − 1), `0x008E44F0`), the others make a new minister (slot +0x10); a constitutional monarchy also
  counts dismissals (slot +0x20: +0x14 += 1; reader UNKNOWN). `CampaignCommand::AppointMinister` (`0x008F3760` with the
  check `0x008B3500`, CONFIRMED): two holders swap; in an absolute monarchy a spare can replace a holder, who leaves; the
  others refuse spares. The list is topped up to 5 only on a new campaign, a government change and elections
  (`0x008BF280`). PROVISIONAL: the leader's post is left out of both commands (the interface's rule is not traced);
  with no spare a dismissal makes a new minister.
- Message type table at 0x0145C530 (88..90 `faction_leader_dies_*`, 178..180 `new_faction_leader_*`, 246..252 spy
  results).

## 8. Recruitment pool and new characters (CONFIRMED unless tagged)
- Saved per faction: `FACTION` #75 `CHARACTER_RECRUITMENT_MANAGER` {`GENERAL_RECRUITMENT` {u32[] candidate ids, u32},
  `ADMIRAL_RECRUITMENT` {same}}. The candidates are ordinary characters of the faction's `CHARACTER_ARRAY` (eur
  startpos: France 3 generals + 3 admirals, timer 0). Loaded into `FactionDetails::general_pool` / `admiral_pool`.
- Pool classes (vtables at 0x0136AE3C general / 0x0136AE84 admiral): cap = variable 90 `character_recruitment_pool_cap`
  (3, `0x00A17B50`); refill time (`0x00A17AF0`): variable 95 `..._refill_rate_general_2` (2 turns) if the faction has
  bonus `character_recruitment_general_refill_2` > 0, else 94 (`_1`, 3 turns) if `..._refill_1` > 0, else 93 (`_0`, 4
  turns); admirals the same with 96..98 and bonuses 122/123. (The variable indices are their order in the registration
  list at 0x00432DC0, array of name objects at 0x0164B550, 8 bytes each; read by `0x008B25F0` int / `0x008B25E0`
  float.)
- Per turn (`0x00A252B0`): when the timer equals the turns elapsed, one candidate is created (`0x009DB1B0`), and if the
  pool is still below the cap the timer restarts at turns elapsed + refill time.
- Recruiting a candidate (`0x00A1B8F0`): taken out of the pool; if the timer is idle it starts (turns elapsed + refill
  time); his +0x520 / +0x521 flags are cleared, so from then on he gains scripted traits and ancillaries (§1, §2).
  Cost (from `0x00A1BBE0`): a distance part min(10, round(10 × min(distance to the capital, max) / max)) × 100 with
  max = variable 99 `character_recruitment_max_distance` (1000); the base cost (400) and the per-star cost (300) are the
  other two variables (where they are added: open).
- Creating a character (`0x00A0DFC0`): a historical character for (faction, agent type) if one is due
  (`0x008C4BB0` → `0x0098F880`, from `export_historic_characters.lua` / `historical_characters`), else a generic one
  (`0x0098F250`): placed at the capital, age = 21 + min(19, ⌊next16 × 20 / 65535⌋) on the campaign RNG (`0x00A05740`,
  so 21..40, as every new character in the vanilla saves), birth year = current year − age (`0x00992E60`), +0x4EC =
  turns elapsed (the "appeared" turn the death check uses), name drawn by `0x009940A0` (the save slot's names work).
- The character's details object sits at character +0x2F0 (`lea ecx,[ebx+0x2f0]` before `0x00992E60`); its +0x19C
  container is therefore character +0x48C: the trait-level sets plus the ancillary sets (`0x009CDF10`). This makes the
  effects store's character set CONFIRMED (EFFECTS_FIDELITY §2 had it INFERRED).
- Round 7: the pools run (`ntw_sim::campaign::pool`):
  - per faction turn start (`pool_tick`; the pool slot `0x00A252B0` is reached through a table, phase PROVISIONAL):
    when the timer equals the turns elapsed, one candidate is created while below the cap, then the timer restarts
    at turns elapsed + refill time while below the cap;
  - a candidate is a generic General or admiral at the capital without a force, aged 21..40 on the campaign RNG
    (`create_candidate`). PROVISIONAL: historical characters due for the faction are not created; names and
    portraits are the save writer's (as for new ministers); the appeared turn (#24) is not kept.
    Portraits since 2026-10-10: drawn by the model when the candidate is made (§14);
  - `CampaignCommand::HireGeneral` (`0x00A1B8F0` → `0x00A164C0`): cost CONFIRMED (`0x00A1BB20` + `0x00A1BBE0`):
    `character_recruitment_base_cost` 400 + `character_recruitment_cost_per_command_star` 300 × rank + min(10,
    round(10 × min(distance to the capital, 1000) / 1000)) × 100; the candidate leaves the pool (timer starts if
    idle); a new army led by him with his culture's general unit (`agent_culture_details` #3 for `General`:
    `Gen_Generals_Staff` european, `Gen_Generals_Bodyguard` middle east, read through the agent record by
    `0x008E27D0`; the Peninsular guerrilla leaders get `Gen_British_Guerilla_Leader_Guerrilla`, not modelled);
    inside the capital when it has no garrison army (PROVISIONAL, as recruited units). Round 8 (§12): the original
    hires a General INTO an army (the new general's army is merged into the target force, `0x008D2FA0`), so
    `HireGeneral { into }` does that and the cost counts the army's distance; admirals are hired onto a fleet
    (`0x00A16110`, `HireAdmiral`), a captain in command going.
  - No vanilla save holds a hired general (no General with an appeared turn above 0 in the evidence), so the
    placement is checked against the code only. Test: `campaign::tests::the_general_pool_refills_and_hires`.
  - For the AI slot: `HireGeneral` with `hire_cost`; the candidates are `FactionDetails::general_pool.0`.
- Still open: the pending trait / ancillary lists of candidates (their gains are dropped, PROVISIONAL).

## 9. Death in battle and other kills (CONFIRMED structure)
`0x00A0C6F0(reason)` kills a character (death date + reason via `0x00A209F0`, then the object removes itself). Reasons
at its 44 call sites: 1 (killed with his unit or force: e.g. a settlement's garrison when it falls `0x00B58A30`, siege
attrition `0x00B796B0` that shrinks units each besieged turn by a random share up to min(30, (turns² − 1) × 1.5) % and
kills the character of a unit that falls below its minimum), 2 (`0x0094ACB0`), 3 (natural causes, §5), 5
(`0x009DAB40`), 8 (`0x008BB060`), and one variable. The model's battle code removes the character of every lost
unit, the commander included (his force then gets a successor, §5b), and every character of a destroyed force; +0x52C
characters escape (PROVISIONAL); a post holder who falls is replaced (§5c). Wounds (`campmap_*_wounded` / `convalesced`, Napoleon's return
to Paris): the +0x52C copy of §5b; its placing is not traced.

## 10. Sight, the shroud and hidden characters (round 6, 2026-10-04)
Model: `ntw_sim::campaign::visibility` (grid, cell sets, sight sources, shrouds), `agents::knows_character` /
`CampaignModel::expose` (hidden characters), `agents::spy_chance` / `spy` (spying); loader and quad-tree codec:
`ntw_campaign::shroud`. Tests: `ntw_campaign/tests/visibility_install.rs`, `campaign::tests`
(`hidden_characters_are_known_once_exposed`, `spying_rolls_and_a_detected_spy_is_exposed`).

**The grid and the cell sets (CONFIRMED).**
- The map is cut into sight cells (world +0xF58): 1024 × 512 on the European map, 512 × 512 on the Peninsular one
  (the quad trees' header), each 1.25 map units, the origin at minus half the extent. Checked on every settlement's
  saved sight box (129 in 4 vanilla saves).
- `QUAD_TREE_BIT_ARRAY` v1 = {u32 columns, u32 rows, u32 root size, `QUAD_TREE_BIT_ARRAY_NODE`}. A node is four
  children, in the order (x, z + half), (x + half, z + half), (x, z), (x + half, z), or a leaf {u32 low, u32 high}.
  A size-8 leaf is a 64-bit mask, bit 8 × row + column; a larger leaf is one uniform block (0 = empty, else full).
  `shroud::decode` / `encode` round-trip every tree of both start positions.
- A source at p with radius r sees a disc (`0x00B696C0`, matched exactly on 33 / 33 and 31 / 31 saved settlement
  trees). Take the cell box x0..x1 with x0 = floor(((p − origin) − r) × columns / extent), and x1 the same with + r;
  the same for z. Let w = x1 − x0 + 1 and h = z1 − z0 + 1. The cell at column c, row k of the box is seen when
  (k − h/2)² + (c − w/2)² ≤ w²/4, all in integer division (`SightGrid::disc`).

**Who sees what (`0x00B617E0` builds the union from `0x00BB2A00`'s sources; CONFIRMED structure).**
- The sources count for a faction when they belong to it or to its protectorates (`0x00B85040` / `0x00B85090`: the
  object's faction, or a faction whose patron, faction +0x754, is it). Which side is which is CONFIRMED by data: the
  only protectorate in the shipped start positions (egy_napoleon) has `egy_mamelukes` holding `protectorate` towards
  `egy_ottomans`, so a faction holding `Protectorate` towards F is F's protectorate (`sight_factions`). The co-op
  shared view (`0x008CA0B0` with faction +0x81C / +0x818) is not modelled (single player).
- **Characters.** Each character sees a disc of his sight radius, character +0x2EC, saved as `CHARACTER` #17 f32
  (CONFIRMED: the loader `0x00991520` reads #14 bool +0x4C5, #15 u32 +0x4C8, #16 bool +0x4CC, #17 float +0x2EC,
  #18..#21 u32, #22 bool +0x4E0, ..., #34 bool +0x52C).
  - The radius is his type's `agents` #2 (General 15, colonel 10, admiral 20, captain 15, rake 18, gentleman 10,
    minister 0, and so on).
  - When his own effect set is re-summed (`0x009CDF10` → `0x009CBC80`: after a trait or ancillary change, and at
    creation), the radius becomes base × (1 + `line_of_sight_extension` / 100) (bonus 132).
  - This update is skipped while a file loads (world +0xFC6, cleared at the end of `0x008742C0`). So a start
    position's characters keep their type's radius until their set changes.
  - Evidence: every #17 of the vanilla saves is its type's value except one French General at 22.5 (Grand Armee,
    +50).
  - Model: `World::sight_radius` (loaded), `update_sight_radius` on every trait, ancillary or creation change.
- **Settlements and slots** (`0x00B40C00`): radius 5.
- **Owned regions**: their saved `REGION` `LINE_OF_SIGHT` shape (`region_sight`; 45 regions in eur).
- **Trade route segments** (`0x00BCA310` → `0x00BD2BC0`). A segment is a source for a faction when one of the routes
  on it belongs to that faction:
  - its domestic routes (segment +0x44 list; saved as the u32[] four children before `LINE_OF_SIGHT` in
    `TRADE_SEGMENTS[]`, holding ids of `DOMESTIC_TRADE_ROUTES`),
  - or its international routes it exports (+0x54; the u32[] three before, holding ids of
    `INTERNATIONAL_TRADE_ROUTES`; a route's id is the u32 after its record).
  - Importers do not see the segment (tested: adding them changes nothing).
  - Model: `World::trade_sight`. An international route counts while its importer is still a trade partner.
    PROVISIONAL: the route lists are the loaded ones; routes built later add no segments.
- **Other queried kinds** (`0x00BB1D00`, `0x00BC6F00`): not identified; they add nothing in the saves tested.
- **Result.** Computed against the saved visible sets:
  - a turn-4 eur save: Russia, Prussia and France exact; Britain 33 of 29 219 cells missing; Austria 261 of 28 029
    missing (the missing cells lie on trade segments near Hanover and Brandenburg whose route lists do not name them:
    UNKNOWN);
  - the Peninsular turn-4 save: exact;
  - the start positions: 97..100 % covered, at most 0.4 % extra.

**The shroud (`FACTION` `CAMPAIGN_SHROUD` v1; loader `0x00AFBFC0`; faction +0x6F8).**
- It holds three quad trees and a bool: #0 explored (always a superset of #1), #1 visible now, #2 a hide set (empty in
  every save, UNKNOWN), and #3 the shroud is on.
- Which factions have one: the start positions give one to every playable faction (5 in eur, 4 in the Peninsular
  campaign). The only vanilla new game in the evidence (the Peninsular saves) keeps only the human's; the eur saves
  continue NapoleonRust files and keep all 5. INFERRED: a new campaign keeps only the human's (`set_human` drops the
  others). What creates or drops one is not traced (UNKNOWN; faction +0x6F8 is only zeroed in the constructors
  `0x008799D0` / `0x0087A190` / `0x0087BD70`). A faction without one sees everything; its AI keeps its own beliefs.
- Visible test (`0x00B7A150`): with the shroud on, the cell must be in #1 and not in #2.
- The original updates the area round a source when it changes (`0x00A27F10`, `0x00B790F0` → `0x00B1B8B0`, which
  ORs the sources of the box of the old and new sight into the visible tree, `0x00B617E0` with its clear flag 0,
  and the new shape into the explored tree); at a human faction's turn start it also re-applies the spy-network
  shapes (`0x008F2480` → `0x00B62350`, §12). Timing (round 8, CONFIRMED from the tree ops, §12): within a turn the
  visible set only grows; the faction's turn end (`0x008BD0F0` → `0x00B66EF0(0)`) clears it and rebuilds it from
  every source over the map. That is why the human's set equals the sources in the turn-start saves (`auto_nr4_t4`,
  `auto_after_c8`), holds 272 to 2 797 more cells in the mid-turn saves (`auto_nr1`, `auto_save_fr_t4`,
  `orig_over_nr4_0252`: moves earlier in the turn) and the AI factions hold up to 1 % more (changes after their
  turn end: Britain 33 of 29 219 cells, Austria 261 of 28 029). Explored grows with visible. Model: `refresh_shroud`
  additive at the turn start and after walks, the rebuild in `FactionEnd`.

**Hidden characters and the exposed list.**
- `CHARACTER` #22 (+0x4E0) is the hidden flag. `0x009D3000` sets it from the stealth test `0x009D1010` (CONFIRMED,
  ported as `CampaignModel::stealthy`), which is true when all of these hold:
  - the character is not in a settlement (`0x009D3CB0`) and his force is not carried (INFERRED for `0x00A0B8C0`,
    force +0x94);
  - he leads a force, and the world mode (world +0xF6C +0x58) is 0 or 8 (taken as passed: hidden armies exist in both
    vanilla campaigns);
  - either he has `campaign_map_stealth` (bonus 80) > 0, or every unit of the force can hide on the campaign map
    (`UNIT_RECORD` +0x135 = `unit_stats_land` #69, CONFIRMED by the record builder `0x00E8CFF0`: light cavalry,
    skirmishers, guerrillas), or he stands on hiding ground (`0x00A88EB0` looks up the ground type at his position:
    `regions.esf` `groundtypes` polygons, record +0x14 = `campaign_ground_types` #2, set for `light_forest`,
    `hilly_light_forest` and their `_cold_att` forms);
  - he does not have +0x510 (#27: he fled a duel, set by `0x00A18460`; false in every vanilla save; modelled as
    `CharacterDetails::fled`, round 8);
  - no character of another faction is in his force.

  Check: the rule gives every saved flag of the vanilla saves, 118 of 118 force commanders in `auto_nr4_t4`,
  `auto_after_c8`, `orig_fr_t1`, `orig_fr_may1811` (13 hidden, among them an Austrian colonel with one Hungarian
  hussar unit on grassland; `visibility_install.rs`). Rebel armies on hiding ground are not hidden: INFERRED, rebels are
  not re-evaluated (the model leaves them as loaded).
- When (CONFIRMED callers of `0x009D3000`): for every character when a campaign is created (`0x008B42F0`, forced;
  model: `start_campaign`), at the character's turn start (`0x00A24EB0`; model: `CharactersStart`), after a move
  (`0x00A285E0`; model: after every walk, for the mover and the army he carries), and when a force's units change
  (`0x008B4260`; model: PROVISIONAL, picked up at the next turn start or move).
- **Knows** (`0x008CE880`, CONFIRMED). A faction knows its own characters. It knows a character with `subterfuge`
  ≥ 1, or one flagged hidden, only once it has exposed him. It knows anyone else. (A human-ally case, faction +0x814
  set and +0x81C clear, is not modelled.)
- **Expose** (`0x008A8B30`, CONFIRMED). A character with `subterfuge` ≥ 1 is pushed onto the faction's list (+0x80C /
  +0x810, `FACTION` `EXPOSED_CHARACTERS[]` = {i32 id}) without a duplicate check. A hidden one is added only if not
  there. Anyone else is not added.
  - France holds one hidden Spanish General in the Peninsular turn-4 save.
  - Callers: assassination and army sabotage on detection (failure and critical failure), spying (§ below), and the
    spotting pass `0x008B4B70`. Building sabotage is not a caller.
  - Model: `FactionDetails::exposed` (loaded). A dead character is taken off every list (INFERRED: the loader re-links
    the ids, `0x008E07F0`).
- **Spotting pass** (`0x008B4B70`, from `0x008F2480` at the faction's turn start; CONFIRMED, ported as
  `CampaignModel::spotting_pass`, run in `CharactersStart` after the hidden flags):
  - Spotters: the faction's characters not in a settlement with `subterfuge`, `research` or `zeal` > 0 that are
    missionaries (agent types 6..10, `0x00F9C710`), or carry +0x4C5 (`CHARACTER` #14, false in every vanilla save, not
    modelled). In practice missionaries spot.
  - Candidates: the other factions' characters inside the spotter's sight (his saved shape when he has one, else the
    circle of his radius; the model uses the circle, PROVISIONAL at the rim).
  - Score: his rank; +2 if his `subterfuge` is exactly 0 and he is a missionary, +1 if exactly 0 otherwise; +4 if the
    target stands in a region the faction owns (`0x00A1BDF0`); clamped 1..9.
  - Threshold against a target the faction does not know: (score − the target's rank + 9) × 5 against a subterfuge
    target; against a hidden one 90 if the spotter has subterfuge, else 25; others are skipped.
  - Draw: `percent_0_100` on the campaign RNG (world +0xFB8, rejection below 88). At or above the threshold the
    target, unless in a settlement, is exposed (message 246 and event 0x13D are not modelled).
  - Order: the original takes spotters in the faction's character list and targets in the spatial query's order; the
    model takes both in id order (PROVISIONAL: the RNG draws can differ in order).
- **Pathfinding** (`0x00B3F4F0`, CONFIRMED). An obstacle is hidden (mode 5, not cut) for the searching faction when
  either:
  - the faction has a shroud and the obstacle's place is not visible (`0x00B6A920`), or
  - the obstacle's character is not known to it (obstacle slot 9 `0x00B4D600` → `0x008CE880`; own obstacles never).

  Model: `plan_path` leaves such obstacles out (`zoc` module docs).

**Spying (`0x0094C500` force, `0x0094CCB0` settlement; ported as the `Spy` command).**
- Chance (`0x00922A70`, CONFIRMED):
  - S = rank + `subterfuge_spying` (bonus 97), 0..9.
  - C = the target side's best spy (`0x00B417E0`): rank + `subterfuge_counterspying` (bonus 99), 0..9.
  - f = 0.5 for a settlement or fort, half the unit count (force +0x1A8) for a force.
  - The chance is S / (S + f + C) × 100, truncated, 5..95; it is 5 when S is 0.
  - PROVISIONAL: C uses the building protector for a settlement and 0 for a force.
- Roll: `subterfuge` (attribute 5).
- Outcomes:
  - critical success: the spy's faction exposes every foreign character at the target it does not know (`0x00B54A70`
    settlement list +0xD8, `0x008D2BE0` force list +0x70), then as a success;
  - success: script event 0x144, faction counter 37;
  - failure: message 246, the victim's reaction (`spying` config), and the victim exposes the spy;
  - critical failure: the same with message 248 and counter 36, and the spy is executed;
  - always: +0x4CC (`CHARACTER` #16) is set, the agent has acted.
- What the spy reveals is his own sight from where he stands; spying adds no sight source of its own.
- PROVISIONAL: the spy staying inside, the event, the counters, the messages, and the settlement order's cheat flag
  (world +0xFA8 +0xEC forces success).

**Exchange (`exchange`, `0x0094D660`, decoded; not an agent action).**
- It is the order that exchanges units between two forces (or a force and a garrison). With its merge flag (+0x58) and
  `0x008B2720` true, every unit of one goes to the other, then `0x008D7310` applies the exchange list (+0x18; also
  used by the exchange panels `0x00A75DB0` / `0x00A78220`). Otherwise an order is queued (`0x00885F30` / `0x00885F60`).
- `0x008B2720`, the can-exchange check:
  - not an army with a navy;
  - two armies or two navies, both not in a settlement;
  - the receiver has room: a human needs giver + receiver units ≤ the receiver's maximum; the AI needs receiver <
    maximum;
  - and the two counts (`0x006A3110`) sum to at most 20.
- It belongs to the forces area: `MergeForces` covers the merge case. A split or exchange command is open (not this
  slot).

**Writer requests (save worker; nothing breaks without them).**
- `FACTION` `CAMPAIGN_SHROUD`: trees #0 / #1 from `World::shrouds` (`ntw_campaign::shroud::encode`, root =
  `sight_grid.root`); #2 and #3 as loaded; no record for a faction without a shroud (as the vanilla Peninsular
  saves).

- `CHARACTER` #17 from `World::sight_radius`; #22 from `CharacterDetails::hidden`.
- `FACTION` `EXPOSED_CHARACTERS` from `FactionDetails::exposed`.
- Optionally each character's `LINE_OF_SIGHT` #13 box and tree from his position and radius (`SightGrid::disc`): the
  original keeps them in step with the shroud.

## 11. Closing summary (0-G, 2026-10-04; residuals closed in round 8, §12)
The characters and agents slot is complete. Everything that drives play is decoded from the exe and ported with
tests. Round 8 (§12) closed the residual table below: every row is CONFIRMED and ported, or marked with the precise
reason it cannot be observed and the tested behaviour in its place.

**Done (CONFIRMED and modelled; section):**
- Traits and ancillaries gained in play, the binding RNGs, 31 character conditions, and the events that fire the
  trigger scripts (§1–§4): `CharacterTurnEnd`, `CharacterCompletedBattle`, `BuildingCompleted`, `ResearchCompleted`,
  `CharacterCreated`, `CharacterPromoted` and others.
- Ageing, natural death, ancillary expiry (§5); commander succession, the royal family, leader succession, vacated
  posts (§5b, §5c); turn-end counters (§6).
- Agent actions with their chances, rolls and outcomes (§7): assassination, duel, army and building sabotage,
  spying, technology stealing; diplomatic reactions; minister dismissal and appointment.
- Recruitment pools and hiring generals (§8); death in battle (§9).
- Sight (§10): the grid, the sources and the shroud; hidden characters, the exposed lists, the stealth test and the
  spotting pass; the pathfinder's hidden obstacles.
- Conversion: 0-B's region religion model (missionaries' rank); there is no conversion order.
- Exchange: decoded as a forces order; its room check is in §10 for 0-B.

**Residual table (state after round 8; the evidence is in §12):**

| item | state | evidence | behaviour in place |
|---|---|---|---|
| shroud update timing | CONFIRMED | `0x00B617E0` clear flag 0 at every box update; the turn end `0x008BD0F0` → `0x00B66EF0(0)` rebuilds | visible only grows in the turn, rebuilt at the faction's turn end (tested) |
| queued reveal areas at turn start (`0x00B62350`) | CONFIRMED | the human's spy-network shapes (`0x008F9A00` ← `0x008B4120`) | `spy_network_step`, network discs as sources, `CharacterBuildsSpyNetwork` (tested) |
| trade-route sight for routes built later | observable only with a route built in play (none in the evidence; the trade area owns route creation) | segment lists only in the file | loaded lists kept |
| stealth: +0x510, world mode | +0x510 CONFIRMED (a duel loser who fled); world mode: unobservable (constant 0/8 in every save and in play) | `0x00A18460` sets +0x510; `0x009D1010` refuses it | `fled` refuses hiding (tested); mode taken as passed |
| hidden flag on force changes (`0x008B4260`) | CONFIRMED caller | the unit-change observer | refreshed at the next turn start or move (the same test applies) |
| spotting order of draws | unobservable without the original's list order (file order of characters; the model's id order equals it for loaded files) | — | id order |
| spying: force protector, staying inside, cheat flag | cheat flag CONFIRMED (`EPISODIC_RESTRICTIONS` #22 via `0x0097A8F0`, human agents only); the force protector of `0x00B417E0` and the spy's stay are the AI / forces areas' (the order's +0x48 target) | §12 | the flag is read from the save and honoured (tested), C = 0 |
| historical candidates, hiring admirals | CONFIRMED | `0x0089B830` / `0x008C9580` / `0x008C4BB0`; `0x00A16110`; the `HISTORICAL_CHARACTER_MANAGER` list (18 keys in the Peninsular saves) | due rows offered first by `uniform_below` on the campaign RNG; `HireAdmiral` onto a fleet (tested) |
| pool step phase | CONFIRMED | `0x008F2620` → `0x00A25310` on faction +0x940 after the spotting pass | `CharactersStart` after the spotting pass |
| `CharacterPromoted` trigger point | CONFIRMED | `0x00A1A2E0` (the field promotion of a unit's commander), not the hire | `PromoteUnit` fires it; hiring does not (tested); the promotion cost PROVISIONAL (see below) |
| +0x52C characters rebuilt when killed | CONFIRMED | `0x0099D2D0` → `0x0098FE60`, placed where he stood, linked to his force | he stays where he fell, counters reset (tested); riding along not kept (no riders in the model) |
| duel "spare" | CONFIRMED: the loser flees | `0x00A18460`: flag +0x510, a flight order to his force or the region's settlement; dies when none | ordinary outcomes: flight, `fled`; critical ones: death (tested) |
| commander order keys | unobservable: the unit raise date and serial are not saved by the original | `0x0088C710` copies the world date into the unit at creation only | id order |
| female faction leaders | CONFIRMED | `0x008A18A0`: leader and the family's leader member not male | `IsFactionLeaderFemale` from `FAMILY` #1 |
| leader's post in the minister commands | interface rule (UI area; the model refuses nothing) | — | left out |
| messages and UI events of agent actions | UI area | message ids listed in §12 | script events fired (§12) |
| trigger conditions | 31 of ~80 | the battle ones need battle facts (battle area) | unimplemented ones return 0 / false |

**Writer (done in round 8 for the characters' fields):** `CHARACTER` #14 / #15 / #27 / #28 / #29 / #30 from the
details, #22 hidden, #36 / #37 duels, #17 sight radius, the pools. Still with the save worker: #24 = turns elapsed for
characters created in play; `CAMPAIGN_SHROUD` from `World::shrouds`; `EXPOSED_CHARACTERS`; the
`HISTORICAL_CHARACTER_MANAGER` list from `World::historical_created`; names and portraits of historical candidates
from `CharacterDetails::historical_key` (`historical_characters_on_screen_name_<key>`).

**Hooks for other slots:**
- AI: `HireGeneral { into }` / `hire_cost_into`, `HireAdmiral`, `PromoteUnit`; the `Spy` command with
  `spy_chance`; `knows_character` and `sees` for target lists. The agent-order executors of the original run only
  for a human agent (faction +0x6E0, CONFIRMED in `0x0094C500` / `0x0094CCB0`); how the CAI's agents act is the AI
  area's.
- UI: the same, plus the shroud's `explored` and `visible` sets for the fog layer; the messages of §12.

## 12. Round 8 (2026-10-04): the residuals closed

Worker: `characters-ai6` on `NR-characters`, Ghidra copy `NR-spt-ghidra` with `SaveDecomp.java` and a copy
`CharDecomp.java` (adds `evt:` = the fire sites of a script event from its name stub) in `target/tmp/gh/scripts`.
Every address is a static VA of Napoleon.exe 1.3.0.0.

**The shroud (CONFIRMED).** The shroud object (`FACTION` `CAMPAIGN_SHROUD`, faction +0x6F8; constructor
`0x00AE89A0`, loader `0x00AFBFC0`) holds three quad trees of {cols, rows, root, node} at +4 (#0 explored), +0x1C (#1
visible) and +0x34 (#2 hide), the faction id at +0x4C and the on flag at +0x50. `0x00B617E0(box, clear)` collects the
sources in the box (`0x00BB2A00`: characters' and forces' +0x27C shapes, settlements' +0x200, slots', regions', the
trade segments, and the faction sight object's +0x20 shape list) and ORs each into the visible tree (`0x00E0D750`,
listing: `lea ecx,[esi+0x1c]`); with `clear` it first clears the box's cells (`0x00E143F0` → `0x00E14420`) — and
every caller passes 0 (the box update `0x00B1B8B0` from a move `0x00A27F10`, a force change `0x00B790F0` and the
character destructor `0x0099D2D0`; the reveal `0x00B62350`; `0x00B73520`; `0x00B78B20`). `0x00B1B8B0` also ORs the
moved source's new shape into the explored tree (`lea ecx,[ebx+4]` before `0x00E0D720`). The whole visible tree is
cleared and rebuilt from every source over the map only by `0x00B66EF0` — called last in the faction turn end
`0x008BD0F0` (after the turn-end counters `0x009DA210`, the regions' `0x00A786C0` and `FactionTurnEnd`; skipped for a
faction sharing a human ally's view, +0x81C) and by the setups `0x008DD090` (the player record), `0x008BD4F0` (the
share undone) — and by `0x00B67150` (`0x008EE3E0`, sharing a human's view). So within a turn the visible set only
grows; the AI saves' ≤ 1 % extra of round 7 are moves after their last turn end. Model: `refresh_shroud(f, true)` in
`TurnStep::FactionEnd`, `refresh_shroud(f, false)` at the turn start and after walks. Test:
`campaign::tests::the_visible_set_grows_during_the_turn_and_is_rebuilt_at_the_turn_end`.

**The queued reveal areas = the spy network (CONFIRMED).** Faction +0x800 is the faction's sight object (its +0 the
faction, +0x20 a vector of 0x2C-byte sight shapes: {box f32×4, a tree, a flag}). At a human faction's turn start
(`0x008F2620`: `if faction+0x800: human ? 0x008F2480 : 0x008B4B70`) `0x008F2480` copies the vector, empties it,
reveals each old shape (`0x00B62350`: OR into explored, then the box's sources into visible), runs `0x008E5120` and
`0x008F9530`: for every character passing `0x008B4120` — with a locomotable (`0x009FB9D0`), one check of it
(`0x00898B80`, not decoded), attribute 5 `subterfuge` > 0 and +0x4C8 (`CHARACTER` #15 idle turns) > 2 — `0x008F9A00`
builds his current sight shape (vtable +0x34), pushes it (`0x008E3D90`) and reveals it; when +0x4C8 == 3 it posts
message 0xFA `spy_network_established` (`0x00A24670`) and counts stat 0x29. The vector is read as a source list by
`0x00B617E0` (`0x00AE5F40` copies faction +0x800 +0x20). Corrections to §6: `0x009DA210` sets +0x4C5 = "did not
act" and then +0x4C8 += 1 (idle turns); `0x00A28B50` (a move starts) clears +0x4C8 and the hidden flag; `0x009D3000`
with a non-zero second argument sets +0x4C8 to it. Model: `World::network_sight` (per human faction), filled by
`spy_network_step` in `CharactersStart`, read by `compute_visible`; the event `CharacterBuildsSpyNetwork` (its
class vtable `0x01355BD0`, built at `0x008F9A91`). Test: `a_spy_network_is_established_after_three_idle_turns`.

**The pool step's phase (CONFIRMED).** The recruitment manager is faction +0x940 (`0x0087B850` in the faction
constructor, loader `0x00994F00`: two pointers, the general pool (vtable `0x0136AE3C`) and the admiral pool
(`0x0136AE84`), both 0x24 bytes: +4 faction, +8 agent record, +0xC.. the candidate list, +0x20 timer; their shared
base vtable `0x0136ADF4`). Its turn step `0x00A25310` calls slot 12 (`0x00A252B0`) of both and is called from the
faction turn start `0x008F2620` after the characters' turn starts (`0x00A24EB0` per character), the forces'
(`0x00AAE820`), `0x00A18A70`, `0x00A24C90`, `0x008F2E10`, `0x008DA210`, the spotting / network step and
`0x009653D0` per character. Model unchanged (`CharactersStart` after the spotting pass).

**Historical candidates (CONFIRMED).** `0x008C4BB0` (this = the model's `HISTORICAL_CHARACTER_MANAGER`, +8/+0xC a
sorted list of created keys) builds a collector context for (faction, agent type) (`0x0087F720`), fires the script
event `HistoricalCharacters` (`0x0087F750`, class vtable `0x01357998`; `export_historic_characters.lua` has 505
handlers, 288 `eur_`, 157 `spa_`, 36 `ita_`, 24 `egy_`), whose handlers ask `CanGenerateHistoricalCharacter(key)`
(`0x0089B830`: the `historical_characters` row's agent type equals the pool's, its faction key the pool's, the
current date lies in the row's #5..#6 years, and `0x008C9700` does not find the key in the created list; in the
Peninsular campaign with a global flag a region test on #7 is added, UNKNOWN use) and offer the row with
`historical_character(key)` (`0x008C9580`, a push). One of the offered rows is picked by `uniform_below(count)`
(`0x005F0830`, the LCG 0x343FD / 0x269EC3 with 16-bit output) on the campaign RNG (world +0xFB8), its key is
inserted into the created list (`0x0086DCA0`, binary search, saved as
`CAMPAIGN_MODEL/HISTORICAL_CHARACTER_MANAGER/CREATED_CHARACTER_ARRAY[]` {utf16}: 18 keys in both Peninsular saves,
turn 1 and turn 4, so the start's candidates came from the lists and no refill happened by turn 4), and
`0x0098F880` makes the character from the record. The table (`sbsssiis`): key, flag, gender (`m` / `f`; e.g.
`spa_antonia_henriques` is a woman), agent type, faction, year from, year to, note. Model: `ntw_data`
`HistoricalCharacter`, `CampaignRules::historical`, `World::historical_created` (loaded), `due_historical` and
`create_candidate` (`CharacterDetails::historical_key` keeps the key for the writer's names; the age is drawn as a
generic character's, PROVISIONAL: the record has no birth year). Test:
`a_due_historical_character_is_offered_before_a_generic_one`.

**Hiring (CONFIRMED structure).** `0x00A1B8F0` (pool slot 27: the candidate check, `0x008B0210` takes him off the
list, a human pays `0x00BAF500(cost, 2)`, slot 17 places him, the timer restarts if idle, +0x520 / +0x521 cleared).
Generals (`0x00A164C0`): a new army with the culture's general unit (`0x0087FB30` with `0x008E27D0`), merged into the
target force (`0x008D2FA0`) at its position; the candidate was moved there first, so the distance part of the cost is
the target army's distance from the capital. Admirals (`0x00A16110`): the candidate moves to the target navy
(`0x00963840`), takes command (`0x008EDE20`, `0x008D2FA0`); a commander already there is removed (`0x00A0C6F0(1)`).
Stat counters 0x19 / 0x1C (`0x008CA100` adds to faction +0x96C[id], humans only; not events). Model:
`HireGeneral { into }` (without a target: the capital, PROVISIONAL), `HireAdmiral { fleet }`, `hire_cost_into`.
Tests: `a_general_hired_into_an_army_takes_its_command_there`, `an_admiral_is_hired_onto_a_fleet_and_its_captain_goes`.

**`CharacterPromoted` (CONFIRMED).** The event class vtable `0x0136A97C` is built in exactly one place:
`0x00A1A300` (this = the character details), which copies an agent record's attributes and abilities into the
details, sets +0x1AC, fires the event, looks the record up in a table and re-sums the effects (`0x009CDF10`);
`0x00A1A2E0` calls it and stores the record. Its callers are the unit classes' slot 20 (`0x008E1C20` land, vtable
`0x01355494` = `0x0135536C` + 0x128; `0x008E2260` naval, vtable `0x01355528`): the field promotion of a unit's
commander to General (agent record 0) / admiral (record 1), making a character for a unit without one
(`0x00990EF0`) and a new force when needed (`0x0087FEA0`). Ways in: the player's `PromoteUnits` (`0x009EF360`,
`CanPromoteUnit` `0x009E0AF0` asks the unit's slot 16) / `CCQ_PROMOTE_COMMANDER` (`0x00936C70` → slot 0x4C of the
selected unit), the console `promote_unit_commander` (`0x00961430`), the AI's `CAI_BDI_PROMOTE_UNIT`. The effects
`promote_general_in_field` (bonus 64) / `promote_admiral_at_sea` (65) exist for it (INFERRED gate). Hiring from the
pool does not fire it (the round-7 reading was wrong). The interface shows a `PromotionCost` (unit UI slot +0x44;
sandbox static trace 2026-10-04, Ghidra read-only, notes in our words, output in
`sandbox target/tmp/gh/promotion_cost*.txt`, not committed: both executors charge that slot's value first
through the treasury spend `0x00BAF500(cost, 2)` — the same spend the UI builder `0x009ABE00` reads the slot
for — while hiring (pool slot 27 `0x00A1B8F0`) builds its cost through pool slot 15 `0x00A1BB20` +
distance slot 11 `0x00A1BBE0` (base + per-star × rank + distance part) and pays it the same way, so the
two costs come from different sources; both paths end with a second spend through `0x008F35E0` against
field +0x6c, read through `0x0047B940`, which is not the treasury move. Land slot +0x44 is `0x008E2770`:
a static table value through `0x008E27D0` (or −1 with no record), with no rank or distance input;
naval slot +0x44 is a return-0 stub; the base class tables hold a return-0 stub at slot +0x40, so the
gate's concrete value for a promotable unit is still UNKNOWN. PROVISIONAL = the hire formula stands
until the probe runs: `target/tmp/probes/promotion_probe.cdb.txt` logs the slot value, the gate
result, both spends and the hire comparison). Model: `PromoteUnit { force, unit }`. Test:
`promotion_in_the_field_makes_the_colonel_a_general_and_fires_character_promoted`.

**The duel ending (CONFIRMED; the "spare" was a flight).** `0x008BFD90` (this = the pending duel: +8 challenger,
+0x10 target, +0x15 pistols, +0x18 outcome): outcomes 0 and 3 kill the loser (`0x00A0C6F0(reason)` inside a
settlement, else `0x00A0C720` + `0x00A0C670`; reason 4 pistols, 7 swords; message 0x45 `duel_critical_success`).
Outcomes 1 and 2 call `0x00A18460(winner)` on the loser: it sets +0x510 (`CHARACTER` #27) and issues a flight order
(`0x0094EE20` → `0x0091B950`, an order object with vtable `0x0135F1DC` whose executor `0x0095D080` →
`0x0091B790` plans the move) towards his force (`0x009FB9A0`, its position) or, without one, the settlement of the
region he stands in (`0x00A17CF0` +0x70); the order is accepted when `0x00921160` allows it and a path exists. If
it was issued the loser lives, +0x512 set, message 0x47 `duel_success` (texts `duel_gent_versus_*_injured`); on
arrival the order's slot 1 (`0x0094EA00`) clears +0x512 with message 0x46 `duel_failure`. Otherwise he dies
(`_killed`). `DuelFought` is then fired for both (`0x00934C60`, vtable `0x01361BF8`); stat 0x22 / 0x23. The message
table at `0x0145C530` is an array of string pointers (0x44 `duel_critical_failure`, 0x45..0x47 as above, 0xFA
`spy_network_established`). Model: `flee_duel` (walks him at once, `fled` kept, `wounded` cleared on arrival;
PROVISIONAL: the walk is immediate). Test: `a_duel_counts_and_the_loser_dies_or_flees`.

**The +0x52C copy (CONFIRMED).** In the destructor `0x0099D2D0`, after the wounded message (`0x00A0B980` →
`0x009D2240`) and the sight box update, when the force has no units left its characters are killed and it goes;
then for +0x52C a 0x550-byte copy is built (`0x0098FE60`: faction, the agent record, +0x2EC sight radius, the
details object with traits and ancillaries (`0x00992C80`), +0x4EC appeared turn, +0x52C = 1, +0x530 = 2, the duel
counters, +0x53C, +0x540, +0x54C; +0x4C8 idle turns, +0x4E0 hidden and +0x512 wound cleared), linked to the force
when it exists (`0x008B02E0`, `0x008CF9D0`: as a rider, not the commander — the observers then pick the successor)
and placed where he stood (`0x009D3B60(0, old)`: enters the world, type radius, a move update). Model:
`character_falls` resets the counters and keeps him where he fell (riders are not modelled). Test:
`a_returning_commander_who_falls_is_reset_and_stays_where_he_fell`.

**Female leaders (CONFIRMED).** `IsFactionLeaderFemale` (handler `0x008A18A0`): the character is the faction
leader (`0x008CF600`) and the family's leader member (FAMILY +4, byte +0x19 "male" = family +0x1D) is not male.
Model: `ntw_script::characters` reads `FamilyMember::male` of member 0.

**The cheat flags (CONFIRMED).** `CAMPAIGN_MODEL/EPISODIC_RESTRICTIONS` (loader `0x009968A0`, world +0xFA8) #20
(+0xEA) and #22 (+0xEC) are set by the episodic scripting commands `force_assassination_success_for_human`
(`0x0097A620`) and `force_garrison_infiltration_success_for_human` (`0x0097A8F0`); `0x0094ACB0` / `0x0094CCB0` use
them only when the agent's faction is human (+0x6E0): outcome 1 without a roll. Both false in every vanilla save
(`orig_fr_t1`: #17..#22 = false, true, true, false, false, false). Model: `World::force_success_for_human`,
`forced_or_roll`.

**The agent script events (CONFIRMED fire sites, `evt:`):** `SufferSpyingAttempt` (`0x0094C540`, the watched
force's commander, before the roll), `SpyingAttemptSuccess` (`0x0094C5A6`), `CharacterFactionSpyAttemptSuccessful`
(`0x0094C601` / `0x0094CD8E`, the spy's faction leader), `CharacterFactionSuffersSuccessfulSpyAttempt`
(`0x0094C65C` / `0x0094CDDD`, the victim's leader), `EspionageAgentApprehended` (`0x0094C79D` / `0x0094C86C`,
outcomes 2 / 3; none in the settlement order), `SufferAssassinationAttempt` (`0x0094AE19`, before the roll),
`AssassinationAttemptSuccess` (`0x0094B226`, outcomes 0 / 1), `CharacterCriticallyFailsAssassination`
(`0x0094B449`), `SabotageAttemptSuccess` (`0x0094E51D`, building, outcomes 0 / 1), `ArmySabotageAttemptSuccess`
(`0x0094DCC2`) / `HarassmentAttemptSuccess` (`0x0094DCB8`, a guerilla), `DuelFought` (above),
`CharacterBuildsSpyNetwork` (above). The event registrar `0x00DF2F40` is a stub (the registration thunks at
`0x00419C90`.. only document name, description and author); an event is an object whose vtable slot 0 is the name
getter (16-byte stubs at `0x008BDC00`..), dispatched by `0x008DBAC0` (world +0x9A8's slot 2) to the listeners.
All fired by the model now (`events.rs`), with the character as the script context.

**Not CONFIRMED in this round (reasons):** the promotion cost (probe written, see below); the trade-route sight of
routes built in play and the spotting draw order (unobservable, table); the world mode of the stealth test
(constant); the leader's post in the minister commands and the agent messages (UI area).

**0-G, 2026-10-05 (branch `work/next/0g-characters`, Ghidra read-only on `NR-sb-ghidra-0g`, kept in
the ignored `target/tmp/gh/promo_cost.txt`, no decompiled code in the repo).** The promotion cost, one
more step: the land class's slot +0x44 is `0x008E2770` = "the class's own getter (`+0x30` →
`0x008D9BB0`) then `0x008E27D0(agent type, culture, 0)` → **`record->(+0x38)`**, `-1` with no record";
the naval class's `+0x38` / `+0x3c` pair is `0x008E27B0` / `0x008E2A60` with the same shape, and its
`+0x44` is a return-0 stub (a naval promotion is free). So the price is **one static unit-record field,
with no rank and no distance input** — the hire formula is structurally wrong, and which table the
record belongs to is still UNKNOWN (`0x008E27D0`'s normal path returns `object->(+0x2C)` out of a hash
table, `0x00F9C2A0`, keyed by a string built from the "General" name table `0x0145D9E0`). If it is the
`units` record, the field is column #7 `unknown_3c` (file `@0x3C` = record `+0x38` under the -4 shift
this file documents for the recruitment time; values 350 / 470 for the two culture general units,
1020-2270 for ships). **The vanilla save fixtures cannot settle it** — the price is read from the DB
at run time and no save field carries it (checked the save schema and `save_check.rs`), so there is
nothing for a fixture to disagree with; the `promotion_probe.cdb.txt` probe remains the cheapest way
to see the number. The UI side is now in place (`CampaignUI.CanPromoteUnit`, the unit rows'
`PromotionCost`, `CampaignModel::can_promote_unit` / `promotion_cost`, all with tests), and the
original's hire entry point is `CampaignUI.PromoteUnits(force, candidate)` — read on the install from
`enlist_commander.lua:106`, so §12's "player `PromoteUnits` `0x009EF360` → selected unit slot 0x4C"
reading needs a re-look (the script's second argument is a pool candidate, not a unit slot).
Details: `CHARACTER_UI_HOOKS.md` H1/H2.

**0-G, 2026-10-05 round 2 (`work/next/0g-characters` rebased on `sandbox/next`; Ghidra read-only on
`NR-sb-ghidra-0g-r2`, kept in the ignored `target/tmp/gh/promo_r2.txt` and `promo_raw_r2.txt`; the
probe is tracked in `analysis/fidelity/debugger/0g_promotion_probe.cdb.txt`).** The promotion price,
step three. Three things closed, one left open.

- **The record is not read straight off a `units` row (INFERRED since the 2026-10-06 review: static trace, decompile not kept; and the round-1 wording corrected).**
  `0x008E2770` reads **`*(record->(+0x0C) + 0x38)`** — one dereference more than round 1 recorded —
  so the record is a wrapper whose `+0x0C` points at the row, not the row itself. `0x008E27D0` has
  two branches and only the first touches a unit table: the guerrilla special case
  (`param_3 != 0` and `spa_napoleon` enabled) looks `Gen_British_Guerilla_Leader_Guerrilla` up in
  `DATABASE_TABLE<EMPIREUTILITY::LAND_UNIT_RECORD>` and returns that row. Its **normal** path builds
  the key from the agent-type name table `0x0145D9E0`, looks it up in
  `DATABASE_TABLE<EMPIREUTILITY::AGENT_RECORD>` — the **`agents`** file, 12 columns, row reader
  `0x00F94940`, `AGENT_RECORD` #0 being the `CHARACTER` #3 type string (`General`, `admiral`,
  `colonel`, `captain`, `gentleman`, `Eastern_Scholar`, `missionary`, …) — and uses that row only
  as a **guard** (absent, or `this+0x514 != 0` via the 11-byte `0x008CEEF0`, and the lookup returns
  0). What it returns instead is `0x00F9C2A0`, the value of a runtime string hash keyed by that
  agent-type name plus `*(*(world_singleton+0x10)+0x10)` — `0x008F3490` is that 9-byte getter, and
  `FUN_008B9DE0` its singleton. So **the price is one value per (agent type, culture)**, read from a
  hash table built at load time, not a field of the promoted unit's row. The identity of the row
  behind that hash is still **UNKNOWN**.
- **`units` column #7 `unknown_3c` is INFERRED unlikely as the price** (was "REFUTED"; the 2026-10-06
  review: the data below is CONFIRMED, but it only rules the column out if the price is one number per
  (agent type, culture), which is itself an INFERRED reading).
  `cargo run --release -p ntw_data --example promotion_price_check` on the install, 442 rows:
  109 distinct values; ratio to `recruitment_cost` from **0.698** (`Cav_Light_British_10th_Hussars`)
  to **4.857** (`Trade_Ship_Dhow`) over 438 rows; **38 of the 119 distinct recruitment-cost values
  split** into more than one `unknown_3c` (spread 300 at cost 1060). It tracks each unit's own
  recruitment cost, so it cannot be a per-(agent type, culture) price. Concretely the two figures
  quoted from round 1 — **350** `Gen_Generals_Staff` / `Gen_Generals_Staff_Foot` and **470**
  `Gen_Generals_Bodyguard` — are two *different* numbers for rows that differ only in which
  general's entourage they are, and the naval rows run 220..3330 in the same column while a naval
  promotion is free.
- **Naval free (INFERRED since the 2026-10-06 review: static trace, decompile not kept; the probe's
  `EXEC-NAVAL` + `PAY amount=0` settles it).** `0x008E2260` reads the class's slot `+0x44` and
  passes it to the treasury spend `0x00BAF500(value, 2)`; the naval slot `+0x44` is a return-0
  stub, so the spend is made with 0. Our model charges 0 (`promotion_cost` → `Some(0)`,
  `promote_unit`), the navy panel's rows (`units_info.Ships`, CONFIRMED the same key the original
  uses — `Units` for an army) carry `PromotionCost = 0`, and `army.lua:825`'s
  `SelectedUnitsPromotionCost` sums the rows, so a price can no longer appear on a free promotion.
  Test: `ntw_sim` `the_field_promotion_...` (treasury unchanged) and `ntw_script`
  `the_promote_gate_and_price_come_from_the_model` (a navy's row reads 0).

**0-G, 2026-10-06 round 3 (`work/next/0g-characters` rebased on `sandbox/next`; Ghidra read-only
on `NR-sb-ghidra-0g-r3`, kept in the ignored `target/tmp/gh/t*out.txt`; no decompiled code in the
repo).** The promotion price, step four — **the thread is closed as "needs the running game", and
the probe is now the only way in.** Round 2 left one specific lead: trace what fills the hash
`0x00F9C2A0` reads. It is traced, and the answer is a negative.

- **The hash's owner and key (INFERRED since the 2026-10-06 review: static trace, decompile not
  kept, and the owner conflicts with the row reader's `+0x40` byte write listed below), and round
  2's wording was corrected.** `0x00F9C2A0`
  is not a free-standing table: its `this` is an **`EMPIREUTILITY::AGENT_RECORD` row**, reached
  two ways that agree — `0x00A1A2E0` stores the row at **character `+0x1AC`**, and `0x009CBC40`
  reads `*(this+0x1AC)` and hands it straight to `0x00F9C2A0`. The map is embedded in the row:
  **bucket count at `+0x3C`, bucket array at `+0x40`, 0x14-byte buckets, chained nodes
  `{next@+4, key@+8, value@+0x14}`, and the function returns `[node+0x14] + 0x2C`.**
- **The key's second half is the faction's SUBCULTURE, not "a world string" as round 2 said.**
  `0x008F3490` (9 bytes) is `*(FACTION_RECORD + 0x10)` = **`factions` column #2 @0x10**
  (`subculture`, `sc_european_west`). `FUN_008B9DE0` (78 callers) is the lazy getter for the
  **human faction's** `FACTION_RECORD`, cached at **`this+0x514`** and **defaulting to the
  literal `"britain"`**; `FUN_00A257C0` (7 callers) is the same field with a fallback through
  `world->+0x98->+0x10->+0x10`. So the price is **one value per (agent type, faction
  subculture)** — keyed off the *player's* faction.
- **The guard table is the `agents_tables` database, and its shipped file is name-only.**
  `0x00E0ED90` logs `Loading database: agents_table`; the inner table name registered by
  `0x00F94D00` is **`agents_tables`**, whose row reader is `0x00F94940` (reached through the
  thunk `0x00F9DAE8`) and whose shipped file is **`db\agents_tables\agents`**. That file is
  **1657 bytes, 65 strings** — `admiral`/`ship`, `General`/`command_land`,
  `colonel`/`command_land`, `captain`/`command_sea`, `gentleman`/`minister`,
  `Eastern_Scholar`/`scholar`, … i.e. agent-type name + trait name pairs and **no numeric column
  at all**.
- **CONFIRMED negative — the number is not in the shipped data.** The other table on the same
  axis, **`db\agent_culture_details_tables\agent_culture_details`** (5020 bytes, 163 strings:
  agent type, culture, model variant — `admiral`/`egy_european`/`euro_campaign_admiralship_weighted`,
  `bandit`/`tribal`/`Campaign_Native_American_Spy`, …), is **also name-only**. Nothing on either
  table can hold the price, so it is built at run time.
- **Not found statically (a negative search, not a proof; was "CONFIRMED negative") — no writer
  of the hash.** Across every call site of
  the string hash `0x0045C1A0` in the exe, **the only one that hashes with the map's functor at
  `+0x30` is `0x00F9C2A0` itself**; so there is no same-shape insert to follow. And the row
  reader `0x00F94940` writes **nothing at `+0x3C`/`+0x40`** — its writes are `+0x00` (string),
  `+0x0C`, `+0x18`, `+0x1C`, `+0x20`, `+0x24` (byte), `+0x28`, `+0x34`, `+0x40` (byte), `+0x44`,
  `+0x50`, `+0x5C`, `+0x60` — so the hash is filled by something *after* the row is read, by code
  this shape cannot find. Recorded **UNKNOWN**: what fills it, and which record the value points
  at.
- **The cost slots have TWO return points, and round 2's probe had the wrong one.** `0x008E2770`
  returns the price at **`0x008E279C`** (`EAX = [[EAX+0x0C]] + 0x38`) and `-1` at
  **`0x008E27A0`**; the probe had `0x008E27A0` alone, so the price line could never have
  appeared. Same shape on the naval pair (`0x008E27BC`/`0x008E27C0`, `0x008E2A6C`/`0x008E2A70`).
  Fixed, with 21 breakpoints now armed and every one tagged `ARMED <name>` as it is set.
  Self-checks: `cargo test -p ntw_data --test probe_script` (5 passed),
  `cargo test -p ntw_data --test probe_install -- --ignored` (`test result: ok. 2 passed`, **21 of
  21** opcode-checked against the shipped exe),
  `cargo test -p ntw_data --lib debugger` (`test result: ok. 9 passed`).
  The self-test **caught a real bug in my own rewrite** — a dropped digit,
  `Napoleon+0x59C2A0` where the offset is `0xB9C2A0` — which would have made the hash
  breakpoints silently never fire.

**Still open, and now firmly the user's one sitting:** the *number*, and which record it comes
from. `analysis/fidelity/debugger/0G_PROMOTION_PROBE_2026-10-05.md` is the run sheet. **Do not open
a fourth static thread on this** — the builder is not reachable by the hash's shape and the data
has been shown name-only on both tables. If the probe reports one flat number, the model replaces
the hire formula and BACKLOG 0-G's Open list loses its last item.

**The model side, completed this round (no `crates/napoleon` file touched; `scene.rs` is 0-D's).**
`CampaignModel::fog_state_at(faction, col, row)` is now the single definition of the three-way fog
test, with `fog_state` (per map position) and `fog_states` (per cell, `row * cols + col`) both
deferring to it, so a renderer and the labels layer cannot shade a cell differently from the way
it is filtered; `cell_centre` is the inverse of `SightGrid::cell_of`. `fog_states` is confirmed to
cover the **whole map grid** (`world.sight_grid`, the same grid the shroud's saved
`QUAD_TREE_BIT_ARRAY` uses), not just the regions. New `CampaignModel::knows(faction, pos)` is the
labels layer's test and is deliberately **not** `sees`. **Fixed a real bug:** the campaign labels
(`RetrieveVisibleEnitityDetails` in `ntw_script/src/ui/campaign.rs`) filtered with `sees`, so a
settlement that had been seen and was now only explored lost its label while the terrain renderer
would still be drawing it dimmed; it now filters with `knows`. The exact tint is still PROVISIONAL
/ INFERRED — the cell sets are CONFIRMED, the shading is not read from the exe.

**Tightened from PROVISIONAL to a CONFIRMED negative (data).** `CurrentNumGenerals` /
`MaxGeneralsAllowed`: there is **no general limit in the shipped data at all**.
`db\campaign_variables_tables\campaign_variables` holds exactly ten `character_recruitment*` keys
(base cost, cost per command star, max distance, pool cap, six refill rates) and none caps the
number of generals; `db\effect_bonus_value_basic_junction_tables\effect_bonus_value_basic_junction`
has four `character_recruitment*` rows and none grants such a bonus; and no key containing
`generals` or `num_generals` exists in any of the **86,977** files in `data.pack`. Ours answers
"commanders of that kind in the world" and "that plus the pool cap"; the pair only feeds the
panel's "3 / 5" label, so it is cosmetic.

**Review of 0-G rounds 1-3 (2026-10-06, branch `work/review-next-0g`).** The bar: a CONFIRMED
needs evidence on record (install bytecode described, shipped data, a real-file test, or an exe
address with a kept decompile). 0-G's Ghidra output was kept only in the sandbox's ignored
`target/tmp/gh/`, so:
- **Downgraded to INFERRED:** naval promotion free; the land cost slot's shape (one value per
  (agent type, culture), no rank or distance); the hash's owner and key; "the price needs the
  running game" (now: no static writer found, which is the case for the probe, not a proof).
  The model behaviour is unchanged (naval charges 0, land charges the PROVISIONAL hire formula).
  `units` #7 `unknown_3c`: "REFUTED" → INFERRED unlikely. The two shipped name-only tables stay
  CONFIRMED (shipped data).
- **Kept CONFIRMED:** the hire's script surface `CampaignUI.PromoteUnits(force, candidate)` and
  its argument order (bytecode read on the install and described, `enlist_commander.luac:106`);
  the shroud's three cell sets and `0x00B7A150`'s test (save-checked).
- **The label change is INFERRED:** filtering the settlement labels with `knows` (visible or
  explored) instead of `sees` keeps an explored settlement's label. Nothing in the exe or in game
  has shown the original does that; it is the consistent reading of the explored set, and it is
  the one visible behaviour change of round 3 — check it in game.
- **Fixed in the model:** `fog_state` answered `NeverSeen` off the grid even for a faction with
  no shroud (while `sees` answered true), so `knows` could be false where `sees` was true;
  `fog_state_at` answered `Visible` outside the grid while `fog_state` answered `NeverSeen`;
  `cell_centre` panicked without a grid (now `Option`). `fog_state == Visible` is now exactly
  `sees`, tested on and off the grid, and the turn-end transition Visible → Explored is tested.
- **Fixed in the probe:** see `debugger/0G_PROMOTION_PROBE_2026-10-05.md` "History" — `#` / `$$`
  comment lines cdb would have run, `ARMED` tags that printed on hit rather than on set, and an
  absolute log path. The checker now also refuses any command that is not read-only.

**Sandbox 0-G static trace (2026-10-04, branch `work/sandbox/0g-characters`, no code change, no push).**
Ghidra read-only against `NR-spt-ghidra` (project opened `-readOnly`, scripts from this sandbox's own
`analysis/fidelity/ghidra_scripts`, raw output only in the sandbox's ignored `target/tmp/gh/`); specs below
are in our words, no decompiled code stored. Targets and findings:
- Charge mechanism (CONFIRMED static): land `0x008E1C20` and naval `0x008E2260` each call the unit
  class's slot +0x44 first and pass the result to the treasury spend `0x00BAF500(value, 2)`; the UI
  builder `0x009ABE00` calls the same slot +0x44 and publishes it as `PromotionCost`. The displayed
  cost and the charged cost are the same slot value by construction.
- Hire comparison (CONFIRMED static): pool slot 27 `0x00A1B8F0` takes the candidate, computes the price
  through slot +0x3c (pool slot 15 `0x00A1BB20`: base from variable 0x5b + per-star 0x5c × rank from
  `0x00A198D0`, plus distance slot 11 `0x00A1BBE0`) and pays it with `0x00BAF500(cost, 2)`. Different
  source from the promotion slot, so PROVISIONAL (hire formula for promotions) is now INFERRED-wrong
  in structure, but the replacement value is not yet CONFIRMED.
- Cost value (INFERRED): land slot +0x44 is `0x008E2770`, a static table read through `0x008E27D0`
  (with the `spa_napoleon` guerrilla special case) returning a record field, or −1 with no record —
  no rank or distance input. Naval slot +0x44 is a return-0 stub (naval promotion charges 0 through
  the treasury spend). The neighbouring slots +0x38 / +0x3c read as a value pair (naval
  `0x008E27B0` / `0x008E2A60`).
- Gate (UNKNOWN): the base tables hold a return-0 stub at slot +0x40 (`CanPromoteUnit` `0x009E0AF0`
  reads it), so a promotable unit's concrete gate value cannot be read statically; the executor
  returns early on 0. The end-of-path `0x008F35E0` spend against field +0x6c (via `0x0047B940`) is
  present in both promotion executors and both hire placements and is not the treasury move.
- Probe: rewritten for this sandbox at `target/tmp/probes/promotion_probe.cdb.txt` (old
  `NR-characters` probe read-only reference only, never modified). It logs the land cost-slot return,
  the gate-check return, `PromoteUnits` entry, the record set (`0x00A1A2E0`), both hire-cost entries,
  all three placements, every treasury `PAY` with reason, and every `0x008F35E0` `CHARGE`. One user
  sitting: promote a colonel in the field (note tooltip), hire a General (note tooltip), optionally
  hire an admiral, end turn. No code change until the log shows the charged value.

**0-E hook spec (2026-10-04, this sandbox, notes-only, no push).**
`analysis/fidelity/CHARACTER_UI_HOOKS.md` (new): for 0-E (folder `NR-sb-0e`)
— `HireGeneral`/`HireAdmiral` buttons, `PromoteUnit` button + `PromotionCost`
display (slot +0x44 table: mechanism CONFIRMED, land value INFERRED, naval 0,
gate UNKNOWN; model cost PROVISIONAL), spy actions, fog-of-war layer inputs.
Every call cited `file:line`; no Rust changes.

## 13. Agent attribute pictures (campaign bug 3, 2026-10-07)

The cards and panels' `PipPath` / `PrimaryAttributePath` used to be built as
`skins/skill_<attribute key>.tga`, which only existed for `command`, `naval`, `shooting` and
`swordfighting` (and not under those keys). The original reads them from data:
- **`agent_attributes` (`ss`, 14 rows, CONFIRMED shipped data)** maps each attribute to a picture:
  `command_land` → `skill_command`, `command_sea` → `skill_naval`, `duelling_pistols` →
  `skill_shooting`, `duelling_swords` → `skill_swordfighting`, `management` → `skill_managing`,
  `research` → `skill_research`, `subterfuge` → `skill_spying`, `zeal` → `skill_persuasion`, every
  one `data/ui/campaign ui/pips/<name>.tga`. Those eight are **loose files** of the install
  (`data\UI\Campaign UI\Pips\`, plus an unused `skill_template.tga`), not in any pack. The other
  six rows (`land_defence_engineering`, `land_siege_engineering`, `morale_land`, `morale_sea`,
  `movement_points_land`, `trade`) say the literal `PLACEHOLDER`: they have no picture.
- **The exe (Ghidra, 2026-10-07):** the character-details builder `0x009AD250` sets
  `PrimaryAttributePath` (string `0x0136EB04`, store at `0x009AE7C5`) from
  `ResolveCharacterAttributeIconPath` `0x009CA690`. That gets the `agent_attributes` table
  (`GetAgentAttributesTable` `0x00E0E580`, loader `0x00F94C10`; CONFIRMED by the table-name
  strings), turns the attribute index into its key through a fixed 14-entry key array
  (`GetAgentAttributeKeyByIndex` `0x00F9DB20`, array `0x0145D978`; an index ≥ 14 logs "Attribute
  out of range") and finds the row (`0x00F9C760`; an unknown key logs "is not a valid key for this
  table" and gives an empty string). The row string goes to a path resolver `0x00A06F20`, which
  splits it at the last `/` or `\` and first tries `<folder>/<skin>/<file>` for up to two UI skin
  folders (format `%S%S/`, existence-checked through the file system), falling back to the path
  itself. CONFIRMED: the source is `agent_attributes`. INFERRED: the column is the icon one (the
  row field offset was not traced; the only other column is the key, and the result is treated as
  a path). INFERRED: a `PLACEHOLDER` row is handed on unchanged.
- **Which attribute is primary, and its level (CONFIRMED static, 2026-10-07 round 2):** the builder
  first takes the character's main-attribute index (`0x00A198C0`) and his rank (`0x00A198D0`, our
  `agents::rank`). Only when the index is below 14 does it set the three `Primary*` fields:
  `PrimaryLevel` = rank + 1, at most 9 (`0x009AE759..0x009AE768`); `PrimaryAttributePath` through
  `0x009CA690`; `PrimaryAttributeName` through `0x00F9C450`. So the primary attribute is the type's
  main attribute, not the highest one. Ours now does the same (`attributes_table` in
  `campaign.rs`, used by the character details, the recruitment rows, the agent-action target rows
  and the commander's unit card), so a tie or an empty attribute list can no longer pick another
  attribute (the old `max_by_key` / `command_land` fallback is gone).
  Re-checked 2026-10-07 (round 3): the rank is stored at `0x009AD285` into the stack slot that
  `0x009AE759` reads back, and nothing writes that slot in between (only `LEA`s of the same
  offset at deeper stack depths, which are other slots); the decompiler shows the same single
  variable. `0x009AE759..0x009AE768` is `INC` then `CMOVL` against 9, so `PrimaryLevel` =
  `min(rank + 1, 9)` with no lower clamp of its own; the rank (`GetCharacterRank`) is already
  clamped to -1..9, so `PrimaryLevel` is 0..9. The formula is CONFIRMED (static). The rank it
  reads is ours (`agents::rank`): CONFIRMED for agents, PROVISIONAL for generals, admirals,
  ministers and missionaries, whose situation bonuses (`GetCharacterRank`: theatre of the region
  for zeal / command_land, battle type and side, enemy faction / culture, army and fleet make-up,
  minister post; each an effect-bonus id read from the character's own effects) are not
  modelled. So their `PrimaryLevel` stays PROVISIONAL until those bonuses are in. A character id
  with no record shows empty `Primary*` fields (PROVISIONAL; the exe always has the record).
- **The per-attribute part:** the builder then loops over all 14 attributes. Each entry gets a
  value from `0x009CB560`, `PipPath` (first the raw row path, then overwritten by the skinned
  resolver `0x009CA690`) and `AttributeName`, and goes into `Attributes` under the attribute's key.
  INFERRED from the loop shape: all 14, keyed by key. PROVISIONAL in ours: the character's own
  attribute list, as an array.
- **Missing row / PLACEHOLDER:** for a key with no row, `0x00F9C760` returns a shared static string
  holding the key text itself, after logging the bad key (CONFIRMED). The resolver `0x00A06F20`
  hands back a separator-less `PLACEHOLDER` unchanged unless a `<skin>/PLACEHOLDER` file exists
  (INFERRED from its split / exists / fall-back flow). Neither names a file, so nothing is drawn.
  Ours gives the empty path for both at the UI boundary, so the texture loader does not warn.
- `agent_attributes` loads as an optional table: when it is **missing** the load prints a `WARN`
  line and uses an empty table (no icons) instead of failing the campaign. A present table that
  does not read (corrupt, wrong version, bad mod override) still fails the load, so data errors
  stay visible.
- Ghidra (saved): `GetAgentAttributesTable` `0x00E0E580`, `LoadAgentAttributesTableRows`
  `0x00F94C10`, `GetAgentAttributeKeyByIndex` `0x00F9DB20`, `GetAgentAttributeIconPathByIndex`
  `0x00F9C760`, `ResolveCharacterAttributeIconPath` `0x009CA690`, with prototypes, plates, the
  structs `UniString` / `AgentAttributeRecord` / `AgentAttributesTable` /
  `AgentAttributesTableLoader` (only the offsets above are known), and labels on the attribute key
  and UI field strings. Round 3 added `BuildCharacterDetailsInfoTable` `0x009AD250` (cdecl, hidden
  result + character; every field-name constant commented), `GetCharacterMainAttributeIndex`
  `0x00A198C0` and `GetCharacterRank` `0x00A198D0` (plate lists each situation bonus by effect id),
  the structs `CampaignCharacter` / `CharacterTypeRecord` / `ScriptTableRef` (known offsets only;
  `ResolveCharacterAttributeIconPath`'s character parameter now typed `CampaignCharacter *`), the
  theatre / faction / minister-post key strings `GetCharacterRank` compares against
  (`szGame_*`), the builder's field-name and path strings (`szUI_*`, `szFmtPath_*`) and the
  rank function's unit-class switch table.
- **Ours:** `CharacterTables::attribute_icon` (`crates/ntw_data/src/characters.rs`) returns the row's
  icon verbatim (empty for an unknown key); every card / panel / recruit row / target row in
  `crates/ntw_script/src/ui/campaign.rs` uses it through `attribute_icon`. The commander entry of
  the unit cards now shows the main attribute's picture of his type (`agents::main_attribute`)
  instead of a fixed `skill_command`. Lookup is by key, so mods may add attributes (original limit:
  the 14-entry key array). PROVISIONAL: the skin-folder override of `0x00A06F20` is not modelled
  (the install has no skin sub-folders under `Pips`, so vanilla is unaffected). Test:
  `ntw_data/tests/real_install.rs::agent_attribute_icons_exist` (ignored, needs the install) checks
  every non-`PLACEHOLDER` icon exists and every model attribute has a row. Unit tests:
  `ntw_script` `character_cards_show_the_main_attribute_and_its_rank` (main attribute over a
  higher one, tie, empty list, level = rank + 1 clamped to 9, `PLACEHOLDER` → empty path, pips
  keep their own pictures; made-up `agent_attributes` rows) and `ntw_data`
  `a_missing_optional_table_is_empty_not_an_error`.

## 14. Portraits of new characters (2026-10-10, worker new-portraits)
All CONFIRMED by static trace (Ghidra names applied in this round) unless tagged; specs in our words.
- **The allocator** (campaign model +0xF94, `PORTRAIT_ALLOCATOR` in every start position and save): per culture
  `CULTURE_PATHS` {agent type → folder culture} and `PORTRAIT_CATEGORIES`, one per agent type plus `king` / `queen`,
  each with four decks in the portrait-type order of `BuildPortraitPicturePath` (`0x009CBF40`): Info young, Info old,
  Cards young, Cards old. A deck (`PORTRAIT_ALLOCATION`) = {count, next position, order}; its own LCG seed (+0x18)
  is not saved. Save layout from the loader `LoadCulturePortraitPaths` (`0x009942D0`) / `LoadPortraitAllocation`
  (`0x00999600`) and the writers `SaveCulturePortraitPaths` (`0x009A0C50`) / `SavePortraitAllocation` (`0x009A2190`).
- **Loading advances the campaign RNG**: the loader steps the campaign RNG (model +0xFB8, the `RandSeed` state; the
  pointer passed from `0x00872550` through `LoadPortraitAllocator` `0x00999870`) once per deck, in file order, and
  seeds the deck with the high 16 bits. 4 × 19 × 9 = 684 steps for a vanilla file. Whether any other part of the load
  draws from +0xFB8 is not traced (our loader draws nothing else).
- **Drawing** (`DrawNextPortraitNumber` `0x009CA8C0`): count 0 → -1; position ≥ count → position 0 and
  `ReshufflePortraitDeck` (`0x00A1E910`: one step of the deck's LCG, then the name decks' `random_shuffle` of the
  numbers in place); returns order[position], position + 1.
- **Who gets which portrait**: the details constructor `ConstructCharacterDetails` (`0x00992E60`) starts the number
  (+0x7C) at -1, reads its portrait string (all digits → the number; `guerrilla…` → the `guerilla` agent record and
  the number after 9 letters, `0x004F3720` parse: empty = 0; anything else → the custom picture name +0x98), then
  `AssignCharacterPortraitForCulture` (`0x009CB3C0`) → `ResolveCharacterPortraitPictures` (`0x00A05440`) with his
  faction's culture, the agent record and his age: custom name → `BuildNamedCustomPortraitPath` (`0x009CBD60`); else
  while the number is -1, draw from the category named by the agent record's portrait folder (+0x1C = `agents` #6,
  or the key when #6 is empty) if the culture has one, else the agent key's own category (case-sensitive hash
  `0x0045C1A0`); Info old deck at age > 44, else Info young; then card / info pictures
  `ui/portraits/<CULTURE_PATHS[agent key]>/{Cards|Info}/<folder lower-cased>/{young|old}/NNN.{tga|jpg}`.
  So admirals (#6 `General`) draw from the General decks and the missionaries and the gentleman (#6 `minister`)
  from the minister decks; guerilla (#6 `guerrilla`) and Eastern_Scholar (#6 `scholar`) from their own.
  The 30 / 50-year pictures do not follow later ageing: nothing re-resolves on age.
- **Generic characters** (`ConstructGenericCharacter` `0x0098F250`, the pool's): age drawn (`0x00A05740`), named
  (`0x009940A0`), details with an empty portrait string → drawn. **Historical** (`ConstructHistoricalCharacter`
  `0x0098F880`): portrait string from `GetHistoricalCharacterPortraitSpec` (`0x00A06430`), the record's +0x3C:
  `guerrilla#<region>#<NNN>` → "guerrilla" + NNN (11 Peninsular guerrilla leaders); every other row → empty → drawn.
  That +0x3C is `historical_characters` #7 (note): CONFIRMED by the data, the only column holding `guerrilla#`
  strings (the builder also puts #7 at 0x3C, DB_BUILDERS.md).
- **Field promotion**: a unit without a character gets one from `ConstructUnitOfficerCharacter` (`0x00990EF0`):
  age drawn first, named, details as the officer type (agent index 2 colonel / 3 captain from `0x00F9DB40`, naval
  `0x008E2260` passes 3) whose decks are empty → number -1; then `ApplyAgentRecordToCharacterDetails`
  (`0x00A1A300`) with the General / admiral record re-resolves the portrait at his current age: number -1 → drawn
  now; a kept number is reused with the new folder.
- **Data check** (`ntw_campaign/tests/portraits_install.rs`): every non-empty deck of the eur, egy, spa and ita start
  positions has the count the builder's probe gives for its folder (folder rule CONFIRMED); the decks of
  `PIR_european General`, `middle_east assassin`, `european gentleman` are 0..count shuffled from seeds that are four
  consecutive chain steps (one chain state found each); `european General` young (cursor 32) is the in-place
  reshuffle of its first deal (draw semantics CONFIRMED). The start positions were built with older tables: eur's
  missionaries have empty decks (path "" = european's fallback culture), spa's have the minister counts.
- **The builder** (start positions; `BuildPortraitAllocator` `0x00999BE0`, `BuildCulturePortraitDecks` `0x00994DC0`,
  `BuildAgentPortraitDecks` `0x00A1F890`, `FillPortraitDeckFromFolder` `0x00A1F730`): per culture (cultures table
  order), per agent (agents table order) then king and queen, per type: chain step, deck seed = chain >> 16, count
  the pictures, deal 0..count-1, shuffle; no pictures → CULTURE_PATHS[agent] = the culture record's +0x14 (fallback)
  and fill again. Chain start 0x61266 in the start-position path (`0x00876BA0`); a new campaign loads the start
  position's allocator instead. **ORIGINAL BUG** (`0x00A1F730` / `FindLastPortraitPictureNumber` `0x009DC310`): the
  count is the LAST picture number found (probes 0, 1, 2, 4, … then halves), so the last picture of every folder is
  never dealt (96 european General young pictures, decks of 95) and a folder with one picture counts 0. Ours has no
  builder yet (every source has an allocator); the fix (count = last number + 1) goes with it (BACKLOG).
- **Model** (`ntw_sim::campaign::portraits`): `World::portraits`, `PortraitDeck::draw` / `reshuffle`,
  `CampaignModel::assign_portrait` / `assign_historical_portrait`, `draw_new_character_age`; rules
  `agent_portrait_folders` (`agents` #6, renamed from `base_agent`) and `HistoricalCandidate::note`. Callers:
  `create_candidate` (generic and historical), `promote_unit` (new officer: age, name, officer-type portrait; then
  the new type's). The campaign source reads the allocator (`ntw_campaign::portraits::fill`, one RNG step per deck)
  and the ESF writer writes the decks back and each character's `PORTRAIT_DETAILS` when the model has one; own saves
  keep the whole allocator (seeds included). `Portrait::default()` has number -1 (the constructor's). The resolve
  follows `0x00A05440`'s order (CONFIRMED, traced 2026-10-10): `0x009CB3C0` returns with nothing changed when the
  allocator has no set for the culture; the number is drawn and stored first, then `BuildPortraitPicturePath`
  (`0x009CBF40`) builds both paths. Its `CULTURE_PATHS` lookup is not checked against the map's end (the builder gives
  every agent type an entry, so the shipped data never misses one); ours keeps the drawn number and leaves the
  pictures as they were (empty for a new character, the old agent type's on a promotion). `%03d` is signed, so a negative fixed number (a modded `historical_characters` #7) gives a
  "-05" path, as ours does; a fixed -1 is drawn over like any -1. The constructor stores a guerrilla leader's number
  before the resolve, so a custom picture name keeps it. `CampaignModel::portrait_problem` names each gap (no
  details, no allocator, no culture set, no `CULTURE_PATHS` folder, empty deck, unresolved number, custom name
  without a card, a number below -1, a card kept from an earlier type) for the app's log, as the agent type his
  portrait resolves as (`portrait_agent`: `guerilla` for a historical guerrilla leader).
- **UI**: the army card's `Portrait` and the commander pool row's `InfoImage` are "data/" + the character's card
  picture (character +0x370 = details +0x80 = `PORTRAIT_DETAILS` #0, CONFIRMED by the writer `SavePortraitDetails`
  `0x0099F6D0`), both through `portrait_image`; the Lists row shows generals and admirals as themselves
  (`0x009AD250`). Without a card (no allocator in the source or an older own save, a gap above) the value is empty,
  so the army card shows its unit picture, and `portrait_image` logs the model's reason once per character.
- **Field promotion's guerrilla flag** (CONFIRMED, traced 2026-10-10): `ConstructUnitOfficerCharacter`'s last
  argument; set, the portrait string is "guerrilla" (the `guerilla` record, number 0). Land
  `PromoteLandUnitCommanderInField` (`0x008E1C20`) makes a force (`0x0087FEA0`, vtable `0x01355494`) whose slot 1
  `AttachNewColonelToLandForce` (`0x008B7EF0`) pushes 0 (`0x008B7F05`); naval `0x008E2260` pushes 0 (`0x008E2558`),
  as does `AttachNewCaptainToNavalForce` (`0x008B7F60`, `0x008B7F85`). So a promoted unit's officer always draws.
- Open: new ministers and family members (`0x008BF310`, `ResolveFamilyMemberPortraitPictures` `0x00A055E0` with the
  king / queen decks) and agents trained in buildings still take their portraits from the save writer's templates
  (§5c PLACEHOLDER).

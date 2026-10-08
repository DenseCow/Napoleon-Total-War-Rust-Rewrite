# Character → UI hook spec (0-G for 0-E, sandbox notes-only)

Branch `work/sandbox/0g-characters`, commit `2bb08d9` + this note. No Rust
changes. No Ghidra runs (reuses the 0-G trace: promotion/hire/spy paths,
`CharacterCreated` / `CharacterPromoted` / `CharacterBuildsSpyNetwork`,
shroud/sight model). Read against `CHARACTERS_FIDELITY.md` §§7–8, 10–12 and
`UI_FIDELITY.md` settlement/diplomacy sections (settlement panel §2, capture
note, diplomacy/government rows, lists panel).

Tags: CONFIRMED / INFERRED / UNKNOWN (findings), PROVISIONAL (model
stand-ins). Every model call cites `file:line` on this branch. "UP states"
below is read as **unit-promotability states** (the per-unit gate + effect +
cost triple the PromoteUnit button needs); §H2 notes the other reading
(character `no_action`/`idle_turns`) explicitly.

**On main** (ported from sandbox/main 90bfc1c, branch `work/sandbox-port-ui`): this file is the
sandbox's notes as written; `file:line` references are to the sandbox branch. Of §H5, the naval
recruitment tab (T1) and the infrastructure tab (T2) are wired in `ntw_script::ui::campaign`; the
agents tab (T3) is wired on branch `work/campaign-agents-tab` (see "Agents panel" below). The
sandbox's `GenerateAgentsPanel` info was incomplete: its own test tolerated script errors from the
card templates.

Branch `work/campaign-fort-agents`:
- the six `CampaignUI.Agent*` calls of T3 (`AgentCardSelectionChanged`, `AgentEmbarkOrDisembark`,
  `AgentGentlemanDuel`, `AgentRakeAssassinate`, `AgentRakeSubterfuge`, `AgentRogueSabotageArmy`)
  are bound as PROVISIONAL no-ops, and `CanAgentEmbarkOrDisembark` answers `false`;
- `ShowAgents`, `ShowAgentButtons`, `AgentCardPosition`, `SelectAgentCard` and `AgentsPanelActive`
  are script-defined globals, not engine calls, so they are not bound;
- the tab stayed unlisted until `ui/agents.luac`'s reads of `info` were known (now done, see
  "Agents panel" below).

**Install probe (2026-10-05, the then-ignored test `agents_panel_opens_without_script_errors`).**
Superseded by "Agents panel" below for `agents`, `card_id` and `controlable`; kept as the record.
CONFIRMED on the install: the campaign root env defines `GenerateAgentsPanel`, `AgentManager`,
`SelectAgentCard`, `IsAgentsPanelActive`, `OpenAgentActionPopup` and `OpenAgentOptionsPopup`;
`layout.root.lua:448` forwards to `agents.GenerateAgentsPanel` (`UI/Agents.lua:154`), which calls
`agents.GenerateAgentCards` (`Agents.lua:220`ff) and `Utilities.CreateCharacterCard`
(`UI/Utilities.lua:111`ff), which instantiates `template.CampaignCharacterCard`.
The `info` fields the scripts read (field names CONFIRMED by the probe; types INFERRED from the
Lua errors, values still to be sourced from the model):

| Field | Type (INFERRED) | Read at |
|---|---|---|
| `agents` | list | `Agents.lua:220` (`#agents`) |
| `agents[i].card_id` | string or number (used as a table key) | `Agents.lua:232` |
| `characters` | list | `Agents.lua:234` |
| `characters[i].Address` | string | `Utilities.CreateCharacterCard` |
| `characters[i].Flag`, `.SmallFlag` | string (image paths) | `Utilities.lua:111` |
| `characters[i].CardImage` | string (image path) | `Utilities.lua` |
| `characters[i].CommanderType` | string | `Utilities.lua` |
| `characters[i].Attributes.PrimaryAttributePath` | string (image path) | `Utilities.lua:123` |
| `controlable` (sic) | list | `Agents.lua` |

The card template (`template.CampaignCharacterCard.luac`, its function at line 29) calls the
engine's `CampaignUI.CharactersRelationshipToPlayersFaction(info.Address)` only when its second
argument is true. CONFIRMED from that template's bytecode: the answer is a number; the main chunk's
locals are `0, 1, 2, 3` with `RTPF_OWNED = 0` (the card destroys its `faction` badge) and
`displayed_rtpf_states = {"neutral", "ally", "foe"}`, indexed by the answer, giving the badge state.
The same function also reads `ShowAttributes`, `Attributes.PrimaryLevel`, `ShowFlag` and `TechImage`.
Now bound (`relationship_to_players_faction`): own faction 0, neutral 1, allied / protectorate /
patron 2, war 3. The stance-to-answer mapping is INFERRED from the state names; the exe's function
is not traced.

## Agents panel (branch `work/campaign-agents-tab`)

Source: the user read `ui/agents.luac` and `ui/templates/template.campaigncharactercard.luac` with
`luac_dump` on the install (2026-10-05). Behaviour is described here in words; no bytecode or
decompiled code is in the repo.

**CONFIRMED from the panel's bytecode.**
- `GenerateAgentsPanel(info)` (Agents.lua:149) resets its card map and calls
  `GenerateAgentCards(info.agents, info.characters, info.controlable)`. `GenerateAgentCards` takes
  two parameters, so `controlable` is read and dropped: the panel never uses it. Afterwards the
  panel walks its cards and selects the first whose `character.PlayerControlled` is true (through
  `SelectAgentCard` with the card component's address).
- `GenerateAgentCards(agents, characters)` (Agents.lua:210) clears the card group, measures a
  temporary `CampaignUnitCard`, then for each index i of `agents`: `agents[i]` and `characters[i]`
  are the same agent (parallel lists). The script sets `characters[i].PlayerControlled` itself from
  `CampaignUI.IsCharacterPlayerControlled(characters[i].Address)`. `agents[i].card_id` is the card
  map's key and the card component's id, so it is a string (a table there broke
  `CreateComponentFromTemplate`'s third argument). The card is `Utilities.CreateCharacterCard` with
  `characters[i]`, the id, the group, the position from `AgentCardPosition(i)` and four fixed flags.
  The card is interactive only when PlayerControlled, its tooltip text is `agents[i].name`
  (`SetTooltipText(name, true)`, pc 105-109), and its "unitcard" hover tooltip only hands
  `agents[i]` to the tooltip template's `InitialiseAgent` (pc 110-118, a `LuaCall` of
  `SetSpecialTooltip("unitcard", BuildAgentTooltip, agents[i], true)`).
- **`InitialiseAgent`'s fields (CONFIRMED, read on the install 2026-10-06).**
  `ui\templates\template.unitcard_tooltip.luac`, the proto at `template.unitcard_tooltip.lua:115`
  (`numparams=2`, upvalues `name_component`, `function_component`, `crew_component`, `this`,
  `expanded`), reads **two** fields of its second argument, the `agents[i]` table: `name`
  (pc 5) and `agent_type_name` (pc 7), and joins them as `name .. " (" .. agent_type_name .. ")"`.
  Everything else it does is fixed: the function component is set to the `strat_army` state, the
  crew row and `dy_stat1`..`dy_stat4` are hidden, `Resize` runs and `expanded` becomes true. So
  the unit tooltip's whole stat-block path (`Firepower`, `Guns`, `Men`, ...) is not used for an
  agent, and no third field is needed.
- `agent_type_name`'s loc key is `agent_culture_details_onscreen_name_<agent><culture>`: the agent
  type's `agents` key with the culture's `cultures` key straight after it, both as the DB spells
  them (`General`, `Eastern_Scholar`). **CONFIRMED from the shipped data**: the install has exactly
  53 such keys and `agent_culture_details` exactly 53 rows, and each row's `<agent><culture>` is a
  key (test `every_agent_culture_row_has_its_onscreen_name_key`, `ntw_script`, install). The
  culture is the character's -- his faction's, as `CharacterCultureType` (0x0089C240) reads -- and
  the names really differ per culture: a rake is a "Spy" in `european` and a "Scout" in `tribal`,
  an assassin a "Hashishin" in `middle_east` and a "Thugee" in `indian`.
- The character card (template function at line 29) reads `Address`, `ShowAttributes`,
  `CommanderType` (compared with Utilities' CT_* values), `Attributes.PrimaryLevel` /
  `PrimaryAttributePath`, the `Flags` child / `ShowFlag`, `SmallFlag` and `TechImage`, and asks
  `CharactersRelationshipToPlayersFaction(Address)` when its second argument is true.
- `ShowAgentButtons(card)` (Agents.lua:68) reads `card.character.Abilities.can_assassinate`,
  `can_sabotage`, `can_sabotage_army`, `can_research` and `can_duel` (each compared with true) and
  `card.character.IsGuerilla`. It asks, each with the agent's Address: `CharacterResidence` (tested
  against nil), `IsCharacterInPortResidence`, `CharacterInEnemyResidence`,
  `ValidAssassinationTargets`, `ValidSabotageTarget`, `ValidSabotageArmyTarget`,
  `CharacterInValidEnemyUniversity` (compared with true), `ValidDuelTargetsInResidence` and, only in a
  port residence, `CanAgentEmbarkOrDisembark` (compared with true); and `CampaignKey()`:
  `spa_napoleon` with `IsGuerilla` shows the harass button instead of the sabotage-army one. The
  answers are the buttons' active flags (`ShowButton(button, index, active)`).
- The action buttons pass only the selected agent's Address (arity 1): `AgentGentlemanDuel`,
  `AgentRakeAssassinate`, `AgentRakeSubterfuge`, `AgentRogueSabotageArmy`; `AgentEmbarkOrDisembark`
  only with a current card during `IsPlayersTurn()`. Steal calls the script's own
  `ShowStealingTechnologies(CharacterResidence(addr), addr)` and asserts the residence is not nil.
  Card selection calls `AgentCardSelectionChanged` with the card's `ItemAddress`.

**The sabotage and duel `and`/`or`.** Those two buttons combine their target answer with the
enemy-residence / has-residence flag with an `and` or an `or` (a `TESTSET` in the bytecode). How to
read it: in Lua 5.1 (`lvm.c`), `TESTSET A B C` copies R(B) to R(A) and jumps when R(B)'s truth
equals C, i.e. C = 0 jumps on a false R(B) (the short cut of `and`; `lcode.c` emits C = 0 for
`and`) and C = 1 jumps on a true one (`or`). The
dumper (`crates/ntw_formats/examples/luac_dump.rs`) prints C = 0 as "jump if true" and C = 1 as
"jump if false", which is the **reverse** of what the VM does. So a printed "TESTSET ... jump if
true" is an `and`, and "jump if false" is an `or`. (Its `TEST` label is taken from A's low bit, not
from C, so a printed `TEST` direction cannot be trusted either.) **Settled from the install dump
(2026-10-05):** both are printed "jump if true", so both are `and` (CONFIRMED). The sabotage button
is active when `CharacterInEnemyResidence(agent) and ValidSabotageTarget(agent)`. The duel button is
active when `CharacterResidence(agent) ~= nil and ValidDuelTargetsInResidence(agent)`. The host does
not depend on it, because the script does the combining on our answers.

**Built from the model (`ntw_script::ui::campaign`, `agents_info` and the bindings).**
- Which agents the tab lists (INFERRED; the engine's list is not traced): every non-commander type
  (not general, colonel, admiral, captain or minister) standing in the settlement's region
  (`economy::in_region`) that the human's faction knows of (`agents::knows_character`; hidden spies
  are left out). The tab is listed last (CONFIRMED position in `FUN_0099A200`) when that list is
  not empty (INFERRED condition).
- `agents[i] = {card_id, name, agent_type_name}`: `card_id` = `agent_<character id>` (PROVISIONAL
  format, unique); `name` = the character's name, else his type's (as `character_details`' `Name`);
  `agent_type_name` = his type's name in his faction's culture, loc
  `agent_culture_details_onscreen_name_<agent><culture>` (CONFIRMED, see above; the ESF type name
  when the model knows no culture for his faction -- INFERRED stand-in).
  **`agent_type_name` is CONFIRMED as the field the tooltip needs** --
  `template.unitcard_tooltip.lua:115` builds `agent.name .. " (" .. agent.agent_type_name .. ")"`
  in the hover tooltip, and without it the agents panel raised a script error on hover (the
  install test `agents_panel_opens_without_script_errors` caught it; it was failing on `next`
  and now passes).
- `characters[i]` = `character_details` (the exe's `0x009AD250` key names) plus `Abilities`.
  `ShowAttributes`, `ShowFlag` and `TechImage` are not given (UNKNOWN values).
- `Abilities.<key>`: with saved `AgentAbilities`, the model's ability level is at least 1 (the
  threshold of the model's own action gates; `can_assassinate` also needs a spy type, the model's
  assassination gate). Without saved abilities, INFERRED from the type: rake / assassin / guerilla
  assassinate and sabotage (buildings and armies), gentleman / scholar duel and research,
  missionaries none of the five.
- `IsGuerilla`: the `guerilla` type (INFERRED).
- `controlable`: an empty table (unused, CONFIRMED).

**The bindings and their tags** (an address that is not a character answers false or nil):

| Call | Answer | Tag |
|---|---|---|
| `CharacterResidence` | garrisoned in a region → its settlement (region address); else on a non-settlement slot's position → that slot (slot address); else on a settlement's position → it; else nil | INFERRED (the model has no residences) |
| `IsCharacterInPortResidence` | the residence is a slot with `port` | INFERRED |
| `CharacterInEnemyResidence` | the residence's holder (slot holder, else region owner) is another faction | INFERRED ("enemy" as the model's actions take it: any other faction) |
| `ValidAssassinationTargets` | some foreign character known to the agent's faction passes `assassination_chance` | gate CONFIRMED; candidates PROVISIONAL (whole map, no reach) |
| `ValidSabotageArmyTarget` | some foreign force with a known commander passes `army_sabotage_chance` | gate CONFIRMED; candidates PROVISIONAL (whole map) |
| `ValidSabotageTarget` | a foreign-held building in the residence (the slot's, or the settlement's slots) passes `building_sabotage_chance` | gate CONFIRMED; "in the residence" INFERRED |
| `ValidDuelTargetsInResidence` | a known foreign character in the same residence passes `duel_chance` | gate CONFIRMED; residence INFERRED |
| `CharacterInValidEnemyUniversity` | the residence is a slot that is another faction's school (`CampaignModel::school`, made public read-only for this) | school test CONFIRMED; meaning INFERRED |
| `CanAgentEmbarkOrDisembark` | false | PROVISIONAL (no agent embark in the model) |
| the six action calls | they open the options popup through the panel manager, which our HUD does not have | arity CONFIRMED; see H3 for the popup's own calls, which **are** wired |

Tests: `agents_tab_lists_the_settlements_agents_with_the_info_the_panel_reads`,
`the_agents_info_carries_the_hover_tooltip_fields` and
`agent_button_questions_answer_from_the_model` (lib, made-up world, no install);
`agents_panel_opens_without_script_errors` (install: places a French agent in Paris, opens the tab
on the original scripts, clicks the card, asserts zero script errors; skips without the install).
Run: `cargo test -p ntw_script --test campaign_ui agents_panel -- --nocapture`.

## What 0-G wired on 2026-10-05 (this branch)

- H1: `CanRecruitCommander`, `AvailableCommandersForRecruitment`, `PromoteUnits` (the hire) — all
  three CONFIRMED from the install's bytecode, wired to `can_recruit_commander` /
  `hire_cost_into` / `HireGeneral` / `HireAdmiral`.
- H2: `CanPromoteUnit` and the unit rows' `PromotionCost`, wired to `can_promote_unit` /
  `promotion_cost`. The value stays PROVISIONAL (see H2's re-trace).
- H3: `RequestDuelTargets`, `RequestAssassinationTargets`, `RequestSabotageTargets`,
  `InstigateDuel`, `InstigateAssassination`, `InstigateSabotage`, `SabotageArmy` — CONFIRMED from
  the install, wired to `Duel` / `Assassinate` / `SabotageBuilding` / `SabotageArmy`. `Spy` has no
  script call in the shipped UI.
- H4: `SpyingDataLevelCharacter`, `SpyingDataLevelUnit`, `show_shroud`, `unveil_black_shroud`, and
  the label / lists filters.
- Fix: `agents[i].agent_type_name`, without which hovering an agent card raised a script error.
- New model reads (all with tests): `CampaignModel::can_recruit_commander`, `can_promote_unit`,
  `promotion_cost`.

Files: `crates/ntw_sim/src/campaign/pool.rs` (+ `tests.rs`),
`crates/ntw_script/src/ui/campaign.rs` (bindings in `install_functions`; `unit_entry`,
`character_details`, `agents_info` and the `RetrieveVisibleEnitityDetails` /
`RetrieveFactionMilitaryForceLists` bodies), `crates/ntw_script/src/game.rs`,
`crates/ntw_script/tests/host.rs`, `crates/ntw_data/examples/unit_promotion_probe.rs` (the research
helper that dumped the `units` cost columns for the promotion-cost question).
**Nothing under `crates/napoleon` was touched.**

## H1. HireGeneral / HireAdmiral buttons

**The script surface is CONFIRMED (read on the install 2026-10-05, 0-G).** The original's name for
hiring a pool candidate into a force is **`CampaignUI.PromoteUnits(force, candidate)`**, and the
panel that offers the candidates is the `enlist_commander` popup:

- `ui/campaign ui/layout.army_promote.luac:13` and `layout.navy_promote.luac:13` -- the army / navy
  panel's Promote button (`g_button_army_promote` / `g_button_navy_promote`) calls
  `army_manager:PromoteUnits()` (arity 0) when its state is not `inactive`.
- `ui/army.luac:582` -- that is Army.lua's `PromoteUnits`: it takes the card group's selection and
  calls `panel_manager:OpenPanel("enlist_commander", nil, nil, nil, "InitEnlistCommander",
  g_panel_is_navy, g_military_force)`. **So the popup gets (is_navy, force).**
- `ui/campaign ui/enlist_commander_scripts/enlist_commander.luac:14/38` -- `InitEnlistCommander` keeps
  `m_military_force` and calls `FillOutInformation(force, is_navy)`, which asks
  **`CampaignUI.AvailableCommandersForRecruitment(force, is_navy)`** (arity 2) and stores the answer
  in the global `commanders_table`.
- `ui/army.luac:708` -- the button is shown only when **`CampaignUI.CanRecruitCommander(force,
  is_navy)`** answers true (`army.lua:1058` `ShowArmyButtons`).
- `ui/campaign ui/enlist_commander_scripts/enlist_commander.luac:106` -- confirming the pick calls
  **`CampaignUI.PromoteUnits(m_military_force, m_current_selected_commander)`** (arity 2), where the
  second argument is an enlist row's `commander_pointer` (the field name as the entry list below
  and the binding spell it; an earlier draft of this line said `CommanderPointer` -- re-check the
  case on the install, Lua field names are case-sensitive), i.e. **a pool candidate**.

Per-candidate fields (CONFIRMED, `enlist_commander_entry.lua:14`): `commander_pointer` (what
`PromoteUnits` is handed), `Name`, `RecruitmentCost` (a string, it goes straight into `dy_cost`'s
state text), `Attributes.PrimaryLevel` (the stars), `UniqueId`, `IsRecruitable`, `InfoImage`,
`Traits` (at most four, `enlist_commander_entry.lua:36`). Panel-level fields (CONFIRMED,
`enlist_commander.lua:38`): `CurrentNumGenerals`, `MaxGeneralsAllowed`, `MaxDistanceToTrack`,
`DistanceToCapital`, `TurnsToNextPoolFill`, plus the array part.

**Discrepancy to re-check in the exe (0-B / 0-E).** CHARACTERS_FIDELITY.md §12 reads `0x009EF360`
("`PromoteUnits`") as the *field* promotion of the selected unit's slot 0x4C. The script path above
shows the same name used for the **hire** of a pool candidate. Either the handler dispatches on the
argument's type, or one of the two notes has the wrong address; the script evidence is the stronger
one, and the model has both commands (`HireGeneral { into }` / `HireAdmiral { fleet }` for this
panel, `PromoteUnit { force, unit }` for the context menu).

**Wired** in `ntw_script::ui::campaign` (`install_functions`): `CanRecruitCommander`,
`AvailableCommandersForRecruitment` (`commanders_for_recruitment`) and `PromoteUnits` (queues
`HireGeneral { into }` for an army, `HireAdmiral { fleet }` for a navy). The model's new reads are
`CampaignModel::can_recruit_commander` and `hire_cost_into`. Tests:
`the_commander_pool_answers_from_the_model_and_hiring_queues_the_command` (lib),
`the_interface_gate_for_hiring_a_commander_follows_the_pool_and_the_purse` (ntw_sim).

**The exe's `CanRecruitCommander` gate stays PROVISIONAL** (its handler is not traced): ours is
the faction's turn, a candidate in the matching pool, and one the treasury can pay for.

**`CurrentNumGenerals` / `MaxGeneralsAllowed` — now a CONFIRMED negative rather than a gap**
(0-G, 2026-10-06): **the vanilla game ships no limit on the number of generals at all.**
`db\campaign_variables_tables\campaign_variables` holds exactly ten `character_recruitment*` keys
— `character_recruitment_base_cost`, `..._cost_per_command_star`, `..._max_distance`,
`..._pool_cap`, and six `..._pool_refill_rate_{general,admiral}_{0,1,2}` — and **none** caps the
number of commanders. `db\effect_bonus_value_basic_junction_tables\effect_bonus_value_basic_junction`
has four `character_recruitment*` rows and none grants a general-count bonus. A scan of all
**86,977** files in `data.pack` finds no key containing `generals` or `num_generals` anywhere.
So whatever the original shows here is a fixed constant, a difficulty setting, or something not
in the DB — **not** a data-driven limit. Ours answers "commanders of that kind in the world" and
"that plus `character_recruitment_pool_cap`"; `enlist_commander.lua` only uses the pair for its
"3 / 5" label, so the number is cosmetic either way.

**Not reachable in game yet:** `panel_manager` is not implemented in our HUD, so nothing opens the
`enlist_commander` panel (the six `Agent*` popups need it too). 0-E's.

Commands (enum `crates/ntw_sim/src/campaign/commands.rs:30`):

- `CampaignCommand::HireGeneral { character, into }` —
  `commands.rs:129`; dispatched at `commands.rs:501` to
  `CampaignModel::hire_general` (`crates/ntw_sim/src/campaign/pool.rs:224`).
- `CampaignCommand::HireAdmiral { character, fleet }` —
  `commands.rs:137`; dispatched at `commands.rs:502` to
  `CampaignModel::hire_admiral` (`pool.rs:293`).

Exe sites (CONFIRMED, `CHARACTERS_FIDELITY.md` §12): pool slot 27
`0x00A1B8F0` (candidate check, treasury spend `0x00BAF500(cost, 2)`,
placement, timer restart); generals `0x00A164C0` (new army merged into the
target force `0x008D2FA0`); admirals `0x00A16110` (takes command
`0x008EDE20`/`0x008D2FA0`, a captain in command removed `0x00A0C6F0(1)`).

Guards the UI must respect (all CONFIRMED structure):

- General: candidate is a `General` in his faction's general pool;
  `into` is an army (not navy) of his faction (`pool.rs:229-238`).
- Admiral: candidate is an `admiral` in the admiral pool; `fleet` is a navy
  of his faction (`pool.rs:298-304`).
- Acting faction's turn (`may_act`, `pool.rs:226,295`); treasury ≥ cost or
  `CommandError::InsufficientFunds` (`pool.rs:246-249,308-311`).

Cost to display (CONFIRMED formula, `pool.rs:183-198`):

- `CampaignModel::hire_cost(c)` (`pool.rs:170`) — candidate at his position.
- `CampaignModel::hire_cost_into(c, force)` (`pool.rs:176`) — measured at
  the target force's position (the original moves the candidate there first,
  then `0x00A1BBE0` measures; `pool.rs:181-182`).
- Formula: `character_recruitment_base_cost` (400) +
  `character_recruitment_cost_per_command_star` (300) × `rank`
  (`crates/ntw_sim/src/campaign/agents.rs:115`) + distance part
  `min(10, round(10 × min(d, max) / max)) × 100`, `max` =
  `character_recruitment_max_distance` (1000). Rank and distance both matter.

Pool contents to display (CONFIRMED):

- `FactionDetails::general_pool` / `admiral_pool` = `(Vec<CharacterId>, timer)`
  (`pool.rs:39-45`); cap `pool_cap()` (`pool.rs:56`,
  `character_recruitment_pool_cap` = 3); refill `pool_refill_time(f, kind)`
  (`pool.rs:61`); per-turn step `pool_tick` (`pool.rs:83`, phase CONFIRMED:
  faction turn start after the spotting pass, §12).
- Historical-first: `due_historical(f, kind)` (`pool.rs:111`) lists due
  `historical_characters` rows; `create_candidate` (`pool.rs:137`) picks one
  by `uniform_below` on the campaign RNG. UI should badge
  `CharacterDetails::historical_key` (`details.rs:90-92`) candidates.

Events (project events, no script names):

- `CampaignEvent::CharacterHired { character, force, cost }`
  (`crates/ntw_sim/src/campaign/events.rs:294`); emitted by both hires
  (`pool.rs:286,333`). Hiring does NOT fire `CharacterPromoted` (CONFIRMED,
  §12; `events.rs:65-71`).
- New candidates surface via `pool_tick` return + `CharacterCreated`
  (`events.rs:153-160`).

0-E wiring: offer pool candidates on the army/fleet panel ("recruit
general" is given on an army in the original); pass `into: Some(force)` /
`fleet`; show `hire_cost_into` for the selected force and `hire_cost` as
fallback. `into: None` (new army at the capital, inside the settlement when
free of a garrison army) is PROVISIONAL — no such path in the original.

## H2. PromoteUnit button + PromotionCost display

**Where the price is shown (CONFIRMED, install 2026-10-05).** The unit rows of `GenerateArmyPanel` /
`GenerateNavyPanel` carry a **`PromotionCost`** field, and Army.lua's `SelectedUnitsPromotionCost`
(`army.lua:825`) sums it over the selected unit cards that are not a General or admiral's own. So the
price belongs on the unit row, not on a button. (Both `SelectedUnitsPromotionCost` and Army.lua's
`CanPromoteUnit(card)` helper are dead code in the shipped script -- the field is set, the helper is
never called -- so the *display* of the price is the panel's or the tooltip's business.)

- `CampaignCommand::PromoteUnit { force, unit }` — `commands.rs:146`
  (docs `commands.rs:143-145`); dispatched at `commands.rs:503` to
  `CampaignModel::promote_unit` (`pool.rs:348`).
- Exe sites (CONFIRMED, §12): unit classes' slot 20 `0x008E1C20` (land) /
  `0x008E2260` (naval); record set `0x00A1A2E0` → `0x00A1A300` (copies agent
  record 0/1 attributes+abilities, fires `CharacterPromoted`, re-sums
  effects `0x009CDF10`); entries: player `PromoteUnits` `0x009EF360` /
  `CCQ_PROMOTE_COMMANDER` `0x00936C70` → selected unit slot `0x4C`,
  console `promote_unit_commander` `0x00961430`, AI `CAI_BDI_PROMOTE_UNIT`.

UP (promotability) states the button needs — show all three, tagged:

1. Unit gate (UNKNOWN value): `CanPromoteUnit` `0x009E0AF0` reads the
   unit's slot 16; base tables hold a return-0 stub at slot `+0x40`, so a
   promotable unit's concrete gate value is not statically known (§12
   sandbox trace). The executor returns early on 0. UI: grey the button
   when the gate reads 0; the exact non-zero values remain UNKNOWN.
2. Faction effect gate (INFERRED from bonus names): land needs
   `promote_general_in_field` > 0, naval `promote_admiral_at_sea` > 0
   (`pool.rs:354-357` returns `Unsupported("the faction cannot promote in
   the field")` otherwise). UI: show requirement text when the sum is 0.
3. Command state (CONFIRMED): refuse when the force already has a
   General/admiral in command (`pool.rs:358-360`); a unit without a
   character gets one via `0x00990EF0` (`pool.rs:363-378`).

PromotionCost display (CONFIRMED mechanism, value split):

- INFERRED (§12 sandbox trace; was "CONFIRMED static", downgraded in the 2026-10-06 review
  because the decompile was kept only in the sandbox's ignored `target/tmp/gh/`): both executors charge the unit
  class's slot `+0x44` value first through treasury spend
  `0x00BAF500(value, 2)`; the UI builder `0x009ABE00` reads the same slot
  `+0x44` and publishes it as `PromotionCost`. Displayed cost == charged
  cost by construction.
- INFERRED: land slot `+0x44` = `0x008E2770` (static table via
  `0x008E27D0`, guerrilla special case; no rank/distance input; −1 with no
  record). Naval slot `+0x44` = return-0 stub (naval promotion charges 0
  through the treasury spend).
- UNKNOWN: the concrete table value for a promotable unit (see "the promotion cost" below).
- PROVISIONAL (model stand-in): `promote_unit` currently charges the pool hire formula for rank and
  distance (`pool.rs`). Do NOT treat the hire formula as the promotion price in the UI once the probe
  lands; until then the UI shows `CampaignModel::promotion_cost` (the value the promotion itself
  charges) labelled PROVISIONAL.
- **The promotion cost, re-traced by 0-G (2026-10-05, Ghidra read-only, kept in the sandbox's
  ignored `target/tmp/gh/promo_cost.txt`).** The land class's slot +0x44 is `0x008E2770`: it calls
  the class's own vtable slot +0x30 (`0x008D9BB0`, a getter on the global campaign object) and then
  `0x008E27D0(agent type, culture, 0)` — the (agent type, culture) land-unit lookup with the
  `spa_napoleon` guerrilla special case — and returns **`record->field(+0x38)`**, or `-1` when there
  is no record. So it is one static table field with **no rank and no distance input** (the hire
  formula is structurally wrong, as §12 already suspected). The naval class's +0x38 / +0x3c pair is
  `0x008E27B0` / `0x008E2A60`: the same `record->(+0x38)` / `record->(+0x3c)`, `-1` when there is no
  record; its +0x44 is the return-0 stub, so a naval promotion is free.
  **Still UNKNOWN which table that record is** (`0x008E27D0`'s normal path returns a field of an
  object reached through a hash table, `0x00F9C2A0`, and its AGENT_RECORD key is built from the
  "General" string table `0x0145D9E0` — the culture half of the key is not pinned down). Candidate if
  it is the `units` record: column #7 `unknown_3c` (file `@0x3C`, i.e. record `+0x38` under the -4
  shift 0-B documents for the recruitment time), which is a plausible money cost per unit (350 for
  `Gen_Generals_Staff`, 470 for `Gen_Generals_Bodyguard`, 1060 for `Gen_Napoleon`-grade generals,
  1020 for a 2-deck ship). To close it: decompile `0x008F3490` (the table `0x008E27D0` reads) and the
  reader/constructor of the record, and match the `+0x38` field against a named column.
- **The vanilla save fixtures cannot settle it** (checked, 0-G 2026-10-05): the charge is read from
  the DB tables through the unit class at run time, and no save field carries it (nothing in
  `SAVE_COMPAT.md` / `save_check.rs` for a promotion price), so there is nothing for a fixture to
  disagree about. The probe `target/tmp/probes/promotion_probe.cdb.txt` (one user sitting) is still
  the cheapest way to see the number and the tooltip.
- **Wired**: `CampaignUI.CanPromoteUnit(unit card address)` (arity 1 CONFIRMED from `army.lua:692`,
  which asks it with the card's `ItemAddress` and only when `IsGeneralOrAdmiral(card)` is false) and
  the unit rows' `PromotionCost` + `spying_data_level`. The model's new reads:
  `CampaignModel::can_promote_unit` (turn, `promote_general_in_field` / `promote_admiral_at_sea`,
  no General or admiral in command) and `CampaignModel::promotion_cost` (exactly what `promote_unit`
  charges). Tests: `the_promote_gate_and_price_come_from_the_model` (lib),
  `the_interface_gates_and_prices_the_field_promotion` (ntw_sim).
- **The action has no script call.** The player's field promotion in the original is the engine's own
  context-menu action `CCQ_PROMOTE_COMMANDER` (slot 0x4C of the selected unit) — no `.luac` mentions
  "promote" outside Army.lua and the enlist panel (checked over all 497 shipped UI `.luac` files). So
  to make it playable our HUD needs a context menu of its own (0-E) or a harness step; the model
  command and the price are ready.

Event: `CampaignEvent::CharacterPromoted { character }` (`events.rs:72`;
fired at `pool.rs:409`). Script name `CharacterPromoted` (30 handlers in
`export_triggers.lua`). Test on this branch:
`campaign::tests` promotion test (`crates/ntw_sim/src/campaign/tests.rs:2405-2419`).

Note on the other "UP" reading: character unavailability counters
`no_action` (`CHARACTER` #14, `details.rs:78-79`) and `idle_turns` (#15,
`details.rs:80-81`) gate the spy network (§H4), not promotion.

## H3. Spy actions (assassinate / duel / sabotage / spy / steal)

**The target-picking step is CONFIRMED (install 2026-10-05, 0-G) — this closes the "PROVISIONAL
no-op" row of the agents panel above.** The agents panel's buttons (`CampaignUI.Agent*`) ask the
engine to open the **options popup**; a button in that popup asks the engine for the target list and
the **action popup** lists them; picking one calls `Instigate*` (or `SabotageArmy` / `MoveIntoTarget`).

| step | call (all CONFIRMED name + arity) | evidence |
|---|---|---|
| options popup | `OpenAgentActionPopup(action, agent, targets)` / `OpenAgentOptionsPopup(...)` are the **root's** globals (LuaCall), not `CampaignUI` calls | `layout.root.lua` |
| target list | `RequestDuelTargets(agent, target)`, `RequestAssassinationTargets(agent, target)`, `RequestSabotageTargets(agent, target)` → a **list**, `#targets > 0` or the popup is not opened | `agent_options.lua:109 / 118 / 128` |
| a target row | the **address field is `Address`**, not `target`. `character_duel_info_pane.InitCharacter(action, row)` reads `row.Address`, `row.Name`, `row.Chance`, `row.Faction.{Name, Key}`; `sabotage_entry.Initialise(row)` reads those plus `row.IconFilename`, `row.ShortDescription`, `row.Level`, `row.MaxLevel`; `agent_action.lua:21` itself reads `row.Faction.FlagPath` (a **folder**; it appends `/small.tga`). `Utilities.CreateCharacterCard` concatenates **`row.Flag`** unguarded and indexes `info.Attributes.PrimaryAttributePath` | `template.character_duel_info_pane.lua:15`, `template.sabotage_entry.lua:5`, `agent_action.lua:21`, `utilities.lua:107` |
| the action | `InstigateDuel(agent, target)`, `InstigateAssassination(agent, target)`, `InstigateSabotage(agent, target)`, `SabotageArmy(agent, force)`, `MoveIntoTarget(agent, target[, true])`. **The second argument is `row.Address`, the address, not the row** | `character_duel_info_pane.lua:52 / 57`, `sabotage_entry.lua:32`, `agent_action.lua:49 / 54 / 59`, `agent_options.lua:139..164` |
| the options popup | `Initialise(src, target, mask, pct, pct2)` (arity 5). **The mask bits, corrected by 0-E on 2026-10-06:** visit **1**, assassinate **2**, sabotage **4**, embed **8**, research **16**, steal_research **32**, duel **64**, sabotage_army **128**, counterspy **256**. The earlier list here read the array order instead of the shift amounts: the module computes `bit.lshift(1, n)` for n = 0..8 into R[3..11] and then pairs each with a name, so `visit` (the seventh entry) takes the n = 0 mask and `duel` (the sixth) takes n = 6. Only **eight** entries are stored (`SETLIST n=8`), so `counterspy`'s bit is built and dropped and the popup can never show that button although the layout has one. The second argument is a **character** address (`string.find(tostring(x), "CHARACTER")`) | `agent_options.lua:0` pc 13-86, `:34` |

**Wired** in `ntw_script::ui::campaign`: the three `Request*Targets` answer the model's own target
lists (`duel_targets`, `assassination_targets`, `sabotage_targets` — each gate is the model's CONFIRMED
chance function; the candidate sets carry the PROVISIONAL tags of `valid_*` above), and
`InstigateDuel` / `InstigateAssassination` / `InstigateSabotage` / `SabotageArmy` queue the model's
`Duel` / `Assassinate` / `SabotageBuilding` / `SabotageArmy` commands (`agent_action_command`). Test:
`the_agent_action_calls_answer_the_model_and_queue_its_commands`.

**Wired since (0-E round 3, 2026-10-06).** `panel_manager` is a shipped Lua module, not an engine
object (UI_FIDELITY.md §8), so `OpenAgentActionPopup(action, agent, rows)` -- **the root layout's own
global**, `layout.root.lua:1187`, arity 3 -- is reachable from our host. `CampaignUI.AgentRakeAssassinate`,
`AgentRakeSubterfuge` and `AgentGentlemanDuel` (arity 1 each, CONFIRMED) now do exactly what
`agent_options.Assassinate` / `.Sabotage` / `.Duel` do: ask the matching `Request*Targets`, and if the
list is not empty hand it to that global, which opens the `agent_action` panel and fills it. Picking a
row then reaches `Instigate*` and queues the model command. Install test:
`an_agent_action_button_opens_the_target_picker`.

**Four real bugs were found by driving the popup, all in our row builder, all CONFIRMED against the
templates named in the table above:** the address field was called `target` instead of `Address` (so
`CampaignCharacter(row.Address)` got nil and the pane's name, chance and faction name were missing);
`Chance` was absent (`dy_chance .. "%"` raised); `Flag` was absent (`Utilities.CreateCharacterCard`
concatenates it unguarded) and `Attributes` was absent (`CampaignCharacterCard.Initialise` indexes it
unguarded). `Faction.FlagPath` is the flag **folder**, not the picture.

**Still PROVISIONAL / named open items:**
- **The mask is the engine's and only four of its nine bits are derivable.** `agent_options_mask` sets
  `assassinate` / `sabotage` / `duel` when the matching `Request*Targets` list is non-empty, and
  `sabotage_army` from the same two questions the panel asks (`can_sabotage_army` and
  `ValidSabotageArmyTarget`). The other five (`visit` 1, `embed` 8, `research` 16, `steal_research` 32,
  and the dead `counterspy` 256) all end in `CampaignUI.MoveIntoTarget(src, target[, true])`, which
  appears in **no** shipped `.luac`, so its contract is UNKNOWN; their bits are left **out** rather
  than faked, and `MoveIntoTarget` is bound to a logging stub so the route cannot raise.
- `AgentRogueSabotageArmy` is still a no-op with a log line: its only target is an **army** and no
  `Request*Targets` list exists for one (`SabotageArmy(agent, force)` is called straight from the popup
  button, `agent_options.lua:159`), so the original's exe picked the force itself.
- The address-representation gap: `string.find(tostring(target), "CHARACTER")` can never match ours,
  because our addresses are light userdata and `tostring` gives `userdata: 0x...`. The original's
  addresses must stringify with a `CHARACTER` prefix (that is the only reason the test exists). **The
  exact original format is UNKNOWN**, and the contained fix is not small: `entity()` hands scripts
  `Value::LightUserData`, so it cannot carry a `__tostring`; switching to a table address with one
  would break every `==` between addresses unless the tables are interned per `(tag, id)`. Left open.
- `Spy` (spying on a settlement or a force) has **no script call at all** in the shipped UI: the rakes
reach it through the options popup's actions, whose mask bit names the action; the model's `Spy`
command is therefore reachable only once that popup exists.

Chance functions (all return `Option<i32>` percent; `None` = illegal target,
button must grey out; every result clamped 5..95):

- `assassination_chance(model, agent, target)`
  (`crates/ntw_sim/src/campaign/agents.rs:168`; exe `0x009225D0`,
  CONFIRMED, §7). Helpers: `rank` (`agents.rs:115`), `attribute`
  (`agents.rs:65`), `ability` (`agents.rs:89`), `protector`
  (`agents.rs:135`, `0x00A04910`).
- `duel_chance(model, challenger, target, weapon)`
  (`agents.rs:236`; exe `0x00922940`, CONFIRMED). `Weapon::{Pistols,Swords}`
  (`agents.rs:215-220`); AI weapon pick `duel_weapon`
  (`agents.rs:254`, `0x00AAAC30` — today a human target also uses the AI
  rule, PROVISIONAL).
- `spy_chance(model, agent, target)` (`agents.rs:623`; exe `0x00922A70`,
  CONFIRMED). `SpyTarget::{Settlement(RegionId), Force(ForceId)}`
  (`agents.rs:609-614`; orders `0x00907040` / `agent_join_force`
  `0x00905650`). PROVISIONAL: settlement protector =
  `building_protector` (`agents.rs:755`); force C = 0 (exe `0x00B417E0`
  undecoded).
- `army_sabotage_chance(model, agent, force)` (`agents.rs:740`; exe
  `0x00922BA0`, CONFIRMED).
- `building_sabotage_chance(model, agent, region, slot)` (`agents.rs:773`;
  exe `0x00922C90`, CONFIRMED; chain `k` from `building_chains` #2).
- `steal_chance(skill, cost)` (`agents.rs:938`; exe `0x008F32C0`,
  CONFIRMED structure) — passive "ChanceToSteal" display for gentlemen at
  foreign schools; resolution `steal_step` (`agents.rs:957`) in the
  research step.

Commands (walk-then-act on arrival, once per turn via `World::agents_acted`):

- `Assassinate { agent, target }` (`commands.rs:88` → `commands.rs:496` →
  `CampaignModel::assassinate`, `agents.rs:486`).
- `Duel { challenger, target }` (`commands.rs:96` → `commands.rs:497` →
  `CampaignModel::duel`, `agents.rs:549`).
- `SabotageArmy { agent, force }` (`commands.rs:104` → `commands.rs:498` →
  `CampaignModel::sabotage_army`, `agents.rs:802`).
- `SabotageBuilding { agent, region, slot }` (`commands.rs:111` →
  `commands.rs:499` → `CampaignModel::sabotage_building`, `agents.rs:851`).
- `Spy { agent, target }` (`commands.rs:120` → `commands.rs:500` →
  `CampaignModel::spy`, `agents.rs:664`).

Roll/outcomes (CONFIRMED, `0x00920B70`/`0x00920C60`, §7): outcomes
0 critical success / 1 success / 2 failure (escapes) / 3 critical failure
(executed). Duel ending (CONFIRMED, `0x008BFD90`, §12): 0/3 the loser dies
(reason 4 pistols / 7 swords); 1/2 he flees — `flee_duel`
(`agents.rs:576`) sets `fled` (`CHARACTER` #27, `details.rs:82-85`) +
`wounded` (+0x512, `details.rs:86-89`) and walks him to his force or the
region's settlement; `DuelFought` fires for both (`events.rs:140-144`,
exe `0x00934C60`).

Events + messages the UI must consume:

- `AgentActionResolved { agent, target, action, outcome }`
  (`events.rs:283`; `target: None` for buildings). Outcome → message:
  `spy_detected_escape` 246 / `spy_detected_execute` 248 /
  `spy_successful_sabotage` 252 / `spy_successful_army_sabotage` 251 /
  `duel_*_killed` / `*_injured` (message table `0x0145C530`, §7).
- Script events fired at the original's sites (all CONFIRMED, §12):
  `SufferSpyingAttempt` (`events.rs:78`), `SpyingAttemptSuccess`
  (`events.rs:84`), `CharacterFactionSpyAttemptSuccessful` /
  `CharacterFactionSuffersSuccessfulSpyAttempt` (`events.rs:90-99`),
  `EspionageAgentApprehended` (`events.rs:102`),
  `SufferAssassinationAttempt` (`events.rs:108`),
  `AssassinationAttemptSuccess` (`events.rs:113`),
  `CharacterCriticallyFailsAssassination` (`events.rs:119`),
  `SabotageAttemptSuccess` (`events.rs:124`),
  `ArmySabotageAttemptSuccess` / `HarassmentAttemptSuccess`
  (`events.rs:130-138`), `DuelFought` (`events.rs:142`),
  `CharacterBuildsSpyNetwork` (`events.rs:149`, message 250
  `spy_network_established` when `idle_turns` hits exactly 3).
- Detection also exposes the agent to the victim (`expose`,
  `agents.rs:390`, exe `0x008A8B30`); `knows_character` (`agents.rs:75`,
  exe `0x008CE880`) filters target lists.

Data the UI must show per action: eligible target list (guard = chance
`Some`), chance % (same function the popup's "Chance" uses:
`OpenAgentActionPopup` / `OpenDuelPopup`), weapon choice for duels (human
target currently AI-decided — flag in UI), `steal_chance` for gentlemen at
schools, result messages from `AgentActionResolved`, duel wound state
(`wounded`, cleared on arrival).

Ministers (government screen, `UI_FIDELITY.md` diplomacy row): spare pool
= ministers without a post (`spare_ministers`, `family.rs:438`;
CONFIRMED 5 per faction in vanilla saves); `DismissMinister`
(`commands.rs:153` → `commands.rs:504` → `family.rs:468`, exe
`0x008EB9D0`); `AppointMinister` (`commands.rs:159` → `commands.rs:505` →
`family.rs:497`, exe `0x008F3760`). Leader's post excluded from both
(PROVISIONAL interface rule). Note: current `InitialiseGovernmentDetails`
returns an empty `MinisterPool` (PROVISIONAL,
`crates/ntw_script/src/ui/campaign.rs:2300`) — 0-E must wire
`spare_ministers` (`family.rs:438`) + `character_details`
(`campaign.rs:1191`) into it.

## H4. Fog-of-war layer

Reads (all CONFIRMED unless tagged; full model §10):

- `Shroud { explored, visible, hidden, active }`
  (`crates/ntw_sim/src/campaign/visibility.rs:137-147`; exe
  `CAMPAIGN_SHROUD` v1, loader `0x00AFBFC0`, faction `+0x6F8`).
- Per-character radius `sight_radius(c)` (`visibility.rs:154`;
  `CHARACTER` #17, loader `0x00991520`); re-sum `update_sight_radius`
  (`visibility.rs:168`, `0x009CDF10` → `0x009CBC80`,
  `line_of_sight_extension` bonus 132).
- Union `compute_visible(faction)` (`visibility.rs:194`; exe
  `0x00B617E0` over `0x00BB2A00` sources): own + protectorate characters'
  discs (protectorate sharing CONFIRMED, `sight_factions`,
  `visibility.rs:178`), settlements/slots radius 5, owned regions' saved
  shapes, trade segments (PROVISIONAL: loaded lists only), human
  spy-network discs (`network_sight`).
- Timing (CONFIRMED from tree ops, §12): in-turn updates only grow;
  rebuild at the faction's turn END. `refresh_shroud(f, reset)`
  (`visibility.rs:273`): `reset=true` at `FactionEnd`
  (`crates/ntw_sim/src/campaign/turn.rs:367-370`,
  `0x008BD0F0` → `0x00B66EF0(0)`); additive (`false`) at turn start and
  after walks (`turn.rs:324-326`).
- Point test `sees(faction, p)` (`visibility.rs:290`; exe `0x00B7A150`):
  no shroud → true; off → true; else visible && !hidden.
- Hidden/exposed: `stealthy(c)` (`visibility.rs:319`; exe `0x009D1010`,
  118/118 saved flags); refresh `update_hidden` (`visibility.rs:353`,
  exe `0x009D3000` — at creation, turn start, after moves);
  `knows_character` (`agents.rs:75`) / `expose` (`agents.rs:390`);
  `spotting_pass` (`visibility.rs:380`, exe `0x008B4B70`, missionaries
  spot); spy-network fill `spy_network_step` (`visibility.rs:453`, exe
  `0x008F9A00`: `subterfuge` > 0 and `idle_turns` > 2; event
  `CharacterBuildsSpyNetwork` at exactly 3, `turn.rs:309-326`).
- Pathing: hidden obstacles (mode 5) skipped when the searcher has a shroud
  and the cell is not visible, or the obstacle's character is unknown
  (`CHARACTERS_FIDELITY.md` §10; `plan_path` zoc docs).

Fog inputs 0-E needs (per faction with a shroud; only the human keeps one
on a new campaign, INFERRED):

**The script surface (CONFIRMED, install 2026-10-05, 0-G).**
- `CampaignUI.SpyingDataLevelCharacter(address)` / `SpyingDataLevelUnit(address)`, arity 1:
  `layout.root.lua:1093` (the double-click handler) asks the level of the selected entity and opens
  the character / unit details **only when it is at least `SPYING_DATA_LEVEL_ADVANCED`**.
- The levels and the knowledge mask are the shipped `Utilities.lua` globals (CONFIRMED values, read
  on the install): `SPYING_UNIT_DATA_UNKNOWN` 0, `_ICON_KNOWN` 1, `_MEN_KNOWN` 2, `_GUNS_KNOWN` 4,
  `_XP_KNOWN` 8, `_OWNED` 15; `SPYING_DATA_LEVEL_INVALID` -1, `_PASSIVE` 0, `_BASIC` 1, `_ADVANCED`
  2, `_OWNED` 3. So they are script-defined, not engine constants, and our HUD gets them for free
  with `utilities.luac` loaded. They were also the values the `unit_entry` comment already claimed.
- `game_interface:show_shroud(on)` (26 call sites in the shipped campaign scripts) and
  `:unveil_black_shroud(on)` (15), one boolean each -- the only fog controls the scripts have.

**Wired** in `ntw_script`:
- `SpyingDataLevelCharacter` / `SpyingDataLevelUnit` answer from the model's own knowledge
  (`spying_level_character` / `spying_level_unit`): own faction `OWNED`; a foreign character the
  faction knows (`agents::knows_character`) and whose cell the shroud has visible
  (`CampaignModel::sees`) `ADVANCED`; known but under the shroud `BASIC`; unknown `PASSIVE`; not a
  character / unit `INVALID`. INFERRED mapping, every step from a CONFIRMED model rule. Test:
  `the_spying_data_levels_follow_the_models_sight`.
- The character cards' and unit rows' `spying_data_level` + `knowledge_mask` come from the same
  answer instead of the old constant 3 / 15.
- `RetrieveVisibleEnitityDetails` leaves out a settlement the player's shroud has not seen (the
  labels; INFERRED), and `RetrieveFactionMilitaryForceLists` leaves out a foreign force whose
  commander the player does not know. `RetrieveFactionRegionList` lists only the player's own
  regions, so it needed nothing.
- Round 3 changed the label filter from `CampaignModel::sees` to `CampaignModel::knows` (visible
  OR explored), so a settlement seen once keeps its label. **INFERRED** (2026-10-06 review): the
  exe's label handler is not traced and nothing was checked in game; it is in the in-game list.
  Since the review, `fog_state == Visible` is exactly `sees`, so `knows` is never false where
  `sees` is true (before, a faction with no shroud "knew" nothing off the grid).
- **Review finding for the binding owner (not fixed here, `campaign.rs` is 0-E's):**
  `spying_level_unit` answers `PASSIVE` for a unit whose force has no commander, even when the
  force is the player's own; it should answer `OWNED` when `owner.faction == human` before
  looking at the commander.
- `game_interface:show_shroud(on)` sets the local faction's `Shroud::active`;
  `:unveil_black_shroud(true)` marks every cell of the sight grid explored (INFERRED meaning; it
  takes no faction, and only the human keeps a shroud).

**Still open:** drawing the fog on the map (the dimmed terrain over the unexplored cells) is in
`crates/napoleon` (`campaign/scene.rs`), which this sandbox cannot build; the model side is ready
(`World::shrouds`, `CampaignModel::sees`) and the label layer already respects it.

1. `explored` set → dimmed/remembered terrain; `visible` set → fully drawn;
   `hidden` (#2, empty in every save, UNKNOWN filler) masks even visible
   cells; `active=false` or no shroud → draw all.
2. `sight_radius` per character for tooltips/debug; `network_sight` discs
   for the spy-network overlay.
3. `knows_character` filter for enemy pieces: draw hidden/unknown
   characters only after `expose` (spotting, successful spy crit, detected
   attempts). Duel-fled characters (`fled`, #27) never hide.
4. Existing UI seams to reuse: `RetrieveVisibleEnitityDetails`
   (`campaign.rs:2073`), lists panel (`campaign.rs:2145-2244`), character
   cards `character_details` (`campaign.rs:1191`, exe `0x009AD250`).

## H5. Settlement tabs: naval recruitment / infrastructure / agents (mapping for 0-E, read-only)

Layout mapping only (this section). No Rust changes. Sources, all read-only:
`analysis/worker3/lua_api.txt` line numbers below; `analysis/campaign/CAMPAIGN_UI.md`
(tab order/keys); the install's `data/UI/Templates/uied.templates` entry names
(parsed read-only, 126 entries); 0-E panel code
(`..\NR-sb-0e\crates\ntw_script\src\ui\campaign.rs`, NOT edited).
Model cites are `file:line` on this branch (`work/sandbox/0g-characters`).

Tab keys (CONFIRMED, CAMPAIGN_UI.md §Review panel): `army_tab, navy_tab,
recruitment_tab, naval_recruitment_tab, agents_tab, infrastructure_tab, siege_tab,
construction_tab`. Settlement panel order (CONFIRMED `FUN_0099A200`): construction,
recruitment (region can recruit), infrastructure (fort/port), army (garrison), agents.
0-E today lists only Construction (+Recruitment, +Army) for settlements
(`tabs_for`, 0-E `campaign.rs:2849-2876`); its `Tab` enum has no
naval-recruitment / infrastructure / siege variants (0-E `campaign.rs:70-102`).

### T1. Naval recruitment tab (`naval_recruitment_tab`)

- Generator: UNKNOWN name. `recruitment_manager` exposes only
  `GenerateRecruitmentPanel` (`lua_api.txt:5002`); no `GenerateNaval*` symbol exists.
  INFERRED: the naval tab reuses `GenerateRecruitmentPanel` with naval capacity.
- **Ghidra trace (CONFIRMED, read-only from `NR-sb-ghidra-0g`):**
  - Settlement panel tab builder `FUN_0099A200` (settlement) iterates tabs in order:
    `construction_tab`, `recruitment_tab` (if `recruitment_points(region, false) > 0`),
    `infrastructure_tab` (if fort/port), `army_tab` (if garrison), `agents_tab`.
  - The `naval_recruitment_tab` is NOT added by `FUN_0099A200` for settlements.
    It appears only on the **character panel** (`FUN_00985F40`) for admirals/captains
    (where `GenerateNavyPanel` is called, `lua_api.txt:1000,3061`).
  - No caller of `recruitment_manager.GenerateRecruitmentPanel` passes a "naval" flag
    in the exe; the generator is invoked once per recruitment tab with the region's
    land capacity. The naval tab's generator name remains UNKNOWN.
  - `CampaignShipCard` template exists in `uied.templates` (parsed entry), but no
    script reference to it was found in the lua_api.txt dump — it is likely instantiated
    by the UNKNOWN naval generator, not by `GenerateRecruitmentPanel`.
  - Naval-only Lua globals (`CampaignUI.NavalUnitLimit`, `NavalRefresh`,
    `UpdateNavalInformation`, `buttonset_naval`, `button_close_naval`) are
    registered in the exe's CampaignUI binding table but are not called from any
    known settlement-panel script path.
- Layout (CONFIRMED names): `RecruitmentCard` + `CampaignShipCard` templates and
  `recruitments_list_box` / `recruitment_list_box` lists, `requires_building_list_box`
  / `requires_tech_list_box`, `g_recruitment*` globals (`lua_api.txt:1868-1870`),
  `GenerateRecruitmentCard(s)` / `GenerateEnemyRecruitmentCard` (`:999-1002`),
  `SetupRecruitmentSlots`, `SetAsRecruitmentType` (`:1550,:1436`); naval-only:
  `CampaignUI.NavalUnitLimit` (`:2612`), `NavalRefresh` (`:1212`),
  `UpdateNavalInformation` (`:1734`), `buttonset_naval:Find` (`:3872`),
  `button_close_naval:Address` (`:3846`); queue cards cancel via
  `CampaignUI.CancelRecruitment` (`:2518`). (`siege_tab` sibling, out of scope:
  `RecruitmentCard_siege`, `SiegeEquipmentCard`, `siege_equipment_counter` templates.)
- Model reads the tab needs:
  - Capacity: `CampaignModel::recruitment_points(region, naval=true)`
    (`crates/ntw_sim/src/campaign/commands.rs:921`) — per-port
    `naval_recruitment_points` of the port's own building, summed
    (`commands.rs:930-936`; bonus id `effects.rs:113`).
  - Offer list: `recruitable_units(region)` (`commands.rs:952`) — includes port
    buildings' `units_allowed`; ship rows carry `is_naval` (`rules.rs:30-31`).
  - Queue: `Region.recruitment_queue` (`world.rs:734`); land/naval lanes split at
    `turn.rs:430,471`; `RecruitUnit`/`CancelRecruitment` → `recruit`
    (`commands.rs:966`) / `cancel_recruitment` (`commands.rs:1000`).
  - Finished ships join `Region.fleet` (`world.rs:741-743`); `HireAdmiral` into that
    fleet is §H1 (`pool.rs:293`).
- **Status:** Generator name UNKNOWN (not CONFIRMED). No code change until CONFIRMED.
  TODO (0-E): once generator is CONFIRMED, add the tab (key + generator) for
  settlements with `recruitment_points(r, true) > 0`; pass naval capacity and ship cards.

### T2. Infrastructure tab (`infrastructure_tab`)

- Generator (CONFIRMED name): `construction_manager.GenerateFortConstructionPanel`
  (`lua_api.txt:3943`). Info-table shape UNKNOWN.
- Layout (CONFIRMED names): fort script surface `CampaignUI.FortDetails/FortEffects`
  (`:2562-2563`), `BuildFort` (`:2491`), `UpgradeFort` (`:2730`), `RepairFort`
  (`:2655`), `DemolishFort` (`:2540`), `CancelFortRepair` (`:2516`),
  `CancelUpgradeFort` (`:2522`), `AbleToBuildFort` (`:727`), `ShowFortInfo`
  (`:1578`), `army_manager.BuildFort` (`:3773`); panel buttons
  `construction_manager.RepairCurrentSelection/DemolishCurrentSelection`
  (`:3946,:3941`); templates `building_constructed_entry`,
  `construction_item_tooltip`, `construction_list_box`,
  `building_info_build_entry/generic_entry` (uied.templates).
- Model reads the tab needs:
  - Walls/fort: `Region.fortification` (`world.rs:691-697`) via
    `FORTIFICATION_SLOT` (`world.rs:748`) / `building_at` (`world.rs:752`); build,
    repair and cancel already accept it (`commands.rs:1023`, `turn.rs:452`,
    `tests.rs:2102-2117`); capture damages it (`capture.rs:166-195`).
  - Roads: `Region.road` (`world.rs:688-690`); road slot = `ConstructionItem.slot
    == None` (`world.rs:643-644`).
  - PROVISIONAL gaps (0-G, this branch): NO separate `BuildFort` / `UpgradeFort`
    model commands are needed — both are `ConstructBuilding` with
    `slot: Some(FORTIFICATION_SLOT)` (CONFIRMED working, `tests.rs:2094-2118`);
    `RepairFort` = `RepairBuilding` + `FORTIFICATION_SLOT`, `CancelFortRepair` /
    `CancelUpgradeFort` = `CancelConstruction` + `Some(FORTIFICATION_SLOT)` (CONFIRMED
    by code reading). The only new command is `DemolishBuilding { region, slot }`
    (`capture.rs:can_demolish` / `demolish_building`), which also serves `DemolishFort`
    via `FORTIFICATION_SLOT` — one model command for the exe's two queued ids
    (`0x84` building / `0x89` fort, CONFIRMED below); no refund (PROVISIONAL).
- Trace (CONFIRMED, 0g Ghidra copy read-only, `MemSearch` + `SaveDecomp`): the
  CampaignUI names live at `0x01369784` (`DemolishBuilding`), `0x01369A00`
  (`DemolishFort`), `0x0136974C` (`CanDemolishBuilding`), registered at
  `0x00428A95` / `0x00428AB5` / `0x004284B5` with handlers `0x009E2380` /
  `0x009E23D0` / `0x009E0930` (struct decoded by `RepairBuilding` calibration:
  second push = handler, `0x009F1300` matches the CONFIRMED handler). Chains:
  `DemolishBuilding 0x009E2380 → 0x009B9590 → 0x00953A20 → 0x0090FA10` → queue
  `0x008FECC0` id `0x84` (same skeleton + queue as `CancelConstruction`
  `0x009E0F10 → 0x009B7AA0 → 0x009539C0 → 0x0090FA10`/`0x84` + flag call
  `0x005AB750`); `DemolishFort 0x009E23D0 → 0x009BA250 → 0x00953B20 → 0x0090F930`
  → queue `0x008FECC0` id `0x89` (slotless, fort implicit; 9 engine callers);
  `CanDemolishBuilding 0x009E0930 → 0x009B7920` = slot resolve `0x009BB6E0`
  (shared with cancel/repair) && not `settlement_road` (`0x00A8B970`) &&
  `!0x00A91FC0` (shared with options filter `0x00B43300` + can-repair `0x00B1A6B0`,
  INFERRED slot-free check) && selected item present. INFERRED: `0x00A91FC0` ≈
  slot held/free of pending work. UNKNOWN: demolish resolution refund/timing;
  `GenerateFortConstructionPanel` info-table shape (untouched).
- TODO (0-E): add the tab for fort/port regions; wire `FortDetails` to
  `fortification` + `road`. Script-host lead (0-E owns `campaign.rs`): bind
  `DemolishBuilding(level, slot_key)` → `DemolishBuilding { region, slot }` and
  `DemolishFort` → same with `FORTIFICATION_SLOT` (mirroring the
  `RepairBuilding` binding); flip `CanDemolishBuilding` from `false` to
  `can_demolish`. `BuildFort`/`UpgradeFort` need no new path: route to
  `ConstructBuilding` with `Some(FORTIFICATION_SLOT)` like `BeginConstruction`/
  `BeginUpgrade`; same for `RepairFort` / `CancelFortRepair` / `CancelUpgradeFort`.

### T3. Agents tab (`agents_tab`, settlement panel last)

- Generator (CONFIRMED name): `agents_manager.GenerateAgentsPanel` (`lua_api.txt:3736`).
- Layout (CONFIRMED names): `g_agent_cardgroup:DestroyChildren` (`:4108`),
  `g_agents_buttons` visibility (`:4109-4110`), `ShowAgents/ShowAgentButtons`
  (`:1558-1559`), `GenerateAgentCards` (`:994`), `AgentCardPosition` (`:752`),
  `SelectAgentCard` (`:1398`), `AgentsPanelActive` (`:753`); templates
  `CampaignCharacterCard`, `ReviewPanelAgentHitbox`, `row_template_agent`,
  `character_duel_info_pane`, `Duellist`; per-kind buttons
  `GentlemanDuel/RakeAssassinate/RakeSubterfuge/ResearcherSteal/RogueSabotageArmy`
  (`:3737-3741`) → `CampaignUI.AgentGentlemanDuel/AgentRakeAssassinate/
  AgentRakeSubterfuge/AgentRogueSabotageArmy` (`:2475-2478`) +
  `AgentEmbarkOrDisembark` / `CanAgentEmbarkOrDisembark` (`:2474,:2499`);
  selection notify `AgentCardSelectionChanged` (`:2473`).
- **Ghidra trace for show-condition (CONFIRMED, read-only from `NR-sb-ghidra-0g`):**
  - Settlement panel `FUN_0099A200` builds tabs in this sequence:
    1. `construction_tab` (always)
    2. `recruitment_tab` — condition: calls `FUN_00B44DA0(region)` which returns
       `recruitment_points(region, naval=false) > 0` (land capacity).
    3. `infrastructure_tab` — condition: region has a fortification slot
       (`building_at(region, FORTIFICATION_SLOT) != null`) OR a port slot
       (region has a port building with `naval_recruitment_points > 0`).
    4. `army_tab` — condition: `region.garrison != null` AND
       `force.units` not empty.
    5. `agents_tab` — condition: **UNKNOWN**. The exe does not emit a clear
       boolean check before adding this tab in `FUN_0099A200`. The tab is
       unconditionally appended after `army_tab` in the decompiled flow, but the
       original UI may grey it (state `RPTS_GREYED = 0`) when no agents are
       garrisoned. No `CanShowAgentsTab` or similar predicate was found.
  - `garrisoned_in == region` characters (agents) are counted at
    `world.rs:799-801`; the model can answer "are there agents here?".
- Model reads the tab needs:
  - Roster: characters with `garrisoned_in == region` (`world.rs:799-801`) with
    kind/faction/position/action points (`world.rs:776-798`); row shape can reuse
    `RetrieveFactionAgentsList` fields (0-E `campaign.rs:2218-2244`).
  - Actions: chance functions + walk-then-act commands, §H3
    (`agents.rs:168,236,623,740,773,938`; `commands.rs:88-120`); result messages via
    `AgentActionResolved` (`events.rs:283`); `knows_character` (`agents.rs:75`)
    filters enemy cards.
- **Status:** Show-condition UNKNOWN (not CONFIRMED). Suggested provisional
  condition: `model.world.characters.values().any(|c| c.garrisoned_in == Some(region) && c.is_agent())`.
  No code change until CONFIRMED. TODO (0-E): add the tab with the verified
  condition; pass the garrisoned roster; action buttons reuse §H3 commands.

## Verification

- Notes-only: no `.rs` touched; `git status` shows only this file plus the
  one-line pointer added to `CHARACTERS_FIDELITY.md` (§12 sandbox note).
- **Ghidra trace artifacts (read-only, not committed):**
  - Settlement panel tab builder: `FUN_0099A200` decompiled flow saved to
    `target/tmp/gh/settlement_tabs.txt` (generated by this sandbox's
    `analysis/fidelity/ghidra_scripts/settlement_tabs.java`).
  - Naval recruitment generator search: no `GenerateNaval*` symbol in exe's
    CampaignUI binding table (`0x00998C50` registrar); only
    `GenerateNavyPanel` (character panel) and `GenerateRecruitmentPanel`
    (settlement land recruitment) exist.
  - `CampaignShipCard` template: entry 47 in `uied.templates` (parsed by
    `ntw_formats::src::ui_templates.rs`), no script reference in lua_api.txt.
  - Agents tab show-condition: `FUN_0099A200` appends `agents_tab` without a
    visible predicate; tab state likely set to `RPTS_GREYED` (0) when empty.
- Tests: existing unit tests cover every hook (`tests.rs:1484-1540`
  chances, `:2401-2419` hire/promote, `:2428-2454` visible rebuild,
  `:2463` spy network, `:2530-2537` `CharacterCreated`); run the
  sandbox-scoped unit suite before handing over (no `napoleon` build).
- NO PUSH performed (hard rule); diff stays in `work/sandbox/0g-characters`.

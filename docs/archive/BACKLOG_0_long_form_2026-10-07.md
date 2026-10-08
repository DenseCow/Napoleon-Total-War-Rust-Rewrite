# BACKLOG §0 — long form as of 2026-10-07

Replaced in docs/BACKLOG.md by per-item checkboxes. Kept verbatim for the round history and evidence pointers.

## 0. Cross-cutting fidelity
**Status: NOT complete** (updated 2026-10-04 ~12:30, at the weekly usage stop; resumes Oct 7). Strict markers as in §1. Split into work areas, one worker slot each; every
area keeps a notes file with a resolved/open table (question, answer, CONFIRMED/INFERRED/UNKNOWN, code).
- [ ] **PARTLY DONE: 0-A battle rules** (paused) [analysis/fidelity/BATTLE_FIDELITY.md]. Done: reload, morale (timers, sub-evaluators,
      casualty-ratio ring buffers, start values), unit attributes, strength potentials and card terms, soldier hit points and the death
      dispatch, chance-to-hit (control, visibility, angle judgement, woods cover, accuracy), the cartridge pool, charges only at impact,
      melee exchange timing, fatigue effects (speed/charge/control/attack), ground speed columns and slope, battle clock and time-out,
      generals, reinforcement entry, units leaving the map, battle Lua scripts in live battles (0 unknown calls; TUT_Land runs),
      skirmish, deployables (placement, contact kills, cover), building garrisons, special abilities, shot types, map-preset weather.
      which soldiers fire per firing drill (S3, round 17: CONFIRMED in Ghidra, `volley_plan`; runtime confirmation in the battle probe), campaign-battle weather pick (ported, not connected until campaign battles start from the map).
      Sandbox port (§57-§58, merged ffe3f32): melee has no experience term (CONFIRMED negative), kv_rules slot = list position,
      `unit+0xD48` is the experience level and drives the waver/rout timers and the fatigue bonus, land/naval experience bonus
      tables decoded, battle-file `unit_experience` reaches every unit, the `unit_scale` decoder (exe default 0.75).
      **Open:** who writes `unit+0xD48` (write watchpoint), where `unit_scale` is applied (reader UNKNOWN, not wired), formation
      radius +0x670 (probe written), garrison cap +0x6C, the `--battle --screenshot` "closed channel" capture. The Austerlitz
      "window closes at 60 s" theory is REFUTED (§58: duration is 2400 s).
- [ ] **PARTLY DONE: 0-B campaign rules** (round 11 merged b18e35e; resume state in CAMPAIGN_FIDELITY.md "Where I am") [CAMPAIGN_FIDELITY.md]. Done: campaign variables, taxes, GDP and town wealth
      (exact, 238 regions), trade (exact in all 9 original saves; nodes, routes, blockades, supply split 358/362, prices), bankruptcy,
      autoresolve (no retreat: CONFIRMED), public order (1180/1184 classes), recruitment and construction queues (CONFIRMED),
      research (rate CONFIRMED), effects wired in, region adjacency, sea-route cap, capture choice (occupy/loot/liberate) and repairs,
      End Turn speed, diplomacy rules, turn order (CONFIRMED), religion conversion, fortification slots, naval autoresolve (ported),
      computed diplomacy factors (506/506), allies called into wars, treaty money, fortification build/upgrade/repair, militia rule
      (CONFIRMED); round 11 (debugger session 2026-10-04): naval inputs from unit_stats_naval and ship captures (CONFIRMED on 71 logged ships), France leader factor (506/506), spa religion drift (missionary rank + theatre zeal, 341/341), desertion gate (bankrupt-turn counter), theatres and research admin mods (no-ops), region recompute chain modifiers (growth 69-72/72), construction cost tech chains (328/330). Round 12 sandbox: the government drift on a government change ported (0x00B1B5A0; drift a turn INFERRED). Rounds 14-15 (sandbox, ported f5e302a): the 3 eur growth misses fixed (governing faction + tax-exempt guard; growth 72/72 eur, 31/31 spa), recruited unit size CONFIRMED (3174/3174 save units) and used for new recruits, demolish command, fort options. Experience-adjusted recruitment cost ported (709dad8, 0x00ED49A0; recruitment use INFERRED, upkeep untouched). **Open:** region transfer in deals (TransferRegion rejected: needs 0x00B449F0 flags), recruitment details (0x00AECEE0/0x00AED220), peace terms (regions/techs as deal items, 0x00B449F0), the importer limit (0x00BB5730), the 4 desertion-exempt unit classes, naval PROVISIONAL details (capture share weighting, captured crew base, gun counts of 8 ship models: needs a probe). The AI's own choices are §6.
- [ ] **PARTLY DONE: 0-C middleware** (worker moved on) [MIDDLEWARE_VERIFY.md, BINK.md]. Done: Miles mixing rules and loudness
      (CONFIRMED vs the exe), game-speed sfx rule, UI sound kinds, Bink intro order/once-per-start/full-screen sizing (intro on by
      default). Sandbox rounds 9-13 ported (06d6a7e): the battle animation ACTION table contract (slot names, has-clip table,
      pose-to-state map, stance ranges, resolver) CONFIRMED against the dumps; cue dispatch closed exe-side. **Open:** the sound bank
      query, the anim cue dispatch (cue→slot INFERRED; see MIDDLEWARE_VERIFY.md for what round 9 settled), the movie skip rule, headphones multiplier.
      SpeedTree leftovers are §2.
- [ ] **PARTLY DONE: 0-D units, animation, terrain, trees** [UNITS_TERRAIN_FIDELITY.md]. Done: animation slot table,
      per-man clip selection, gait levels, sim-driven fire/reload/melee/death/knockdown clips, unit and horse LOD switching,
      groupformations.bin and the default deployment (incl. the guerrilla group), tree list format, heightfield normalisation,
      training-level order. Round 7 (sandbox, data first): the **tree scale byte is a relative scale** and decodes as
      `u8 / 128` clamped to 0.5 ..= 1.4 (bytes are exactly 63..=253 over 3.84 M instances); the **flag cloth's geometry,
      pole and bone** are CONFIRMED (`ntw_formats::verlet`, the pole is the bearer's personal equipment on bone 3 =
      `Weapon3`); the idle-stance test's value set is closed and both the training level and
      `man_animation_type` are refuted from data. Round 8: **the standard bearer's flag is DRAWN** — the verlet solve
      (`ntw_sim::battle::cloth`, round 9) from the shipped `.logic`, the plain-text `*.tai` atlas reader
      (`ntw_formats::texture_atlas`), one StandardBearer figure per unit and his flag pole's cloth
      (`napoleon::battle::flag`). The frame offset `(0, -0.967, -0.075)` and the six **zero-rest-length welds** of the
      hoist are CONFIRMED from the data; the sail hangs 1.43 .. 2.58 m up the pole by plain arithmetic on the exe's own
      `Weapon3` frame-0 matrix. **Round 9:** the solve moved to `ntw_sim::battle::cloth` (the reader stays in
      `ntw_formats::cloth`; `ntw_sim` still has **zero** dependencies and the install measurements are
      byte-identical before and after) and **`slots_art` / `slots_templates_models` now load** —
      CONFIRMED: `slots_art` is 12 slot types × 6 cultures, its #4/#6 are keys of `slots_templates_models`
      (54/54), and the region's fort model is `fFort` level *n* -> `fort_lvl<n+1>_blend.rigid_model`
      (3 levels, exactly 3 models). **CORRECTION for 0-E:** `0x00B42B90` is the **settlement's**
      `_slot_fortifications_lvl` builder, not the region fort's. Round 9 also **closed the 39 dependent
      factions' flag key**: it is the last segment of **`factions.flag_path`**, a key of `flags.tai` for
      **77 of 77** factions (not the faction key, and not `faction_group`, which resolves only 1 of the 12
      rebel groups). **Round 10:** the **settlement's fortification mesh is now DRAWN**
      (`napoleon::campaign::scene::{SettlementWalls, sync_walls}`) — the file level is proved to be the
      `sFortifications` **chain level + 1** by **23** vertex ladders that all satisfy `_lvl0 < _lvl1 < _lvl2`
      and share the plain city's three textures, and the mesh swaps when walls are built or demolished
      (INFERRED additive, not looked at yet; **no shipped start position has walls**, so nothing changes
      until the player builds some). Round 10 also **refuted the rival flag columns by count** (only
      `flag_path` resolves all 39 dependents: `model_faction` 22, `rebel_flag_path` 26,
      `republic_flag_path` 10, `faction_group` 7, `subculture` 0) and **corrected round 9's target for
      the battle flag**: `0x01227BD0` is the flag system's *registry lookup*, has one caller, and that
      caller hands it the literal `"default"`; everything around it is **campaign-map** flag art (§3.2a).
      **Open:** the enum behind `unit->+0x1B0` — round 8 read its five consumer predicates
      through, corrected the value set to `{0,1,2,3,5,6,7}` and identified the reciprocal-parent back link at `+0x214`,
      but **could not name it**: CLOSED-ATTEMPTED (§9.8). Also open: the flag's wind speed and the solver's substep /
      iteration / damping constants (PROVISIONAL, targets named), **what string names a flag record** (a
      field at `+0x38`; the `flag_` prefix is completed in only two places in the exe and neither is a
      per-faction lookup — target: a write watchpoint on the flag record array, which needs the
      debugger), whether the standard bearer's cloth reads `flags.tai` at all (UNKNOWN), and the exe's own
      limp-flag shape.
- [ ] **PARTLY DONE: 0-E campaign map, campaign UI, front end** (settlement panel merged 2026-10-04; resume from UI_FIDELITY.md "Where I am") [UI_FIDELITY.md, CAMPAIGN_MAP.md §10]. Done: UI scale for
      720-high windows (CONFIRMED), radar map, government, technology, objectives, lists and diplomacy screens, building browser
      details, Load Game page, tooltips, credits, text entry, custom battle setup (settings, armies, save/load, start in our engine),
      campaign trees, Bezier splines, river ribbons, coastal surf, close-up supertexture, town/port facing, the settlement panel (every slot's upgrades, build/cancel/repair, recruit/cancel, faction/culture building permission 0x008BE7F0, restricted buildings kept in our saves; proof modes `--campaign-ui-proof` etc.), the building browser tree, the capture screen. Sandbox port (709dad8): the negotiation object (20 methods; region-transfer rows deferred), the naval and infrastructure/fort settlement tabs, demolish (model + button), recruitment cards priced with experience. **`panel_manager` is NOT missing (0-E round N+2): it is the shipped `ui\panelmanager.luac`, which the root layout requires itself, so it always ran; what blocked the hire was our own trait row missing `Effects`/`AttributeEffects` (fixed) and `Component.Call("IsDragged")` answering nil, which killed every unit/agent card's left click in both HUDs (fixed). The `enlist_commander` panel now opens and lists the pool; the eight globals, `OpenPanel(panel_name, show_data, init, ...)`, `ClosePanel(name|address)` and `IsPanelOpen` -> (component, side, is_topmost) are CONFIRMED in UI_FIDELITY.md. **The panel's POSITION was never a bug (0-E round N+3): `layout.root.lua:790` calls `huds.RegisterHud(g_hud, true)` -- a component and a boolean, not a width and a height -- and `ui\huds.luac` (the shipped module) fills `h_width`/`h_height` from `UIComponent(hud):Bounds()` and `s_width`/`s_height` from `hud:Parent("root"):Dimensions()`. On 1280x960 that is 1280/241/1280/960, and `Huds.MoveRelativeToHUD`'s centre branch puts the 624x720 panel at (328, -1) -- `TruncToInt(v) = v - (v % 1)` is `floor`, so the one-pixel overhang is the original's own arithmetic. `UIComponent:Bounds` is CONFIRMED the size like `Dimensions`.** **The agent action popups now work end to end (0-E round N+3): `OpenAgentActionPopup`/`OpenAgentOptionsPopup` are the ROOT LAYOUT's own globals (`layout.root.lua:1187` arity 3, `:1191` arity 5), reached by `root:LuaCall` -- so "no caller in any shipped .luac" was beside the point, and the engine's half of that call is our `CampaignUI.Agent*` binding. Assassinate / Sabotage / Duel now ask `Request*Targets` and open the `agent_action` picker, whose rows reach `Instigate*` and queue the model command. Driving it found four CONFIRMED contract bugs in our row data (`Address` not `target`; missing `Chance`, `Flag`, `Attributes`; `Faction.FlagPath` is a folder).** **Open:** diplomacy negotiation playability and region exchange, campaign save naming, the agent **options** popup's other five actions -- `visit`/`embed`/`research`/`steal_research`/`counterspy` all end in `CampaignUI.MoveIntoTarget`, which is in **no** shipped `.luac`, so their mask bits are deliberately left unset and `MoveIntoTarget` is a logging stub; `AgentRogueSabotageArmy` (its only target is an army and no target list exists for one). **The address representation is CLOSED (0-E round N+4): the original's address is a `UTILITYDLL::LUA::Pointer<T>` userdata with a metatable whose only fields are `type`, `__tostring` and `__eq` (53 registering call sites of `0x0105AE10`), `__eq` compares the wrapped pointers so `==` is identity, and `__tostring` (`0x01058F60`) is `sprintf("%s (0x0%x)", metatable.type, pointer)` -- where `metatable.type` is the interned C++ signature string of the registration, e.g. `...operator <<<const class EMPIRECAMPAIGN::CHARACTER>(const class UTILITYDLL::LUA::Pointer<class EMPIRECAMPAIGN::CHARACTER const > &)`, which contains `CHARACTER`. So `string.find(tostring(target), "CHARACTER")` DOES match in the original, and ours does too: addresses are now interned tables (so `==` stays identity) carrying that exact string, and `agent_options.lua:34`'s branch is taken. Full evidence and the interning blast radius in UI_FIDELITY.md 9; 0-G no longer needs to read 9.8 before touching `entity()`, but should read 9.9 before `MoveIntoTarget` -- that stub is NOT 0-B's, because `CampaignCommand::MoveCharacter` already exists and only the engine-chosen *target* is untraceable.** The agents settlement tab's panel (its hover tooltip error is fixed; the tab itself is listed), fort as its own selection, BuildFort/UpgradeFort levels (PROVISIONAL), animated sea/rivers, far-view tree models,
      resource-slot models, forts, textured borders (**the round-3 lead is REFUTED, 0-E round N+4**: `testdata\westerneuborders.rigid_mesh` is now parsed by `ntw_formats::campaign_map::BorderRibbon` -- `u16 | u32 count (586) | 586 × 56-byte version-0 rigid vertex | u32 count (1758) | 1758 × u32`, which closes on the byte at 39,858 -- but it has **no UVs at all** (both texture coordinates are exactly 0.0 on every vertex, `y` is 0, the normal is exactly `(0, 1, 0)`) and its positions are **not** map display units (x spans 5.29, z spans 4.04). Its index list is a fan from vertex 0, not a strip of pairs, so no constant width is readable either. **The border's SHADER is now named (0-E round N+5): the exe's material declaration table at VA `0x01418500` pairs `supertexture_border` with `SupertextureTile.fx`, `RigidModels/CampaignBorders/Textures/border_diffuse` and TWO colour uniforms, `g_border_colour_a` and `g_border_colour_b` -- so the border is a supertexture ribbon tinted per side, and only the width and the `V` mapping remain the exe's job. CAMPAIGN_MAP.md 10.3a-3 has it. The same table CORRECTS a recorded negative: `rigidmodels\campaignroads\textures\{dirt,primitive}_diffuse.dds` ship, so roads are textured too (all four materials named: primitive / dirt / stone / tarmac), not untextured as 10.3a said. And `fx\campaignriver.fx` ships, so the river's two scroll rates are CONFIRMED at 0.02 and 0.05 texture units per second** (`tile_factor` and `timescale` are shader-local `= 1`) -- the whole remaining river gap is one UV offset in a material of our own, `StandardMaterial` being unable to do it. **The campaign sea is still UNKNOWN**: no campaign sea shader appears in that table, and `Grid.fx`'s two-layer scrolling water is the battle terrain grid, so the named lead is the campaign scene loader, not the material declarations. No geometry was invented; round 4's refutation stands).
- [x] **0-F effects system** (merged through 0-B, 8f99a2a): techs, buildings, traits (CONFIRMED level rule), ancillaries, government,
      ministers, difficulty; query API in EFFECTS_FIDELITY.md §4, wired into the economy.
- [ ] **PARTLY DONE: 0-G characters and agents** (round 8 closed every residual but one; CHARACTERS_FIDELITY.md §11-§12) [CHARACTERS_FIDELITY.md]. Done:
      trait and ancillary gain, natural death, the commander succession order (CONFIRMED), royal family and succession, vacated posts,
      minister dismissal/appointment and the spare pool, agent action rolls and chances (CONFIRMED), assassination, duels, sabotage,
      technology stealing, diplomatic reactions, campaign sight and shroud (CONFIRMED), hidden characters and the exposed list, spying,
      the stealth test (118/118 saved hidden flags) and spotting pass, recruitment pools and HireGeneral (cost CONFIRMED), turn-end
      counters, CharacterCreated/CharacterPromoted events; conversion is 0-B's religion model (nothing separate to port).
      Round 8 (CONFIRMED): shroud rebuilt at the faction turn end, spy networks (3 idle turns, CharacterBuildsSpyNetwork), historical characters in the pool, HireGeneral into an army, HireAdmiral, PromoteUnit and the CharacterPromoted point (field promotion, not hiring), duel loser flees wounded, +0x52C rebuild, female leaders, forced-success flags, all espionage/duel script events.
      Sandbox round (0-G, 2026-10-05): the four character UI hooks are **wired** (CHARACTER_UI_HOOKS.md "What 0-G
      wired"): `CanRecruitCommander` / `AvailableCommandersForRecruitment` / `PromoteUnits` (the hire; CONFIRMED from
      the install's `enlist_commander.luac`), `CanPromoteUnit` + the unit rows' `PromotionCost`,
      `RequestDuelTargets` / `RequestAssassinationTargets` / `RequestSabotageTargets` / `InstigateDuel` /
      `InstigateAssassination` / `InstigateSabotage` / `SabotageArmy` (the target-picking step found on the install,
      all wired to the model commands), and the fog layer (`SpyingDataLevelCharacter` / `SpyingDataLevelUnit`,
      `show_shroud` / `unveil_black_shroud`, the label and lists-panel filters); the agents panel's hover-tooltip
      error fixed (`agents[i].agent_type_name`, which the install test was failing on). The promotion cost's charge is
      CONFIRMED structure (`0x008E2770` = `*(record->(+0x0C) + 0x38)`, no rank or distance input; naval free).
      Sandbox round 2 (0-G, 2026-10-05): **the record is not a `units` row** — `0x008E27D0`'s normal path
      guards on an `agents` table (`AGENT_RECORD`) row and returns a value out of the runtime string hash
      `0x00F9C2A0`, keyed by the agent-type name (`0x0145D9E0`) plus a world string, so **the price is one
      value per (agent type, culture)**; and `units` column #7 `unknown_3c` is **REFUTED** as the price by the
      data (109 distinct values, ratio to recruitment cost 0.698..4.857, 38 of 119 cost values split). Naval
      free confirmed end to end (navy panel rows read `PromotionCost = 0`, treasury unchanged).
      Sandbox round 3 (0-G, 2026-10-06): the price's **key is now CONFIRMED** — `<agent-type name>` ++
      **`<the human faction's subculture>`** (`factions` column #2 @0x10; the human faction defaults to the
      literal `"britain"`), so it is one number per (agent type, faction subculture), not per unit. And the
      **number is confirmed to be unreachable from the shipped data**: both tables on that axis,
      `db\agents_tables\agents` (1657 B, 65 strings) and `db\agent_culture_details_tables\agent_culture_details`
      (5020 B, 163 strings), are **name-only with no numeric column**, and the hash's builder is not
      statically reachable (`0x00F9C2A0` is the only function in the exe that hashes with that map's functor
      at `+0x30`; the row reader `0x00F94940` writes nothing at `+0x3C`/`+0x40`). **Thread closed as "needs the
      running game" — do not open a fourth static attempt.** The probe is hardened and self-checking
      (`cargo test -p ntw_data --test probe_script`; `cargo test -p ntw_data --test probe_install -- --ignored`
      confirms all 21 breakpoints against the shipped exe), and **round 2's probe had a real bug**: it broke on
      the cost slot's `return -1` (`0x008E27A0`) instead of its `return <price>` (`0x008E279C`) four bytes away,
      so the price line could never have appeared; every breakpoint now prints an `ARMED <name>` tag.
      `CurrentNumGenerals`/`MaxGeneralsAllowed` is now a **CONFIRMED negative**: the vanilla data ships no general
      limit at all (ten `character_recruitment*` variables, four effect rows, nothing in 86,977 files).
      Fog: `fog_state_at` is now the single definition of the three-way test (per-position and per-cell both
      defer to it, so renderer and labels cannot disagree), `fog_states` confirmed whole-map, and a **real label
      bug fixed** — the labels filtered with `sees`, dropping settlements that were merely explored while the
      renderer still drew them dimmed; they now use `knows`.
      **Open:** the price *number* — the hardened probe costs the user one sitting
      (`analysis/fidelity/debugger/0g_promotion_probe.cdb.txt`, run sheet `0G_PROMOTION_PROBE_2026-10-05.md`);
      and nothing is reachable in game yet because `panel_manager` (the popup opener the original
      uses for `enlist_commander` and the agent options/action popups) is 0-E's, and the field promotion has no
      script call at all in the original (it is the engine's own `CCQ_PROMOTE_COMMANDER` context menu).
      **And (0-E round 3, 2026-10-06) `panel_manager` turned out NOT to be the blocker at all -- it
      is the shipped `ui\panelmanager.luac`, which the root layout requires itself, so it always
      ran. The agent options/action popups now open and work end to end (UI_FIDELITY.md section 8).
      What remains unreachable is the FIVE agent actions that all end in `CampaignUI.MoveIntoTarget`.
      **`MoveIntoTarget`'s contract is now KNOWN (0-E round 5, UI_FIDELITY.md 9.10): the exe's own
      registration document reads "In: Character (agent), character or settlement (target), bool
      (research - steal if enemy settlement)", so `m_target` is a character OR a settlement and the
      third flag is what separates `research`/`steal_research` from `visit`/`embed`/`counterspy`. What
      is still missing is the behaviour on arrival -- the model's agent order queue -- so the five mask
      bits stay unset and the stub stays; with the queue, `embed` is move-then-`spy`, which the advisor's
      own sentence and the documented target kind already agree on.** 0-E also
      confirmed `CCQ_` appears in no shipped `.luac` **and the pack ships no context-menu layout**
      (only the warscape border art), so `CCQ_PROMOTE_COMMANDER` needs a menu of our own that
      someone able to compile `napoleon` writes.
- [ ] **PARTLY DONE: Comparison harness against the original game.** Built: `image_diff`, `NAPOLEON_BATTLE_TRACE` and the user checklist
      `docs/COMPARE_WITH_ORIGINAL.md`. **Open:** the first real side-by-side run (needs the user).
- [ ] **PARTLY DONE: Determinism** (battle sim, campaign, AI). Audit done (no hash-map order, clocks, threads or OS randomness in the
      model crates); twice-run harness identical for battles and campaigns (checked at every campaign merge). **Open:** platform maths
      (sin/cos/exp) only matter if we ship on other platforms.
- [ ] **AI** (§6) is paused by the user's section order; its open questions stay in `analysis/ai/AI_RESEARCH.md` §7 (incl. what the
      new campaign rules need from it: research choice, construction cost, tech gates, capture choice, agent actions).

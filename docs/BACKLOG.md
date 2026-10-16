# NapoleonRust — complete backlog to a finished 1:1 game

Created 2026-10-03 at the user's request: "add everything else to the backlog that's left before the game is complete".
The goal is a **complete 1:1 remake**. Everything below must match the original game as closely as possible: its formulas, data,
visuals, UI and middleware. The one exception is the original engine's hard limits, which are removed. These include fixed
counts of factions, regions, religions and cultures, and the caps on units per army, unit size and battle size. Defaults stay at
the original's values. See the Goal section of `CLAUDE.md`.

## Progress

Generated 2026-10-16 by `bash tools/progress.sh` (from `tools/backlog_count.sh` and `tools/tag_count.sh`); never edit it by hand.

Tags left in the code (done means zero): 1790 (699 PROVISIONAL, 102 PLACEHOLDER, 989 INFERRED), plus 584 UNKNOWN.

| Section | Done | Partly done | To do |
|---|---|---|---|
| 0. Cross-cutting fidelity | 134 | 0 | 28 |
| 1. File formats and data | 11 | 0 | 0 |
| 2. Graphics and rendering | 0 | 11 | 36 |
| 3. Battle gameplay (land) | 2 | 11 | 27 |
| 4. Naval battles | 0 | 1 | 7 |
| 5. Campaign gameplay | 4 | 13 | 29 |
| 6. AI | 0 | 1 | 10 |
| 7. User interface | 1 | 7 | 13 |
| 8. Video (Bink) | 2 | 1 | 1 |
| 9. Audio | 0 | 2 | 11 |
| 10. Multiplayer | 0 | 0 | 6 |
| 11. Modding (goal: original mods work out of the box, plus easy modding) | 2 | 0 | 16 |
| 12. Platform, release and quality | 0 | 0 | 6 |

§0 counts individual pieces; the other sections still count whole features, so their numbers move
more slowly. Split a big item into sub-checkboxes when work on it starts.

**How to use this file:**
  - `[x]` done in `main`. A note in brackets names anything still PROVISIONAL.
  - `[ ] **PARTLY DONE:**` partly done (GitHub shows it as an empty box).
  - `[ ]` not done yet.
  - `(running: <worker>)` on an item or a `##`/`###` header means a worker is on it now. A header
    followed by a `**Reserved by the maintainer**` line reserves that whole section or subsection.
    Both are copied to the public pinned claim issue by `tools/claims.sh sync` on each push to main.
- One line per item, kept short. Evidence, round history and long explanations go in the notes file
  named in brackets, not here.
- The manager ticks items off when a branch is merged, and adds new items as they are found.
- Each item's evidence and open questions live in the notes named in brackets.
- Rough order of priority inside each section: top first.

---

## 0. Cross-cutting fidelity

**Reserved by the maintainer** (listed in the public pinned issue #2): contributors don't claim §0
work while this line is here. To open §0 up, delete this line (or move it under single `###` headers).

**Exit bar (user, 2026-10-15):** §0 keeps only open items that change what the player sees or plays. Deep traces, debugger-only reads and Ghidra tidy-ups go to their feature section ("From §0" blocks) or Polish.

Each area keeps a notes file with a resolved/open table (question, answer, tag, code); read its "Where I am"
first. Evidence and round history live there; the pre-2026-10-07 long form of this section is in
`docs/archive/BACKLOG_0_long_form_2026-10-07.md`.

### 0-A battle rules (paused) — [analysis/fidelity/BATTLE_FIDELITY.md]
- Open items moved to §3 "Battle rules from §0-A" (2026-10-07, so §2 can start; the user: "most efficient").
- [x] Reload
- [x] Morale: timers, sub-evaluators, casualty-ratio ring buffers, start values
- [x] Unit attributes, strength potentials and card terms
- [x] Soldier hit points and the death dispatch
- [x] Chance to hit: control, visibility, angle judgement, woods cover, accuracy
- [x] Cartridge pool
- [x] Charges only at impact; melee exchange timing
- [x] Fatigue effects on speed, charge, control and attack
- [x] Ground speed columns and slope
- [x] Battle clock and time-out
- [x] Generals, reinforcement entry, units leaving the map
- [x] Battle Lua scripts in live battles (0 unknown calls; TUT_Land runs)
- [x] Skirmish, deployables (placement, contact kills, cover), building garrisons, special abilities, shot types
- [x] Map-preset weather
- [x] Which soldiers fire per firing drill (`volley_plan`, CONFIRMED in Ghidra)
- [x] Experience: melee has no experience term; `unit+0xD48` is the level and drives waver/rout timers and the fatigue bonus; land/naval bonus tables; battle-file `unit_experience` reaches every unit
- [x] kv_rules slot = list position

### 0-B campaign rules — [analysis/fidelity/CAMPAIGN_FIDELITY.md]
- [x] Campaign variables, taxes, GDP and town wealth (exact, 238 regions)
- [x] Trade, exact in all 9 original saves: nodes, routes, blockades, supply split, prices
- [x] Bankruptcy and the desertion gate
- [x] Land autoresolve (no retreat, CONFIRMED); naval autoresolve (inputs CONFIRMED on 71 ships)
- [x] Public order (1180/1184 classes)
- [x] Recruitment and construction queues (CONFIRMED); construction cost tech chains (328/330); construction and repair cost, affordability, charge and cancel refund traced in the exe (CONFIRMED, merged `47323d4`, user-checked 2026-10-07)
- [x] Research rate (CONFIRMED)
- [x] Effects wired in; region adjacency; sea-route cap
- [x] Capture choice (occupy / loot / liberate) and repairs
- [x] End Turn speed; turn order (CONFIRMED)
- [x] Diplomacy rules and computed factors (506/506); allies called into wars; treaty money
- [x] Religion conversion and drift (341/341)
- [x] Growth (72/72 eur, 31/31 spa) and the region recompute chain
- [x] Fortification slots, build / upgrade / repair; fort options (candidate rule INFERRED)
- [x] Militia rule (CONFIRMED)
- [x] Government drift on a government change (drift turn INFERRED)
- [x] Recruited unit size (3174/3174 save units), used for new recruits
- [x] Demolish command (refund PROVISIONAL)
- [x] Experience-adjusted recruitment cost (recruitment use INFERRED; upkeep untouched)
- [x] Region transfer in deals (`TransferRegion` rejected: needs the `0x00B449F0` flags)
- [x] Peace terms: regions and techs as deal items (`0x00B449F0`)
- [ ] Deals: the AI's region valuation is ported (`0x00C131C0` → `0x00A364B0` → `0x00AA1E90`, scaled by `0x00C4D140`), but its inputs are still PROVISIONAL: the hypothetical-order attitude (`0x00A79050`; offered regions use 0), every region assumed to belong to a group (region +0x12C, `0x00AA1F0A`), the AI personality's multipliers in `region_deal_worth` (defaults now); `grant_technology` requirement walk (PROVISIONAL); a foreign garrison left by a deal or liberation: what the exe does when the owner's army arrives (ours waits outside), `defenders_of` and recruit spawn (`0x008EF790`) (PROVISIONAL); the capital loss path (`0x008BA580` / `0x008DABC0`)
- [x] Recruitment details (`0x00AECEE0`, `0x00AED220`): cost from units #7 and region effects, the unit cap (units #15, `0x008F68B0`), the AI handicap and recruit prices from the model (`0x008DD090`)
- [x] Recruitable population gate and charge (`0x00A89550` / `0x00AAF190` / `0x00A61AA0`, `REGION_FACTORS` #2)
- [x] Capture refunds a port's naval queue items to the old owner (`0x00B1A760(1)` per port); our `occupy` refunds nothing; `0x00B58A30` is a fourth capture variant missing from the notes
- [x] Diplomacy-changed broadcasters (cai-broadcasters): make_protectorate follows `0x00B105C0`/`0x00B104E0` (ends old patron ties via `0x00B64B10`, hands on its protectorates, breaks the protectorate's other alliances); spa_napoleon loot resentment (`0x00B541E0` → `0x00B544F0`) as a CampaignFeatures row
- [x] Faction destruction and re-emergence (broadcasters2): the capital moves on capture, deal or director transfer (`0x008BA580`, `0x008DABC0`); a faction left with no capital ends (`0x008AB6C0` DestroyFactionAndResetRelations, `0x00B2B920` resets, `0x00B73430` trade break); liberation brings it back (`0x008AE6E0` → `0x00B2BB00`, records reset `0x00B2BC10` / `0x00B72AC0`, no pair with the rebels)
- [ ] Broadcasters left (broadcasters3): the revolt path `0x00AAB440` → `0x00B544F0` (INFERRED meaning; spa_britain access `0x00B444B0`; multiplayer); protectorate peace step (`0x00B105C0`); the tribute stays PROVISIONAL; PROVISIONAL `0x00B30A50`, the second factor array, `0x00B268B0`; INFERRED settlement slots, slot owner, tie order; listeners `0x00BB9410` / `0x00BB9B90`; pair-cache refresh `0x00BBDBA0` / `0x00BAC0D0`; mid-turn prices `0x00BCB020`; route rebuild `0x00BA0490`; CanThreaten; PROVISIONAL combined cap in `build_trade_route`; INFERRED port-full gap (an exporter-map entry the exe keeps empty, `0x00BB4F80`)
- [x] Ending a protectorate tie (`0x00B64B10`) moves the patron's characters out of each of the protectorate's regions (`0x008D9DC0` per region): modelled by transfer3 (`end_protectorate_tie` → `expel_from_region`, test in campaign/tests/transfer.rs)
- [x] Deal embargo direction CONFIRMED: the embargoed faction gets `trade_embargoed` towards the deal's other side (`0x00C18800` → `0x00B28DB0`) (deals-fix)
- [x] Director: the other desires that draw on the director RNG (`0x00BAC560`, `0x00BAC9C0`, `0x00BC06E0`, `0x00BD4A60`, `0x00BAED7A`): merged 2026-10-12 (director-rng: at-war, boost, XP, replenish draws; at-war order `0x00ABAFB0` CONFIRMED; garrison skip `0x00C3B7A0` +0xFC CONFIRMED). Left: the garrison object chain +0x12C→+0x34 (PROVISIONAL, ntw_sim director.rs)
- [x] Path grid: sea cells carry their sea region (`0x00BB52E0`), used by the director's transfer checks
- [x] Trace: does a region transfer eject the old owner's agents (and where to)
- [x] Director transfer garrison (director3): the count drawn first, the region's recruitable list filtered by the receiver's land recruitment beliefs (`0x00BAF9A0`, `0x00D9C9D0` / `0x00D9CD60`), sorted by category (`0x00B7F510`), raised round and round into one garrisoned army (`0x00BA8400`, placement as `0x008ECAD0`) CONFIRMED and tested; the belief-cache freshness left (§5)
- [ ] Region-transfer expulsion left (transfer3; rule `0x008D9DC0` modelled behind `region.expulsion`): PROVISIONAL placement stand-ins (outline point `0x008AF750`, flood `0x00B35640`, footprints, faction gain order, unstable sort); INFERRED embarked army never counts (shared-polygon lists `0x00B4D800` +0x140 / +0x154 and the carrier position `0x00A99A60` untraced); PROVISIONAL one theatre per map (a no-op on every shipped map)
- [ ] Access-loss expulsions not modelled: `0x00B29170`, `0x00B67BD0`, `0x00B0CA20`, `0x00B13A30`, `0x00B0D0C0` (transfer3)
- [ ] The deal's peace-record pass (+0x184) after a region exchange is not modelled (PROVISIONAL in negotiation.rs; transfer3)
- [x] (tradebreak) Breaking trade removes every route of the pair: `break_trade` and `break_trade_both_ways` (treaties.rs ~408-445); the exe removes only the first exporter→importer route each way (`0x00B73430` → `0x00B13C30` ×2 → `0x00BC0120`; `0x00B29BB0` → `0x00B733B0` → `0x00BC0120`), and its lookup creates an empty exporter entry (polish26)
- [ ] (running: routeacc) Trade per-route accumulated value (leftroutes, PROVISIONAL `accumulate_trade`): the exe keeps it per route (`0x00B05CC0` +0x3C, erased by `0x00BAC2A0`), ours per pair; and the `0x00B79B20` +0x60 deduction vector is not modelled
- [ ] Diplomacy backstab penalty (polish36 trace): the other-faction penalty (`0x00B0E540`) goes to factions of the declarer's subculture (`ApplyBackstabbingPenalties` `0x00B0E420` compares at `0x00B0E446`), not the victim's as treaties.rs `same_subculture(x, b)` does; the CAMPAIGN_FIDELITY backstab row also says "target"
- [x] (leftroutes) Trade routes left after a break (tradebreak review): the exe keeps earning on routes that outlive their agreement (`0x00BB3490`, `0x00BBD870`) while ours counts only `trade_partners`, and `trade_home_value` (trade.rs ~255) keeps subtracting their stale loaded volumes; trace and match
- [ ] `trade_port_full` no-entry rule (tradebreak, PROVISIONAL in trade_check.rs): the split `0x00BC26D0` (operator[] `0x00BC3699`) gives every faction an exporter entry, so a 0-cap port reads full in the exe and not in ours; `0x00B73430` never calls the price update `0x00BCB020`, so the entry `0x00BC0120` creates lasts until the next split
- [x] (dipcounters) Diplomacy manager counters (polish31 review): load DIPLOMACY_MANAGER #1–#4 (+0x14/+0x18/+0x1C/+0x20) into `World`; a faction without the block starts `treaty_breaks` at 1 (both constructors, `0x00AE8DD0`; vanilla saves hold 1 for every faction); ours starts at 0, so a first backstab has no `peace_treaty` penalty (`0x00B0E420`, treaties.rs ~276)
- [ ] The expulsion's move notice `0x009FB680` (message 0x119) is not sent (transfer3)
- [ ] UNKNOWN: which command calls the transfer path at `0x00936B37` (transfer3)
- [x] 14 recruit-cost save misses: prices fixed at queue time (traced 2026-10-10); Spain +49 was a saved type-0xE (unit category) `cost_mod` +25 on `naval_merchant` that `0x00B0D220` adds and our saved-bonus key never reached (recruit-cost2, CONFIRMED by name tables `0x0145B75C`/`0x0145B778`; 394/406 save prices match); `0x008B16C0` callers in CAMPAIGN_FIDELITY.md
- [x] Trace whether turn.rs:471 should block training of flagged items (`0x00B5AD90`)
- [x] Recruitable list, no-entry gates (recruit-gates: gates `0x00AA1660` (units #14 vs RESOURCES_ARRAY) and `0x00A27960` (scripts UNIT_RESTRICTIONS) CONFIRMED, construction before the queue step, +0x20/+0x24 = road/fortification slots)
- [x] Construction at round end (`0x008BC650` → `0x00AB3DA0`, before diplomacy `0x00B29100` and research `0x008F3C40`) (construction)
- [x] The DB loader drops units rows with an unknown region_unit_resources key (`0x00EDC000`, recruit-followups): every units reader goes through the dropping path; `Region::unit_resources` PROVISIONAL removed
- [x] Walls/road effects on region +0x188/+0x198 (`0x00A6AEE3`/`0x00A6AEFA` via `0x00A62380`, recruit-followups): walls effects count; slot holders on `Region`, own save v3
- [x] Construction options check the slot holder (`0x00B4333B` → `0x00A91FC0`); owner-change holder rule CONFIRMED `0x00B1B300`; the save writer writes slot holders; ESF construction item read CONFIRMED `0x00AFBA90` / `0x00B256C0` (construction)
- [x] Construction follow-ups (construction2): repair health per turn (`0x00B78490`, SetBuildingHealth); demolish items (type 3: StepDemolish, CanDemolish, QueueSlotDemolition, refund); the ESF item tails (per-type loaders from `0x00B256C0`: 0 upgrade `0x00AE7040`, 1 new building `0x00AE6EF0`, 2 repair `0x00AE6FE0`, 3 demolish `0x00AE6EB0`; tail writer vtable +0); inactive items wait (`0x00B7880E`); a construction-step seam in `CampaignRules::seams`; own save v4 (kind and active byte)
- [x] The exe reads `units_to_groupings_military_permissions` (`0x00F74CB0`); already modelled through the merged DB (recruit-cost2)
- [x] The importer limit (`0x00BB5730`): none; the split `0x00BC26D0` ported (352/352 route volumes)
- [x] The 4 desertion-exempt unit classes (`0x008BA1E0` + `0x00EED3E0`: cavalry_heavy, elephants, general, infantry_elite)
- [x] Trace the affordability rules of the unit pool and treaties (ours: their own checks in pool.rs ~309/484, treaties.rs ~467), and their charging arithmetic (ours: saturating/plain; overflow behaviour untraced; upkeep economy.rs ~525 and script treasury changes ntw_script game.rs:211 also bypass treasury.rs)
- [x] What the exe does with a queued recruitment item whose target commander dies (ours keeps the dead id: `character_dies` characters.rs:532, pool.rs:434, family.rs)
- [x] IsRecruitable bits 0 and 2 (generals maximum, no room) are not modelled (treasury-rules, CAMPAIGN_FIDELITY.md §Treasury changes)
- [x] Gate byte +0x1B7 (`0x009D1CD0`) is not modelled
- [x] Map the remaining treasury charge/credit callers (CAMPAIGN_FIDELITY.md §Treasury changes); queue item +0x24 is not modelled
- [x] `0x008F56C0` (paid when both sides are human) is not modelled
- [x] Ship repair in port (`0x008EEB70`, treasury3): order/toggle charge, cancels refund the recomputed cost, turn-end repair, one undock path for every navy move (`0x00A8C0B0`)
- [x] AI navy repair `0x00AA90F0` gated by the scripted shopping shroud (`0x00A8B2D0` / `0x00AA9B40`, script `disable_shopping_for_ai_under_shroud`); director credit `0x00BAF200` → `0x00BB33A0` traced, matches `boost_step` (treasury4)
- [ ] Land unit replenish toggle (shiprepair, PROVISIONAL in ntw_script ui/campaign/army.rs: logs once, returns false): `CCQ_TOGGLE_REPLENISH_UNIT`, state `0x00B78090`; vanilla scripts never call it
- [ ] navy_repair calls the undefined `Army.OnRepairButtonDestroyed` once the host sends OnDestroyed; handle it (shiprepair). Trace the `repair_navy` console command (`0x00426A40`)
- [x] (shiprepair) Campaign UI: wire the port ship-repair button(s) to `RepairShip` / `ToggleShipRepair` (ntw_sim commands.rs; treasury3 modelled the rule, no UI calls it yet); trace which layout/.luac element the original uses
- [x] Diplomacy: cap a demanded one-off payment at the payer's treasury (`0x00C4AB4E`–`0x00C4AB82`; `deal_item_commands` / `one_off_payment` do not cap; check the negative-treasury case for an ORIGINAL BUG; CAMPAIGN_FIDELITY.md deal → item copy)
- [x] Importer: read the capital even when the faction has no `FORT_UPGRADE_MANAGER` block, as the exe does (`0x0087B1DA`; `crates/ntw_campaign/src/details.rs` `faction_details`)
- [x] Diplomacy: payments Propose rows and the AI's payment evaluation use the capped offer (seam `diplomacy.proposed_payment`, `0x00C4AB45`–`0x00C4AB85`, row text shows the capped amount); ORIGINAL BUG fixed: a negative treasury turned a capped offer into a demand (`0x00C4AB7E`); a deal applies all or nothing (deals-fix)

### 0-C middleware — [analysis/fidelity/MIDDLEWARE_VERIFY.md, BINK.md]
- [x] Miles mixing rules and loudness (CONFIRMED against the exe); game-speed sfx rule; UI sound kinds
- [x] Bink intro: order, once per start, full-screen sizing
- [x] Battle animation ACTION table contract (CONFIRMED against the dumps)

SpeedTree leftovers are in §2.

### 0-D units, animation, terrain, trees — [analysis/fidelity/UNITS_TERRAIN_FIDELITY.md]
- [x] Animation slot table, per-man clip selection, gait levels
- [x] Sim-driven fire, reload, melee, death and knockdown clips
- [x] Unit and horse LOD switching
- [x] `groupformations.bin` and the default deployment (incl. the guerrilla group)
- [x] Tree list format, heightfield normalisation, training-level order
- [x] Standard bearers draw their flag: cloth geometry, pole and bone CONFIRMED; verlet solve in `ntw_sim::battle::cloth`
- [x] `slots_art` / `slots_templates_models` load; region fort model `fFort` level n → `fort_lvl<n+1>`
- [x] Dependent factions' flag key = last segment of `factions.flag_path` (77/77; the exe call PROVISIONAL)

### 0-E campaign map, campaign UI, front end — [analysis/fidelity/UI_FIDELITY.md, analysis/campaign/CAMPAIGN_MAP.md §10]
- [x] UI scale for 720-high windows (CONFIRMED)
- [x] Radar map; government, technology, objectives, lists and diplomacy screens
- [x] Building browser and tree; capture screen; Load Game page; tooltips; credits; text entry
- [x] Custom battle setup: settings, armies, save/load, start
- [x] Campaign trees, Bezier splines, river ribbons, coastal surf, close-up supertexture, town/port facing
- [x] Settlement panel: upgrades, build/cancel/repair, recruit/cancel, building permissions, restricted buildings
- [x] Naval and infrastructure tabs; demolish button; experience-priced recruitment cards
- [x] Negotiation object (20 methods)
- [x] `panel_manager` (the shipped `panelmanager.luac`); `enlist_commander` panel opens, position CONFIRMED
- [x] Agent action popups end to end: Assassinate, Sabotage, Duel
- [x] Address representation matches the original (`Pointer<T>`, `CHARACTER` in `tostring`)
- [x] River scroll rates (CONFIRMED 0.02 / 0.05); border shader named; roads are textured (4 materials)
- [x] Panels can't be closed: merged `8aad50d`, checked in game by the user 2026-10-07
- [x] Side-by-side UI fixes (user screenshots 2026-10-07), merged `53361ec`: technology links from the exe's link step (no line over the title); Army | Recruitment tabs by the exe's rule (generals and admirals; colonels none, CONFIRMED by the user); general portraits in the army bar and Lists; Lists docking in the scripts' 1280x960 frame (debugger sitting, CONFIRMED at 1920x1080). Open parts are their own §0 items (generated-character portraits, army recruitment contents)
- [x] Wall building: merged `28471b5`, checked in game by the user 2026-10-07 (the walls are the last construction card; they appear on the map when finished)
- [x] Region labels under the HUD (CONFIRMED: parents sort children by signed Priority on `OnAdoptChild`, `0x01016BD0` / `0x01024F70`; labels -1)
- [ ] Region details panel: filled from `0x009E69B0` / `0x009AF570` (merged 2026-10-10: taxes, religion, classes, governor, public order, wealth/growth with factors, predicted values from a one-round projection, PopulationChange/WealthChange; Name CONFIRMED). Left, PROVISIONAL: Effects (region `+0x1CC` list, writer untraced), NextTown (town emergence `0x00AB3FF0`/`0x00AADB00` not modelled), the predicted garrison for a selected army (`0x00A190A0`)
  - [ ] The round-start GDP/growth/`+0xC4` refresh `0x00AB4410(0)` (also on construction and tax events) is not modelled, so the TownWealth total can differ from the live factors; WealthChange is computed live, not the exe's stored `+0xC4`
  - [ ] `RetrieveFactionRegionList` and `LabelDetails` still show 0 for both changes: use the projection
- [x] `OnDivorceChild` fires on Adopt, Divorce, Destroy and DestroyChildren (`0x01027BA0` → `0x0102DE60`; ORIGINAL BUG `0x01027B20` fixed: DestroyChildren could loop for ever)
- [x] Does `RetrieveVisibleEnitityDetails` skip settlements behind the HUD (campaign hud.rs `covers`)?
- [x] `UIComponent:Adopt(child, index)` ignores the optional index `0x01014580` reads
- [ ] Diplomacy negotiation playable; region exchange rows. Merged diplomacy2: (user check 2026-10-09: the changes work) greeting by attitude through the queued BeginNegotiation (`0x00BF5A60`), diplomat portrait, button lists for all three relationships, power/wealth words (power = raw unit upkeep, CONFIRMED `0x008B2150`), regnal numerals. Austria power now "Terrifying" as the original (power CONFIRMED against the 2026-10-10 debugger values at the Coalition start; prestige PROVISIONAL 0). Merged 2026-10-10: option buttons built (`SetState` returns as `0x01013550`), `FindChildAddress`, treaty lines, tradeable regions and technologies (two lists, `0x009C5770` / `0x00C5C170`); red cancel texts are the original's. Propose builds region/tech deals in the model (2026-10-10; other actions' items PLACEHOLDER). Open: tech list order PROVISIONAL, region-row tooltip empty, our text ~20% wider (a treaty line wraps), deal rows and result still held in the UI until the deal is modelled
- [x] Runtime `force_diplomacy` calls have one home in the model (relationship field #18, merged 2026-10-10)
- [ ] Scripted diplomacy permissions (#18) not yet read by: the AI intention filter `0x00CB8200`, the counter-offer filter `0x00CC58C0`, `0x00C561C0`, the trade UI check `0x009F33E1` (CAMPAIGN_FIDELITY "Scripted diplomacy permissions")
- [ ] (running: savename) Campaign save naming
- [ ] The five agent options actions via `MoveIntoTarget` (contract known; needs the model's agent order queue)
- [ ] `AgentRogueSabotageArmy` (no army target list)

### 0-F effects system — [analysis/fidelity/EFFECTS_FIDELITY.md]
- [x] Techs, buildings, traits (CONFIRMED level rule), ancillaries, government, ministers, difficulty; query API wired into the economy

### 0-G characters and agents — [analysis/fidelity/CHARACTERS_FIDELITY.md §11-§12, CHARACTER_UI_HOOKS.md]
- [x] Trait and ancillary gain; natural death
- [x] Commander succession (CONFIRMED), royal family, vacated posts, female leaders
- [x] Minister dismissal, appointment and the spare pool
- [x] Agent actions: rolls and chances (CONFIRMED), assassination, duels, sabotage, tech stealing, forced-success flags, script events
- [x] Diplomatic reactions
- [x] Sight and shroud (CONFIRMED), hidden characters, spying, spy networks, the stealth test (118/118)
- [x] Recruitment pools, historical characters, HireGeneral (cost CONFIRMED), HireAdmiral, PromoteUnit / CharacterPromoted
- [x] Turn-end counters; CharacterCreated / CharacterPromoted events
- [x] Character UI hooks wired: commander hire, promotion, target pickers, fog layer
- [x] Promotion price key: agent type + the human faction's subculture (CONFIRMED); naval promotion free
- [x] No general limit in the vanilla data (CONFIRMED negative)
- [x] Fog: one `fog_state_at` definition; explored settlements keep their labels
- [ ] Promotion price number: debugger sitting step 2a (`analysis/fidelity/debugger/SITTING_SHEET.md`; slot +0x44 at `0x009AC5FF` settled statically)
- [ ] Field promotion menu (`CCQ_PROMOTE_COMMANDER`): no shipped layout, needs our own context menu
- [ ] Minister, royal family and agent portraits for new characters (`0x008BF310`, `0x00A055E0`; the family is a PLACEHOLDER, CHARACTERS_FIDELITY.md §5c)

### Cross-cutting
- [x] Determinism audit (no hash-map order, clocks, threads or OS randomness in the model crates); twice-run harness identical
- [x] Comparison harness built: `image_diff`, `NAPOLEON_BATTLE_TRACE`, `docs/COMPARE_WITH_ORIGINAL.md`
- [ ] First real side-by-side run against the original (needs the user)
- [x] Battle `B.WindowsTime`: `timeGetTime()*0.001f` as a float (`0x005D4BF0`); the campaign's `0x009FB0B0` truncates to int; one shared clock
- [x] Portraits of generated characters: pool hires and promoted generals get a portrait in the model as the exe does: the per-folder deck is dealt by `0x00A1F730`, a promoted officer draws only while his number is -1 (`0x00A1A300`), and the draw order follows `0x00A05440`. The guerrilla flag is CONFIRMED 0. ORIGINAL BUG `0x00A1F730`: a folder's last picture was never dealt, fixed (CHARACTERS_FIDELITY.md)
- [ ] Army recruitment tab follow-ups (tab merged with army-recruit-tab): a finished item targeted at a commander marches to him (`0x00B0A270` / `0x00AE8B10`; ours spawns it in the settlement); the `0x00AC54C0` limit test and zone-of-control cuts of the source search; who writes `+0xA94`; original save #2 = the item's target character id (INFERRED); the naval tab's port path (PROVISIONAL)
- [x] Battle HUD scripts' frame: CONFIRMED 1280x960 (debugger sitting 2026-10-09, root at manager +0xC4); the battle host uses `UiFrame::ScriptFrame`; CreateFromLayout makes the file root's first child at its own frame position (`0x01027400`, `0x0101E270`)
- [x] Battle HUD UI scale: the exe's UI scale (`0x0114EB20`) applies to the battle HUD too; ours lays the battle HUD out at window / scale and maps the mouse back
- [x] Battle `B.Time`: the exe binds it (`0x005D3BB0`): the UI pulse clock, real time capped at 300 ms a frame
- [ ] The front end still draws at UI scale 1 (the exe's `0x0114EB20` applies to every HUD)
- [x] (layoutxy) `battle_prelude.lua:72` drops CreateFromLayout's x, y: root.luac's pip_frame and xp_bling and the esc_menu options (all 0,0) land at their file offsets
- [x] Battle formation (formation): units re-place their living men after losses; width/depth/radius measured from the men (`0x0053D1A0`, `0x0053C3D0`), evade room from the box front centre (`0x0057A6F0` → `0x0053FBD0`), shape kinds (`0x005498D0`, `0x0057A6F0`), skirmish strips with the box overlap test (`0x0055A1B0`), leave/arrive per-man circles (`0x005857A0`) CONFIRMED
- [x] Battle formation (formation3): men walk to the nearest free slot instead of the view snap (`0x005373F0`, `0x00502770`, `0x005281D0`); reform walk/run (`0x005736B0`, `0x0055F160`, run beyond 25 m² `0x00658C50`, tolerance 910); the box refreshes on alternate ticks (`0x0053C180`); the first previous heading is 0 (`0x014A3498`); the place_at deployment refresh (`0x0054FDD0`); slots are reassigned only by a first order (`0x0051A8E3` / `0x00581AE0`, not a re-issue `0x00584E10`)
- [x] Battle formation (formation4): the idle reform gate (`0x00573770` / `0x00557C20`) and the in-place check (`0x00557C80`) CONFIRMED and tested; shape parameters traced (line, kind 3, kind 6 light infantry); morale uses the unit's real training level (+0x194)
- [ ] Battle formation left (formation4): the alternate-tick parity uses our unit id, the exe a serial (PROVISIONAL; debugger sitting); the attached-member radius (`0x00607540`); request +0x28; building and reinforcement place_at; special soldiers; task `0x00515210` (kind 6 files); soldier +0x1A8; fire-group gate `0x005732B0`; controller +0xB4 veto; the `0x00580DD0` task
- [x] Battle formation shape layouts (formation5): rows (`0x00619EB0`, p4 apart `0x00659F00`), diamond (`0x0062FDC0`, `0x0061A260`), wedge (`0x00620990`, `0x00619B60`), kind 6 p3 3.5 / p4 2.0 (`0x006210D0`) and the light-infantry stagger (`0x0065A4F0`) CONFIRMED and tested
- [x] (formation6) Battle formation shape layouts: squares, kinds 2 and 4 (`0x00630390`, `0x00631560`, template table `0x014D0690`); pike wall, kind 3 (`0x006311B0`); the kinds 1/5 anchor (`0x007FC920` measures)
- [ ] Battle formation shapes left (formation6, PROVISIONAL in march.rs): the arcs along the path (`0x007F2DA0`, vfuncs +0x14/+0x18, builder from `0x00815F80`); pike-wall width and p1 (`0x00578860` / `0x00579780`) and its pose word (5/4/3); slot headings are not applied
- [ ] Soldier flags +0x35B / +0x35C, cleared by `0x005577C0` (formation4), are not modelled
- [ ] (running: attackers) Battle morale attackers (fightcav, PLACEHOLDER in model.rs): build the attacker list and direction from the exe's list (+0xADC → +0x74C) and `0x0057F310`; the per-unit seen set lives until the destructor `0x00522690` (INFERRED; debugger)
- [x] (fightcav) Wire `fighting_cavalry` (`0x0053C720`) to the riding test `strength::riding` (`0x0055C2A0`) (horsefright)
- [x] (horsefright) `horses_frightened` (ntw_sim battle morale.rs) tests `strength::mounted`, the exe `0x0055C2A0` (riding and not dismounted): they differ for a cavalry or camel unit with `dismount` active; check which strength callers use `0x0055C2A0` and which `0x0055ABF0` (formation4)

AI (§6) is paused by the user's section order; its open questions stay in `analysis/ai/AI_RESEARCH.md` §7.

## 1. File formats and data
**Status: COMPLETE** (2026-10-04 ~10:55: all 11 items done. Save compatibility with the original game was REMOVED from scope by the user after NR-14/NR-15 crashed the original; our game saves and loads its own saves and reads every vanilla save. History: SAVE_COMPAT.md). Markers are strict here: `[x]` = nothing left, `[ ] **PARTLY DONE:**` = partly done or still being
worked on, `[ ]` = not started.
- [x] packs/Vfs, DB, loc, ESF, rigid_model, DDS, TGA, UI layouts, fonts, unit variants, anim, weighted meshes, battle terrain,
      vegetation lists, campaign map, preferences, sound banks
- [x] Bink `.bik` video: container, video (73/73 movies) and audio (229 tracks) decoders plus the player, merged d5251e6 [BINK.md].
      (The player's skip and loop rules and turning `--intro` / `--frontend-movie` on by default need a comparison with the original;
      that's §0/§8.)
- [x] SpeedTree `.spt`: all 229 files parse to the end; generator recreated (merged 7ffc1d7).
- [x] **`pathfinding.esf` and `sea_grids.esf`** (closed 2026-10-04, round 7 of pathfinding-ports; residual unknowns below). Done: readers (incl. nap_spain's value cipher), the movement grid
      from the original polygons, polygon kind = flags & 0xF (CONFIRMED), the original's cell flood search and its mover/kind table
      found; **the original's path search found and ported** (`ntw_sim::campaign::polypath`: A* over (cell, polygon) nodes, CONFIRMED
      costs: the cell header bytes are per-direction cost bytes, roads cost `road_level_N_action_point_cost` per region, border strips the
      cheaper region; fleets use the same search on sea kinds; the `grid_data` u32 = polygon count; same-component pre-check); reachable
      range = cost within the remaining AP; rivers = kind 3 strips crossed only at roads/kind 7; flag bits 24..31 = direction mask.
      Path smoothing found (corridor, triangles, taut line) and ported (`polysmooth`); zones of control decoded and ported (`zoc`: enemy
      armies block a 6-unit walking zone, navies 12, others their core). Ports / landing / embarking ported
      (`ntw_sim::campaign::embark`: port and landing nodes, landing ends the turn, Embark / Disembark commands; a fleet lands an army
      on Corsica in the install test) [PATHFINDING_PORTS.md].
      Round 2: polygon point, landing valid-position check and port relation (own / at war) CONFIRMED; harbour exit through the
      port footprint INFERRED (data fits all 67 ports). Zone-of-control core = 1-unit circle (CONFIRMED).
      Round 3 (pathfinding-ports): run-time cutter decoded to the polygon clipper (shape kinds per mode, kind link table CONFIRMED;
      not ported: zones still block whole polygons); mover families CONFIRMED (family A unless inside a building) and the kind-7
      rule applied to land movers (`movers`, all settlements reachable); the saved transport link (NAVY #4 / ARMY #7) CONFIRMED
      and loaded.
      Round 4 (pathfinding-ports, `zoc` now owned there): zone limits fixed (army 6, fleet 12; saved boxes match, swapped 0);
      the run-time cutter ported (`rtcut`: overlap kinds decoded in `0x00B13520`, exact core clipping, kind link table; zones
      re-kind their polygons; `polypath::Overlay`/`View`; `plan_path` searches the cut map); the second kind 7 rule
      (`0x00B167B0`) decoded, kind 7 rules applied to every mover incl. fleets; garrison core = 0.25 triangle; obstacle record
      API for the save writer (`zoc::obstacle_record`, box and cell ranges CONFIRMED on every start-position obstacle).
      Round 5: the original's obstacle storage decoded on vanilla data (pieces, cell versions and their flags rule, grid
      nodes, pair lists, slots = modes) and `ntw_campaign::grid_obstacle::add_character_obstacle` writes a complete obstacle
      for a new character (passes `save_check`; same cells and rings as the original for 68 of 95 start-position obstacles).
      Round 7 (closing): search mode 2 / 3 CONFIRMED (the order's target obstacle re-kinded to 10 / 11: walk in and end there,
      never cross) and ported; family A never enters its target footprint by search (CONFIRMED; the order code snaps it in,
      ours walks in, tested); the zone flood's cost pairs and node points decoded in structure.
      **Residual unknowns (none blocks load, save or play; tested safe behaviour in place):** how the target is named to the
      query (3-unit stand-in); the snap step's AP; 26 of 69 army zone boxes one cell off (flood entry points `0x00B011B0`);
      the in-port fleet's search type (harbour exit INFERRED, fits all 67 ports); the embarked commander's saved position
      (no vanilla example; ours keeps him with the fleet); the pieces' exact shapes; modes 2..4 in written obstacles
      (wiring new obstacles into the save writer is save-compat's)
      [PATHFINDING.md, PATHFINDING_PORTS.md, CAMPAIGN_DATA.md §1, §11].
- [x] Startpos/save fields the loader skipped: **done:** names, portraits, traits, ancillaries, government posts (leader, ministers),
      governorship taxes, capitals, full diplomacy records, the scripts' `save_value` slots (loaded and written) [CAMPAIGN_DATA.md §3-4].
      Mission records: layout CONFIRMED from the exe, reader + writer `ntw_campaign::missions` (s1-missions-ui) [S1_MISSIONS_UI.md].
      Diplomacy: all 29 `DIPLOMACY_RELATIONSHIP` fields named from the exe (24 attitude factors, 14 force_diplomacy slots CONFIRMED)
      [S1_LEFTOVERS.md §1]; #16/#21/#22 CONFIRMED unused (only loaded, saved and copied). Mission targets are object ids re-linked
      through the id map (CONFIRMED) and missions load into the model (542c263). `other_income_mod`: INFERRED never saved (§11).
- [x] `.markers`, `.farm_fields_tile_texture` and the farm files (battle map extras): readers and install tests; the grass-texture chunks
      are TGA files (all 720 decode). Farm records typed (`FarmManager`, `FARM_COLLISION` radii CONFIRMED 387/387) [S1_LEFTOVERS.md §2].
      Collision bool, the run-time centre rule and the two farm lists (inside / reaching outside the playable area) CONFIRMED; the
      stored centres are INFERRED leftovers. Piece pairs = the walls on the farm's edge, {wall index, side = the wall's slot for the
      farm} (CONFIRMED: data 86/86, and the exe's copy for a farm in both lists flips the side); the owner's second u32 = the tile
      template index (CONFIRMED; 0 because no manager has 2 templates). Wall instances settled: #1 = the boundary kind the generator
      draws from the farm's kind list (CONFIRMED), the {u32, i32, u32} list = the bordering farms (CONFIRMED), the last list
      CONFIRMED unused (empty everywhere) [S1_LEFTOVERS.md §2 updates].
- [x] The 23 non-mount `.variant_weighted_mesh` files: all 286 parse (equipment attachments, headerless agent and testdata layouts) [CAMPAIGN_DATA.md §7]
- [x] UI templates and layout fields: `uied.templates` all 126 entries read (v<6 legacy fix; the header u32 is the entry offset).
      UI layout `unknown_da` = ClipChildren (CONFIRMED), `unknown_e5` = UseGlobalClicks (behaviour CONFIRMED in every use: takes
      clicks anywhere without consuming them; the name comes from the editor's field list, as for ClipChildren), `unknown_140` = an inherited
      DrawMode (inheritance CONFIRMED; it picks one of three sprite-batch / text render variants, CONFIRMED). State +0x60/+0x64 =
      TextXOffset/TextYOffset (CONFIRMED, applied in the front end with the exe's text layout rules); the +0xF0 pair is editor-only
      (s1-missions-ui). DrawMode 0/1/2 CONFIRMED (0 = scaled with the UI, 1 = 1:1 window pixels, 2 = full screen). State +0xD0/+0xD4
      CONFIRMED unused at run time (loaded only; no reader in the UI code) [UI_LAYOUT_FORMAT.md].
- [x] The 31 older-layout `testdata` anim clips (28-byte keys; the 4 oldest without bone names): all 3,814 `.anim` parse [ANIM_FORMAT.md]
- [x] The 2 exotic DDS formats (A16B16G16R16; fourCC 63 = Q8W8V8U8 used as plain RGBA): all 8,239 DDS decode
- [x] All installed languages for loc text, plus the language setting: `--language xx` or our own `language.txt` in the NapoleonRust
      user folder, else the install's `language.txt`; any installed `local_XX` can be opened (only `local_en` ships here) [CAMPAIGN_DATA.md §10]

## 2. Graphics and rendering
Direction (user, 2026-10-11; DESIGN.md §3.7.1): our own modern, data-driven renderer, not a port of the original's. The
original's data (texture channels, masks, per-map lighting, LOD and unit-detail choices, effects) is still traced and is
the input; how it is drawn is ours. Items below that say "as the original" mean its data and choices, not its shading.

**Renderer foundation (do first when §2 starts)**
- [ ] Renderer design pass: material model (PBR + translation layer for the original's specular/gloss materials,
      overridable by mods), lighting pipeline (shadows, AO, SSR, light probes, sky, volumetric fog, tonemapping, AA),
      mod-added materials and shaders without rebuilding, release-build frame budget; no ray tracing, one renderer.
- [ ] Data-driven figures, no fixed unit kinds (DESIGN.md §3.7.1): a unit is composed in data from model parts attached
      at points (each with its animations) and a movement height relative to the ground (a number: flying, underground);
      Napoleon's man + horse is just the default data.
- [ ] General engine abilities for mods (with §3 and §11): terrain changed during a battle (trenches, craters, tunnels)
      in the model, pathfinding, line of sight and drawing; units below ground. Vanilla never uses them.

**Shaders and materials**
- [ ] The original's materials read in full (`Textured_Rigid.fx`, the unit/skin shader, the terrain shader in the exe,
      water, sky: what each texture and constant means) and fed to our materials: normal, gloss and specular maps, the
      faction colour-mask combine, baked shading compensated.
- [ ] Per-faction texture atlases for units, if the exe uses them instead of the loose textures [ANIM_FORMAT.md]
- [ ] Crowd system: LOD switching for buildings, units and horses with the original's choices (`0x0125ED60`, unit-detail
      option), GPU culling, instancing; far stand-ins only if measured to be needed for heavy mod models.
- [ ] Shadows, lighting, HDR and tone mapping, bloom and depth of field in our renderer, driven by the original's per-map
      values (graphics options included).

**Trees and vegetation**
- [ ] **PARTLY DONE:** **SpeedTree 1:1.** Near trees, shrubs, LOD fade and wind are merged (7ffc1d7); still to do: branch/frond/leaf LODs, normal-mapped lighting, leaf rocking, weather wind, per-tree rotation. Still open: the `.spt` decoder, the branch/frond/leaf generator with
      SpeedTree's own RNG, LOD fade to billboards, wind and lighting.
- [ ] **PARTLY DONE:** Shrubs, drawn with SpeedTree (7ffc1d7). Open: the same SpeedTree leftovers as above (LODs, lighting, leaf rocking).
- [ ] **PARTLY DONE:** Billboard trees (the stand-in; the atlas UV bug is fixed).
- [ ] Grass (`fx\grass.fx`, `grassmap.tga`, `.tai` atlases).

**Battle terrain and battlefield**
- [ ] **PARTLY DONE:** Terrain heights, colour maps, buildings, deployment zones.
- [ ] Detail, blend, cliff and rock maps, and the lightmap [BATTLE_TERRAIN.md]
- [ ] **PARTLY DONE:** Water: rivers, lakes and sea, with reflections and shorelines. Done (sandbox port 06d6a7e): battle-map sea surface (ocean.fx port, sea level 0 m CONFIRMED on 60 maps), camera above the sea; foam off by default (PROVISIONAL). sea-water (merged 2026-10-11): shipped sea\sea and sea\swell wave frames, sea_surfaces + sea_climate_details values, SKYGEN sky reflection; user check 2026-10-11: looks great vs the original's Nile. User check 2026-10-10 (NHB_Nile vs the original, same view): ours is pale sky-blue and very bright (reads as a sky reflection), the original dark grey-green; our waves are long regular stripes, the original small irregular chop; `NAPOLEON_SEA_FOAM=1` shows no visible change. Open: rivers, lakes, shorelines, reflections, campaign water [WATER.md].
- [ ] Sky dome and clouds, time of day, and the `.environment` lighting in full.
- [ ] Weather: rain, snow, fog, wind (visual and gameplay effects).
- [ ] Fences, walls, bridges, fords and other placed battlefield objects.
- [ ] Building damage and destruction (the `_destruct` models), and fire.
- [ ] Generated battlefields from campaign positions (terrain tiles), for campaign battles that aren't historical maps.

**Battle effects**
- [ ] **PARTLY DONE: Musket smoke, cannon smoke, muzzle flashes, dust.** Ported from the old sandbox
      branch (s2-effects) onto `next` and finished: the effects database reader (`ntw_formats::effects`
      for `effects\landbattle.xml` and `effects\unit_dust_parameters.txt`, 283 emitters / 152 groups
      CONFIRMED on the install), the deterministic particle world (`battle::fx`, `CaRng` off the
      battle seed), the per-unit dust timer at the shipped entity frequency, the draw layer
      (`battle::fx_draw`, `particle.wgsl`) with the original's own effect textures. Three draw-layer
      bugs fixed on port: the 2D HUD camera was used for the billboards, the buckets were matched by
      query position (a `Query` has no order), and every frame added a new mesh to `Assets<Mesh>` (an
      unbounded leak). Round 2: **which group a muzzle plays is now the shipped data** —
      `projectiles` column 31 is the fire effect group itself (120 of 144 rows set it, every value a
      `landbattle.xml` group), so `fx::fire_group` reads it and the 9-pound guess is only a fallback;
      `FIRE_GROUPS` is now exactly the eleven groups the data names. Round 3: **sprite facing now
      comes from the file too** — the reader names all four shipped `sprite_facing_mode` values
      (CONFIRMED over all 435 emitters: `CAMERA_FACING` 401, `BILLBOARD` 18, `LOCAL_Y_AXIS` 14,
      `WORLD_Y_AXIS` 2) and the draw layer honours the two Y-axis modes, so a shell's earth and
      debris stand upright instead of tipping at the camera; `VELOCITY_FACING` is a clean negative
      and `align_to_velocity` is false everywhere. Still PROVISIONAL: the muzzle height and
      position, the dust group choice, the light one uniform, `RENDER_METHOD_DISTORTION` drawn flat.
      Not checked in game [BATTLE_EFFECTS.md].
- [ ] Projectiles: musket balls, round shot, shells, canister, rockets, with visible flight. Not
      started: our model resolves a volley at once, so there is no flight to draw. **Finding for 0-A:**
      whether the original inserts a delay between fire and impact is a battle-rule question, so it is
      Claude's to answer, not ours — inventing flight time here would be a fiction, not a port.
- [ ] **PARTLY DONE: Explosions, impacts, craters and decals.** Blood on a man, the air burst and the
      ground scorch play the shipped group names (`blood_gen`, `AirExplosion_sml/med/lrg`,
      `Cannon_Groundimpact_explosive`). Round 2 closed the size rule from data: the air burst and the
      scorch are the shot's own row in `db\projectiles_explosions`, read by the new
      `ntw_formats::projectile_fx` with no schema at all (the 0/0/17 byte-gap run is unique to a row
      and occurs 35 times, the header's count). The size is **authored per row, not computed**:
      `shell_12lb` -> `AirExplosion_med` but `shrapnel_12lb` -> `AirExplosion_sml`, so the 6/12-pound
      rule is only a fallback. Two negatives: the row's third string column is a fragment
      **projectile** (`projectiles` key), not a group; and **craters/decals are not driven by the
      effect system** — no shipped effect file names `decal.fx` and there is no `RENDER_METHOD_DECAL`,
      though the decal textures ship. Still open: `projectile_impacts`' surface column order (so the
      blood group stays INFERRED) and the numeric columns' names.
- [ ] **PARTLY DONE: projectile trails.** Round 3 **closed the one open naming thread from rounds 1-2**:
      a shot's trail is a *pair*, and the two halves live in two different columns.
      `projectiles` column 32 (`trail`) is the trail's **effect group** in `landbattle.xml` — all 7
      distinct values over 9 rows are groups — and **column 6** (`trail_texture`), already decoded and
      already named in `analysis/worker2/schemas_battle.md`, is the foreign key into
      `projectile_trails`: all 5 distinct values over all 144 rows are that table's 5 keys. Round 2's
      negative was real but was the wrong question; column 32 was never the foreign key. The table
      now reads whole with no schema guess (`ntw_formats::projectile_fx::TrailTable`, 5 rows,
      `ssffffffffff` = key + blend mode + 10 floats). Named from the data: the second string is the
      **blend mode**, and floats **4-7 are an 8-bit RGBA quadruple** (white at alpha 128 on three
      rows, grey at alpha 100 on the rocket, all zero on `none`). **Still open, on a negative:** the
      other six floats — one table row serves projectiles spanning 4..250 m/s of muzzle velocity and
      50..750 m of range, so no float in it can be a per-shot duration or length. **Not drawn**:
      trails need visible flight, which is a 0-A battle-rule question (see above).
      [BATTLE_EFFECTS.md §7]
- [ ] Unit flags and standards: the cloth mesh and simulation. The flag cloth is missing entirely today [CAVALRY.md]
- [ ] Selection rings, movement and order markers, formation drag, and destination ghosts as the original draws them.

**Units and animation**
- [ ] **PARTLY DONE:** Soldiers, cavalry, command figures, DB clip choice, GPU skinning. Open: the flag cloth, the per-man pick RNG, the original unit shader (paused on `work/shaders`) [ANIM_FORMAT.md §7, CAVALRY.md §7].
- [ ] Combat, charge, death, reload, firing, turning, idle-variation, transition and dismount clips, all played.
- [ ] Per-man clip alternatives (more variety than 3 kits per unit).
- [ ] Artillery pieces, limbers, horse teams and crews, animated. Then the in-game look at shell scorches (orange; grenade/carcass hits leave none; measurable claims pass with `NAPOLEON_FX_CHECK=all`): moved from HANDOFF, user 2026-10-10 (no guns drawn yet).
- [ ] Ships (see §4).
- [ ] Generals on the battlefield, and Napoleon's own model.
- [ ] Equipment on/off per clip (sword drawn, musket shouldered) [ANIM_FORMAT.md]

**Campaign map graphics**
- [ ] **PARTLY DONE:** Terrain, borders, rivers, roads, settlements, army markers.
- [ ] Forts, ports, resources (mines, farms, logging), minor settlements and their growth levels.
- [ ] Trees on the campaign map, the coastline mesh, and textured border, river and road ribbons.
- [ ] Finer supertexture levels near the camera.
- [ ] Army, navy and agent figures as the original shows them (not markers), and ships at sea.
- Moved from §0-D/§0-E (2026-10-07; they need the shaders):
- [ ] Settlement walls mesh: drawn and swapped on build/demolish; level 1 seen appearing on completion in game (user, 2026-10-07). File level = chain level + 1 and additive drawing are still INFERRED (not compared with the original; looks off until the §2 shaders)
- [ ] Animated rivers (one UV offset in our own material) and campaign sea (shader UNKNOWN; lead: the campaign scene loader)
- [ ] Textured borders (width and V mapping are the exe's)
- [ ] Far-view tree models, resource-slot models, forts on the map
- [ ] Fog of war, seasons (winter snow), and campaign weather.
- [ ] **PARTLY DONE:** Region and settlement labels, selection effects, movement arrows and zone-of-control display. Done (sandbox port 06d6a7e, not checked in game): region labels (names/positions CONFIRMED, rendering PROVISIONAL), movement arrows with the original's assets (layout INFERRED, spacing/colours PROVISIONAL). Open: settlement labels, selection effects, zone of control.

**From §0 (exit bar, 2026-10-15)**
- [ ] Tree scale byte decode `u8 / 128`, clamp 0.5–1.4 (PROVISIONAL after review)
- [ ] Flag wind speed and solver substep / iteration / damping constants (PROVISIONAL)
- [ ] What string names a flag record (`+0x38`; needs a debugger watchpoint)
- [ ] Whether the bearer's cloth reads `flags.tai` (UNKNOWN); the exe's limp-flag shape; then a side-by-side look at Spanish and rebel units' flags (moved from HANDOFF, user 2026-10-10: trace first)

## 3. Battle gameplay (land)
- [ ] Gait change blend: merged 2026-10-10 (gait-blend2: mount blend, locomotion ladder with transition clips, acceleration and braking, timed move order, ground by palette colour; UNITS_TERRAIN_FIDELITY.md §1.9-§1.10). User check 2026-10-12: soldiers look good. Left:
  - [ ] Ground for maps without a TGA (`0x00EDCFC0`); the border tie rule; object ground +0x10/+0x18 (`0x0081AC80`)
  - [x] The order source `0x006DC700`: CONFIRMED statically (sitting-sheet, 2026-10-15): only `0x00585602` and `0x006DC7AF` call `0x006533E0`, and `0x006D9A22` reaches it through `0x006DC700` (UNITS_TERRAIN_FIDELITY §1.10)
  - [ ] Per-soldier timed orders
  - [x] User check 2026-10-11: cavalry now good; infantry may still hitch a little (investigate, same causes as gait-jitter?). Earlier (cavalry lockstep, run → walk hiccup; 2026-10-11 hitch + surge): gait-jitter merged 2026-10-11 (loops over n−1 intervals 0x010DA030, transition end frame, stride phase carried; 11-tick re-plan 0x00581B73, the surge is the original's own); earlier fixed in gait-blend3 (per-display playback scale `0x0078B8A0` on moving gait loops only, frozen poses + speed lerp at the run → walk change); waits for the user's look (HANDOFF). gait-infantry merged 2026-10-12: each man his own timed move (per-soldier speed and lag), path-end clamp, one formation grid; user check 2026-10-12: soldiers look good
  - [ ] Trace what the follow state's int target key measures (`0x0054D9F0`, key source vfunc +4): model.rs ~1195 re-issues a first issue every tick for a marching enemy (PROVISIONAL); the 2 m least drawn depth `setup::drawn_size` is ours (PROVISIONAL)
  - [ ] Ladder transitions under an action loop (PROVISIONAL); whether one-shots (shot overlay) move the exe's shown node
- [ ] Battle controls are ours, not the original's (PLACEHOLDER, napoleon/src/battle/input.rs): R restarts with a new seed, F toggles fire at will, arrow keys / Q / E move the camera. Replace them with the original's battle key set read from the shipped key bindings (find the file and the exe's defaults first); keep any restart only behind a test flag (user, 2026-10-08: R is not in the original).
**Battle rules from §0-A** (moved 2026-10-07; notes in analysis/fidelity/BATTLE_FIDELITY.md)
- [ ] `volley_plan` runtime confirmation in the battle probe
- [ ] Campaign-battle weather pick: ported, connect once campaign battles start from the map
- [ ] `unit_scale`: decoder done (exe default 0.75); where it is applied is UNKNOWN, not wired
- [ ] Who writes `unit+0xD48` (debugger write watchpoint): sitting sheet step 3b (`analysis/fidelity/debugger/SITTING_SHEET.md`)
- [x] Formation radius `+0x670`: settled (BATTLE_FIDELITY §6.1; sitting-sheet review 2026-10-15)
- [ ] Garrison cap `+0x6C`: sitting sheet step 3j (Aosta Valley farmhouses, `analysis/fidelity/debugger/SITTING_SHEET.md`)
- [ ] The `--battle --screenshot` "closed channel" capture
- [ ] Battle restart (R) keeps the same unit ids, so views survive with old corpses and `acts.dead` (napoleon/src/battle/view.rs ~426).
- [ ] Units drawn between model ticks (ours, PROVISIONAL `view::DrawnPose`, 2026-10-08); the exe's render blend is UNKNOWN: debugger sitting in BATTLE_FIDELITY.md §59.
- [ ] Run button: fixed to the traced rule (BATTLE_FIDELITY.md §59, change of speed edits the current move); in-game check pending. Double right-click run still untraced.
- [ ] Player run orders: our battle input never issues `MoveSpeed::Run` (only scripts do), so units can't be made to run (user, 2026-10-07). Build the original's controls from its layout/scripts and exe input rule (double right-click, the HUD run toggle; trace in Ghidra).

**Formulas, all from Ghidra or the DB**
- [ ] **PARTLY DONE:** Melee, morale, fatigue, missile fire. Done (0-A): who fires per drill (the "half the men" placeholder is gone). Open: target choice, holding fire in melee, other PROVISIONAL values [`ntw_sim` shooting].
- [ ] **PARTLY DONE:** Accuracy and hit chance, reload formula, ammunition, experience and chevrons, weather effects on muskets. Done (0-A): hit chance factors, reload (with the firing drill), the cartridge pool, map-preset weather, which soldiers fire (S3), the campaign-battle weather pick. Experience in battle ported (morale timers, fatigue; melee has none, CONFIRMED). Open: chevrons display, who raises experience, connecting campaign-battle weather once campaign battles start from the map.
- [ ] **PARTLY DONE:** Charge impact, mass and collision, push-through, and cavalry vs square. Done: charges count only at impact (CONFIRMED), charge bonus, defences stop charges. Open: mass/collision and push-through, cavalry vs square checks.
- [ ] **PARTLY DONE:** Routing, rallying, shattered units, and the general's aura and abilities. Done: morale states, rally test, routers leaving the map, general/army morale terms. Open: general abilities (RALLY/INSPIRE), general death/flight timers.
- [ ] Generals as units on the battlefield, with RALLY and INSPIRE (general abilities 0x11/0x12). The AI's GENERAL_SUPPORT tactic is
      decoded and waiting for this [AI_RESEARCH.md].
- [ ] **PARTLY DONE:** Unit abilities in the sim: square, skirmish mode, ability 0x10 (used by DEFEND_ABSTRACT), and group formations (GroupFormations). Done: square flag, skirmish behaviour (CONFIRMED), groupformations.bin default deployment. Open: ability 0x10, AI use of group formations (§6).
      [AI_RESEARCH.md].
- [ ] Unit pathfinding on the battlefield (around buildings, trees, rivers and slopes).

**Formations and abilities**
- [ ] Line, column, square, skirmish, fire by rank, wedge and the rest, exactly as unit abilities define them.
- [ ] **PARTLY DONE:** Special abilities: rally, inspire, rockets, and any others in the DB. Done: perform_special_ability (square, stakes, unlimber recorded), tech-gated abilities. Open: rally, inspire, rockets.
- [ ] **PARTLY DONE:** Deployables, if the DB defines any for NTW. Done: placement geometry (CONFIRMED), drawn with the original models, contact kills (CONFIRMED triggers), cover. Open: defences blocking movement (PROVISIONAL).

**Artillery**
- [ ] Cannon types, shot types (round shot, canister, shell), arcs, ricochet, limbering and unlimbering, horse artillery.

**Buildings and sieges**
- [ ] **PARTLY DONE:** Garrisoning buildings, firing from windows, building cover. Done: the defendable rule, walls, slots per fire line (CONFIRMED rule). Open: the garrison cap +0x6C, entry/exit rules (PROVISIONAL).
- [ ] Forts and walls: siege battles, gates and breaches.

**Battle flow**
- [ ] **PARTLY DONE:** Historical battle armies, deployment phase, victory and defeat, results screen (v1, merged 20df5dc). Open: reinforcements, allied AI, formation drag, victory grading, results-screen gaps [BATTLE_FLOW.md].
- [ ] **PARTLY DONE:** Reinforcements arriving from adjacent armies (campaign battles). Done: entry groups, joining when inside the playable area (CONFIRMED), scripted reinforcements. Open: campaign battles don't launch yet (§5).
- [ ] Time limits, victory points and capture points, where the original has them.
- [ ] Attacker/defender role per side (from the battle file or the campaign), map edges for withdrawing and routing, settlement-capture
      victory conditions, unit visibility, and the battle-difficulty attack bonus on AI armies. The battle AI already reads these
      (defaults to "attacker" until the model has them) [AI_RESEARCH.md §8].
- [ ] Battle speed, pause and the camera modes (free, general, unit) as in the original.
- [x] Historical battle scripts (Lua): every call in the shipped land scripts is implemented (0 unknown stubs, 0 ignored orders); Waterloo, Austerlitz, Dresden, Lodi and TUT_Land run with their scripts (0-A).
- [ ] **PARTLY DONE:** Battle tutorials. Done: TUT_Land's script runs (markers, selection/command/input handlers). Open: unit voices and UI highlights it asks for.
- [ ] Battle replays (save and watch).

- [ ] Floating unit banners as in the original (trace in Ghidra: banner model/texture, when names/men/morale show), replacing the PLACEHOLDER text labels (napoleon battle/labels.rs; user check 2026-10-10: they overlap when units stand close).
**From §0 (exit bar, 2026-10-15)**
- [ ] Animation cue → slot dispatch (INFERRED): the round-9 "dead" proof is void (the play paths are vtable slots); needs a user debugger sitting in a battle
- [ ] The enum behind `unit->+0x1B0` (value set INFERRED; static attempt closed)
- [ ] Gait: read n and k at `0x0054C691` (loop frame count; `TIMED_MOVE_PLANNED_SECONDS` INFERRED): debugger sitting, docs/FOR_USER.md.
- [ ] Battle tick counts the deployment ticks (exe Battle Model +0x58; PROVISIONAL on `Battle::tick`, ntw_sim battle model.rs).
- [ ] Battle formation traces (formation3): the friend query range `0x006C9C60` (PROVISIONAL; likely strip-based, not 150 m); the segment test `0x00501A60` and soldier +0x58 = man radius (INFERRED); `evade_angle` (`0x00533BA0`) uses positions, not box centres; soldier vfunc +0xB8 (from `0x00653560`) in deployment: are men put at their slots at once (INFERRED); the men's own headings (soldier +0x64) against the unit facing in `slot_cost` and `0x0055F160` (PROVISIONAL); the in-tick order of BuildUnitMoveOrder `0x0051A530` vs the squad's layout step (`0x00580A30` → `0x00581AE0`) (INFERRED); `0x0057AC10` assignment on a re-issue where `0x005516D0` holds (not modelled)

## 4. Naval battles
- [ ] Ship models, sails, rigging, and damage models (hull, sails, crew).
- [ ] Ammunition types (round, chain, grape), broadsides and reloading.
- [ ] Wind direction and speed, ship handling, and formations.
- [ ] Boarding, capture, surrender, sinking and fire/explosions.
- [ ] Naval battle UI, deployment and results.
- [ ] **PARTLY DONE:** Naval autoresolve and the campaign fleet hook. Done (0-B round 10): ported and running in the campaign (engagement, kill rates, damage, sinking, capture; SHIP_DAMAGE_INFO CONFIRMED). Open: ship potential, morale and range level from the runtime ship record (PROVISIONAL), ship captures, saving ship damage states (save writer queue).

**From §0 (exit bar, 2026-10-15)**
- [ ] Naval PROVISIONAL details: capture share weighting, captured crew base, gun counts of 8 ship models (needs a probe)
- [ ] Port the campaign battle wind-level roll (`0x00535B30`): a campaign sea battle takes the first sea_surfaces row for now (PROVISIONAL, napoleon terrain/water.rs `sea_settings`).

## 5. Campaign gameplay
**Turn and economy**
- [ ] **PARTLY DONE:** Turn loop, Lua events, income and upkeep, taxes, public order, construction, recruitment (PROVISIONAL formulas)
      [CAMPAIGN_PLAY.md]
- [x] The exact economy formulas: town wealth, GDP, tax efficiency, trade income (0-B: exact against the start positions and all original saves).
- [ ] **PARTLY DONE:** Trade: land trade routes, sea trade theatres, trade ships, blockades. Done: routes, nodes and trade fleets, blockades, prices, supply split, sea cap. Open: the importer-side limit (INFERRED rule in use).
- [ ] **PARTLY DONE:** Research: technologies, gentlemen in schools, and their effects. Done: states, rate (CONFIRMED), schools, completion, gates, tech stealing. Open: the building-chain match (INFERRED), the research UI command wiring.
- [ ] **PARTLY DONE:** Government types, ministers, revolutions, rebellions, emergent factions. Done: royal family and succession, vacated posts, ministers' dismissal/appointment, government classes in public order. Open: revolutions, rebellions, emergent factions, elections.
- [ ] Population, settlement growth, minor settlements, resources.
- [ ] Attrition (winter, desert), seasons, and turns per year per campaign.
- [ ] BUG: Turn steps may drop CharacterMoved / ForceDestroyed events before they reach scripts (ntw_script host.rs fire_all); verify.

**Military**
- [ ] **PARTLY DONE:** Army movement, pathing, intents (move, attack, merge, enter).
- [ ] **PARTLY DONE:** Zones of control, forced march, ambush, river crossings, retreat after battle. Done: zones of control with fog of war (CONFIRMED), river crossings, no retreat after battle (CONFIRMED). Open: forced march, ambush.
- [ ] **PARTLY DONE:** Generals: traits, ancillaries and retinue, command stars, death and wounds. This includes Napoleon's special rules. Done (0-G): traits and ancillaries gained by script triggers, natural death, succession of command, the wounded-and-returns Generals (+0x52C). Open: command stars, retinue limits, battle wounds.
      (e.g. returning when wounded).
- [ ] **PARTLY DONE:** Recruitment pools, replenishment, unit upgrades and experience. Done: the recruitment queue (CONFIRMED), character recruitment pools loaded and saved. Character pools run: refill timers and bonuses (CONFIRMED), HireGeneral (0-G). Open: replenishment, unit upgrades, experience, the pool step's phase (PROVISIONAL).
- [ ] Sieges on the campaign map: turns until surrender, and assaults.
- [x] Occupy, sack and liberate options after capture: the model (preview, damage roll, apply, liberation; CONFIRMED) and the capture screen (0-E).
- [ ] Capture previews: trace which class the public-order previews at CaptureOption +0x18 / +0x1C belong to (`0x00A61400` picks it by a faction test, `0x00A610C0`) against capture.rs:77 lower/upper (polish26 review)
- [ ] Director transfer garrison (director3): which changes mark a region recruitment belief dirty (`MarkBeliefDirty` `0x00CF0AB0`, 147 sites, `0x00CF0B10` propagation); ours rebuilds the belief each time (INFERRED in director.rs `land_recruitment_belief`)
- [ ] **PARTLY DONE:** Navies: fleets, naval movement, embarking and disembarking armies (done: Embark / Disembark with the
      original's landing rules, PATHFINDING_PORTS.md; open: fleet capacity, AI transport, naval invasions, naval battles).
- [ ] The campaign → real-time battle → campaign loop (the hook exists: `pending_battle` / `apply_battle_result`).
- [ ] **PARTLY DONE:** Autoresolve. Done (0-B): land autoresolve and the no-retreat rule CONFIRMED; naval autoresolve ported. Open: naval ship potential and morale inputs (PROVISIONAL, see §4).

**Diplomacy and agents**
- [ ] **PARTLY DONE:** Diplomacy: treaties, trade agreements, alliances, protectorates, gifts, region exchange, attitudes and the AI's responses. Done (0-B): attitude factors and drift (1,469 steps match vanilla pairs), war/peace/alliance/trade/embargo/access/gifts/payments/protectorate rules, computed factors (religion and government 506/506; leader 484/506), allies called into wars, treaty money each round. Open: the faction-leader factor (France's leader post), region exchange (TransferRegion rejected in review: needs 0x00B449F0 flags), the negotiation UI (0-E; object ported 709dad8, not checked in game), the AI's responses (§6).
- [x] An effects layer, so difficulty handicaps and building/tech effects really apply (GDP, recruitment and upkeep costs, research,
      attrition, policing). Today only the AI's own budget uses the handicaps [AI_RESEARCH.md].
- [x] Region adjacency in the model, from the map's region outlines (0-B). (The AI still uses the 4 nearest settlements: §6.)
- [ ] **PARTLY DONE:** Agents: spies, gentlemen and the others, with their actions (sabotage, assassinate, duel, research) and success formulas. Done: chances and rolls (CONFIRMED), assassinate, duel, spy, sabotage, tech stealing, reactions. Open: conversion, stealth/spotting, the UI and AI hooks.
  - [ ] Trace how the exe picks a building's protector (`building_protector`, ntw_sim agents.rs): in ours a target whose subterfuge beats every other candidate's is his own protector (CHARACTERS_FIDELITY §15)

**Missions, events and victory**
- [ ] Missions, events and rewards (Lua and DB), and the event messages.
- [ ] The advisor, campaign and battle, with speech.
- [ ] Victory conditions for each campaign: Italy, Egypt, Europe, Waterloo, Coalition, and the DLC campaigns if installed.
- [ ] The prologue and tutorial campaign flow, and its unlock progression.
- [ ] Campaign cutscenes and videos.

**Saves**
- [ ] **PARTLY DONE:** Save and load: our saves round-trip and keep script state, economy, characters, research, families, sight and names. Open: compatibility with the original game after AI turns (§1, running).
- [ ] Restricted units (`add_restricted_unit_record`, saved in `EPISODIC_RESTRICTIONS/UNIT_RESTRICTIONS`) live only in the script state (loader → napoleon/src/campaign/scene.rs → `ScriptState::restricted_units`); the AI reads them, the model's recruit command and the recruitment panel don't. Trace where the exe checks them (as `0x009CDA90` for building levels), then give them one home, a `World::restricted_units` that model, UI and AI read (ntw_campaign/src/lib.rs ~210).
- [ ] A save's restricted building levels reach the model before the campaign scripts load (they are in the loaded `World`); the exe's order of restrictions vs. script load is not traced (napoleon/src/campaign/scene.rs ~430).

**From §0 (exit bar, 2026-10-15)**
- [ ] `CDIR_INTENTION_TRANSFER_REGION_OWNERSHIP` (`0x00B93840`, applied via `0x00B58A30`): merged 2026-10-11 (region-transfer: campaign director in the model behind seam `director.region_transfer`, TurnStep::Handover, cdir_configs keyed campaign_faction_variable `0x00FCD150`, CDIR_INTERFACE + counter in .esf `0x00B94170`/`0x00B9D6E0`/`0x00B9B6C0`, startpos director RNG, 4 ORIGINAL BUGs fixed; CAMPAIGN_FIDELITY.md). Left (PROVISIONAL): the stored relation copy `rival_score` reads (next line), the siege test (§5 "Sieges on the campaign map"), sea mobiles, the director's other RNG users (below)
- [ ] Trace faction +0x754, which `0x008AB6C0` clears (two candidate meanings, broadcasters2 notes); model the clear
- [ ] Trace when a re-emerged faction's posts get holders (ours leaves them empty, INFERRED)
- [ ] Trace the SettlementOccupied events (`0x00B58560` / `0x00B58890` / `0x00B58C00`) against FactionDestroyed (`0x00A64AC0`): ours sends FactionDestroyed first (INFERRED)
- [ ] `0x008AB6C0` parts not modelled: the killer path (+0x924, `0x00BA80E0`), the multiplayer report (`0x009C7660`, event +0x120)
- [ ] Construction left (construction2): slot garrisons (`0x00A64440`, slot +0xC0); port blockade (`0x00A784E0`); slot +0x1EC; the other SetBuildingHealth callers (`0x00B541E0`); BuildingCompleted posted per list entry (`0x00B78230`); the +0xBA0 event; what clears an item's active byte (+8); the walls damage-object restore; research stopping on an upgrade to another building (`0x00B4D4D0`); `docked_port` PROVISIONAL
- [ ] Trace the faction region-list order (+0x778 / +0x77C) at startpos, at load and on gains and losses, so the round end walks regions in the original's order (ours: id order, PROVISIONAL, ntw_sim turn.rs; construction2)
- [ ] Trace whether GDP (`0x00A6AFC0`) reads the walls (recruit-followups)
- [ ] Map callers of the recruit gates `0x00AA1660`/`0x00A27960`: `0x00A87640`, `0x00AAB440`, `0x00A799A0` → `0x00B43FC0`, `0x009B74E0`, `0x009DF476`
- [ ] Recruit cost: which commands the three handlers near `0x00936530` are (UNKNOWN, recruit-cost2)
- [ ] Treasury callers still not modelled (treasury4 traced): missions credit `0x00A06050` (needs mission completion); captured-ship sale / placement (`0x00B38F10`, needs the state-6 force rebuild); MP results counter FACTION +0x954 (`0x008F56C0` / `0x00B298F0`, with MP results)
- [ ] Undock: `0x00A8C0B0`'s three notifications are not modelled (the handle clear and port slot hand-back are, polish21); the war kick-out of docked navies (`0x008DA179`) and the port order caller `0x0094D4F0`; docking within 1 map unit and the battle undock before captures are PROVISIONAL (treasury3)
- [ ] Map forts (the `fFort` chain, separate from settlement walls): `BuildFort`/`UpgradeFort` levels PROVISIONAL; nothing in the shipped exe builds a map fort (UI_FIDELITY.md)
- [ ] Region details: Migration (needs theatres); a `region_wealth_factors` table from mods; base_capacity 0 drops the population to the minimum (check the exe)
- [ ] Fort as its own selection

## 6. AI
- [ ] Our campaign AI's Austria attacks in its first turn, so the diplomacy screen's Power reads Mighty instead of the original's Terrifying (self-checks2 (f), IN_GAME_CHECKS_2026-10-10.md); trace the original's turn-1 AI choice
- [ ] **PARTLY DONE:** Battle AI v1 and campaign AI v1, merged. The campaign AI plays at End Turn; the battle AI has the original attack FSM thresholds and think rhythm.
- [ ] Battle AI in full:
  - artillery use, squares against cavalry, using terrain and buildings;
  - flanking plans, reserves, retreat;
  - sieges and naval battles.
- [ ] Campaign AI in full:
  - diplomacy offers, trade, research (RESEARCH_TECHNOLOGY ported f5e302a: structure CONFIRMED, tech pick and school PROVISIONAL);
  - agents, naval invasions, sieges;
  - taxation.
- [ ] The original's tactic state transitions, think rates and priority constants (Ghidra) [AI_RESEARCH.md §7]
- [ ] Difficulty levels (campaign and battle) and AI handicaps, as the original applies them.
- [ ] Scripted AI controls (`release_control`, scripted battle units, frozen armies).
- [ ] Battle AI mounted tests (horsefright, INFERRED in ntw_ai battle): the skirmisher arm (`0x007D2CF0`, class 0x16) uses the on-foot test `0x0055C1C0`; trace the mounted-share builder (+0x78/+0x84); defence contact uses the per-soldier vfunc +0xF0

**From §0 (exit bar, 2026-10-15)**
- [ ] Stored CAI relation belief (`0x00A89E00` faction +0x100 list +8, written by `0x00ABAFB0`): merged 2026-10-14 (cai-belief: belief store + dirty mark in the director state, per-broadcaster marks CONFIRMED `0x00A8F320` → `0x00A88CE0`, capture `0x00A64AC0`/`0x00B62120`, misdeeds `0x00B0D870`/`0x00B67FD0`/`0x00B0D700`/`0x00B67E60`). Left: the turn-change mark (INFERRED, `0x00CF7F60`, propagation `0x00CF0B10`: debugger sitting); the other 184 readers of `0x00A74A20` (PROVISIONAL); the +0xA8 enemy-majority flag
- [ ] AI shopping shroud (treasury4): the naval goal that creates CAI_BDIM_REPAIR_NAVY (`0x00CDD540`; `ai_repair_navy` has no runtime caller yet); the gate `0x00A8B2D0` on its 11 other callers (incl. `0x00A67890`); trace CAI slot +0x104 (the gate's position, INFERRED at capture.rs ~472); the CAI unit-list order (INFERRED); the ESF CAI_INTERFACE bool is neither read nor written (no load warning)
- [ ] Trace the AI diplomatic budget pot (CAI +0x140, `0x00AAF570`; PROVISIONAL in the AI's payment evaluation, deals-fix)

## 7. User interface
**Front end**
- [ ] **PARTLY DONE:** Main menu, single player, Load Game, Napoleon's Campaigns and Battles, Coalition, Options.
- [ ] **PARTLY DONE:** Custom battle setup (armies, map, settings), army selection and unit purchase. Done: settings page, armies page
      (presets, recruit cards, experience, validation), setup files (save / load, `.sp_default`), starting the battle in our
      engine, text entry (Save army works from the GUI). Open (0-E): the unit size option in battle, the category
      mask, naval custom battles (UI_FIDELITY.md "Where I am").
- [ ] Multiplayer pages (see §10).
- [ ] **PARTLY DONE:** Credits (`BuildCredits`), tutorials, loading screens with art and tips. Done: credits (21 pages from credits.xml). Open: tutorials, loading screens.
- [ ] Graphics options that really change our renderer (resolution, windowed, quality presets, the anti-aliasing and shadow settings
      the original offers).
- [ ] Key bindings page and custom key sets.
- [ ] **PARTLY DONE:** Tooltips, list scrolling and clipping, cursor states, all keyboard shortcuts. Done: tooltips (shared hover path, drawn last), clipping, text entry. Open: list scrolling, cursor states, shortcuts, the hover delay.

**Campaign UI** (v1 merged 6eaf4b6: HUD fixes, army review, construction and recruitment, labels, tooltips, lists)
- [x] The "ygT" text, the 2 HUD script errors and the radar map script error (`template.map_image.lua:197`) are fixed (0-E: RegionsInTheatre and the theatre calls from the exe).
- [ ] **PARTLY DONE:** Unit cards, settlement panel and recruitment panels exist; building browser details are done. Open: the browser tree, and a playability pass of building/recruiting from the settlement panel (user, 2026-10-04: clicking a settlement doesn't do much).
- [ ] Character details (traits, ancillaries), agent actions.
- [ ] **PARTLY DONE:** Diplomacy, trade, research, finance and faction summary panels, objectives, event messages, the pre-battle panel. Done: government (taxes, trade tabs), technology, objectives, diplomacy faction list and details. Open: negotiation, finance and faction summary, event messages, the pre-battle panel.
- [ ] **PARTLY DONE:** Map labels, tooltips, the radar/minimap. Done: radar with owner colours, view outline and click-to-move; tooltips. Open: label details.
- [ ] The in-game encyclopedia/help pages.

**Battle UI** (v1 merged: unit cards, orders bar, speed controls, deployment, results; the items below remain)
- [ ] Unit cards, order buttons, formations and abilities, speed controls, the radar/minimap.
- [ ] Deployment UI, the results screen, kill counts and the heroes list.
- [ ] Unit info panels and tooltips, and the battle advisor.
- [ ] The unit cards' `SetInitialState` gets our info table where template.BattleUnitCard.lua takes its naval flag (`m_naval`, bytecode at its line 55; analysis/battle/BATTLE_FLOW.md "Cards"): trace what the engine passes (both arguments) and pass that (ntw_script/src/ui/battle_prelude.lua `__ntw_battle_create_cards`).
- [ ] Battle unit outlines (user screenshot 2026-10-10): `napoleon` battle/view.rs ~1094 draws a red/blue debug box round every unit (untagged; the original shows no such box: replace it with the original's unit markers/selection from its UI and effect data, or tag it PLACEHOLDER), its edges are straight lines that sink into hills, and units whose men aren't drawn still show the box and are fired at: find why their figures are missing (not loaded, hidden by line of sight, or off-field).

**From §0 (exit bar, 2026-10-15)**
- [ ] Agent attribute icons: merged `25a6b21` (`agent_attributes` icons, main attribute, `PrimaryLevel` CONFIRMED);
  waits for its in-game check (HANDOFF). Left: rank PROVISIONAL for non-agents, UI skin folders not modelled
- [ ] The battle HUD never calls `set_cursor_position`: `Component.CursorPosition` returns nil x, y in battle
- [ ] Trace the campaign `Time` counter (`0x00A0EB80`; pulse `0x009D5F31`) against the main-loop clock; if it is the same struct, share the battle `advance_ui_clock` (campaign's is uncapped and fractional, PROVISIONAL)

## 8. Video (Bink)
- [x] A Bink 1 (`.bik`) decoder recreated as our own pure-Rust code from the format spec and Ghidra (73/73 movies, 229 audio tracks; see §1) (the game ships `binkw32.dll`; we
      don't use it).
- [ ] **PARTLY DONE:** Intro videos, the front-end background movie (`Frontend2.bik`), campaign and battle cutscenes, loading movies. Done: intro order (CONFIRMED), front-end movie, full-screen sizing. Open: campaign/battle cutscenes, loading movies, the skip rule.
- [x] Movie audio routed through our audio system with the original movie volumes (merged d5251e6, BINK.md) [AUDIO_FORMAT.md]

**From §0 (exit bar, 2026-10-15)**
- [ ] The movie skip rule: trigger CONFIRMED (Escape key-up while the intro queue plays, `0x0048A650`; user check 2026-10-09: Escape skips in ours); the can-stop flag `+0x58` still INFERRED

## 9. Audio
- [ ] **PARTLY DONE:** Sound banks, decoding, front-end music, UI clicks and hover sounds, 3D volleys, the music state machine, the Miles mixing (0-C), battle music looping without restarting (merged `d6c843b`, user-checked 2026-10-07). Open: bank selection, the anim cue dispatch [AUDIO_FORMAT.md §7, MIDDLEWARE_VERIFY.md §3].
- [ ] **PARTLY DONE:** The Miles mixing rules 1:1: gain law, 2D/3D multipliers, falloff, pan, low-pass, doppler, ducking, speed-of-sound delay. Done (0-C, CONFIRMED vs the exe): all of these and the loudness. Open: which speaker setups open (headphones, 0-C), low-pass at the output rate (PROVISIONAL).
- [ ] Footsteps and group movement by ground type, projectile impacts, explosions, cannon.
- [ ] Unit voices and officers' commands, battle ambience and weather.
- [ ] Advisor speech, and campaign ambience in full (emitters exist).
- [ ] Music by battle and campaign phase and state, with the original fade timing.
- [ ] Per-soldier firing sounds driven by animation cues (`.anim_sound_event`, cue meanings UNKNOWN).
- [ ] The bank selection rule (Ghidra).
- [ ] Trace in Ghidra the exe's sound start-rule order (repeat limit, probability, at-once, then file/pitch/delay RNG draws); ours is the round-13 order, untraced.
- [ ] Whether the exe's at-once / voice limits count finished one-shots not yet freed and voices fading out (ours: `Player::live` counts both; untraced).

**From §0 (exit bar, 2026-10-15)**
- [ ] The sound bank query: reopened 2026-10-09 (the entry reader `0x00E28DF0` is reached through vtable slot `0x013A9944` from `0x00E256D0`); next a main-menu debugger watchpoint (MIDDLEWARE_VERIFY.md §3 round 14)
- [ ] The headphones multiplier: CONFIRMED from the `sound_provider` pref and Miles' speaker setup (`0x01004390`, `0x01006600`); left INFERRED: which speaker setups Miles opens on a machine (a failed one shifts a saved index)
- [ ] `EnumerateSoundProviders` (ntw_script ui_prelude.lua:869) returns `{}`; it should list the providers from `0x00DA1270`

## 10. Multiplayer
- **Decided (user, 2026-10-03): multiplayer is only between copies of NapoleonRust.** No protocol compatibility with the original
      game, and no Steam networking or matchmaking: our own networking layer.
- [ ] Networking layer and lockstep determinism checks.
- [ ] Lobby, matchmaking or direct connect, and chat.
- [ ] Multiplayer land and naval battles, unit purchase screens, ranked/unranked as the original offers.
- [ ] Two-player Coalition campaign, and drop-in battles, if the original supports them.
- [ ] Desync detection and multiplayer replays.
- [ ] Multiplayer campaign army caps: the lobby's campaign (`0x004AA900`, game type 3) passes a fixed 20 / 14 at the lobby's unit scale (`0x004A6540`); `ForceCaps` has no such constructor yet (MODDING_AUDIT, self-checks)

## 11. Modding (goal: original mods work out of the box, plus easy modding)
- [ ] Mod loading merged (layered Vfs, `user.script.txt` parser, DB merging, loc rules, precedence graph; MOD_LOADING.md §2.1, §3.1, §7). Left: four INFERRED in `ntw_formats` pack/mods.rs (line encoding :95, `;` splitting :185, install-root paths :457, pack-name matching :485 = vfunc+8 in `0x0105BD40`): trace each
- [x] Wire it into the app: `--mods`, `--no-mods`, `--list-mods`, the `mods\` folder with `load_order.txt`.
- [ ] Exact original pack priority and DB merge order (Ghidra) [MOD_LOADING.md §5]: precedence graph CONFIRMED (2026-10-10); left: the dependency closure for install packs, in-file duplicate rows in graph-node packs
- [ ] Test with real original-game mods (unit packs, overhaul mods, map mods).
- [x] One table path: about 20 readers read only `db/<x>_tables/<x>` and miss additive and `bob_` mod files (ntw_script UI `table`/`small_table`, mount.rs:64, unit_model.rs:121, vegetation.rs:228, names.rs:53, battle/setup.rs:246, model_viewer/source.rs:141); route them through the merged GameDatabase, and add a test that fails when code outside the loader reads a `db/..._tables` path (the guard, CLAUDE.md "Modding seams") (merged 2026-10-10: `ntw_formats` db_folder.rs, guard `napoleon/tests/table_path_guard.rs`)
- [ ] Check each typed `db_record!` key against its loader in the exe (MOD_LOADING.md §3.2 method): junction tables keyed by one column (e.g. `BuildingEffect`, `BuildingUnitAllowed` by `building`) collapse a mod's added rows (polish4 review)
- [ ] DB row hash order and in-file duplicate keys (PROVISIONAL; MOD_LOADING.md §6 start-up debugger run)
- [ ] Our DB reader does not port the header flag byte 0: a mod file with flag 0 and rows drops the rows loaded before it in the original, ours keeps them (`ntw_formats` db.rs:10/:224, db_folder.rs:62; trace the exe reader first)
- [ ] Optional open formats: glTF models (the one open model format, Blender exports it directly; no FBX or format of our own, user 2026-10-10; FBX import only as a later converter to glTF if modders ask), image heightmaps, hi-res texture overrides.
- [ ] Modding docs and our own tools (pack editor, DB editor, map tools, a campaign map editor with live preview), so modders don't need the original's closed tools.
- [ ] **Custom campaign maps** (user, 2026-10-09: a whole new world, e.g. a Japan map; design in docs/DESIGN.md §3.5.1)
  - [ ] Move the `.esf` and original-map coupling behind the importer (main part merged 2026-10-10: own save format, `CampaignSource` seam, `MapDisplay`; the rest in the next line): audit done 2026-10-09
        (analysis/modding/MODDING_AUDIT.md §3, 15 sites, ~9 items). First our own model save format (saves patch the
        source ESF tree, napoleon campaign/play.rs:703) and a `CampaignSource` seam (scene.rs:253-321, `read_info`, the
        `CampaignMap` type).
  - [ ] Left from the coupling audit (MODDING_AUDIT.md §3.4): the UI's `theatre_bounds` still parses `regions.esf`; no fallback assets for arrows/rivers; ESF names and index orders in the model; victory options from the ESF; `UNIT_CLASSES` import
  - [ ] Naming traces: does the unit constructor `0x0088B850` spend a name pair on a general hire (bodyguard naming INFERRED from saves); the land promotion builds a new unit (`0x0087FEA0`) carrying the old unit's name, ours reuses the old unit
  - [ ] Open campaign format spec and loader (campaign.toml, regions.png, heightmap.png, settlements, factions,
        armies, diplomacy, events) → `CampaignModel`.
  - [ ] Generator: borders, adjacency, region meshes, pathfinding grid, sea lanes and coastlines from the images,
        cached by source hash; the same output types the importer produces from the original's map files.
  - [ ] Campaign map display from generated data (terrain, ground layers, coasts, rivers, labels).
  - [ ] Custom campaigns listed in the campaign menu next to the original's.
  - [ ] Exporter: the original's campaign (with mods) to the open format.
  - [ ] A small sample custom campaign as a test and a modding example.
  - [ ] Later: converters from other Total War games the user owns (e.g. a Shogun 2 map); separate format investigation.
- [ ] **Generic engine** (user, 2026-10-09: a platform to build any Total War game; CLAUDE.md intro): audit done
      (analysis/modding/MODDING_AUDIT.md §1, ~45 sites, ~14 items): one per-campaign feature table for the nine
      `spa_napoleon` / `mp_eur_napoleon` / `spa_france` switches, unit category and class lists from data in one place
      (4 and 3 copies today; an unknown category counts as artillery), the calendar from `turns_per_year`. Move each
      into data, defaults unchanged. Merged 2026-10-10: feature table (`ntw_sim` campaign/features.rs), `unit_kind` as the one category source, calendar and seasons (`0x008A98B0`), upkeep as the exe (`0x008F9B10` / `0x008F9D00`). Left:
  - [ ] Trace the remaining exe uses of `spa_napoleon` (72 xrefs at `0x01315FF4`; ~11 ported) into the feature table
  - [ ] The front-end campaign list, default faction and unlock from data (MODDING_AUDIT §1.2, with the custom-campaign menu)
  - [ ] AI manager/personality fallback; a category behaviour column of our own; `balance_group` against `cdir_unit_balance_groups` (PROVISIONAL)
  - [ ] Era enums from data (shot, ability, drill, ground, weather, muzzle effect); government, agent, research, tax, ship-gun and MP-category tables
  - [ ] An upkeep test against a start position or save (economy_fidelity covers other income only)
- [ ] **Replaceable systems** (user, 2026-10-10: mods change or add whole systems, e.g. how population works, new mechanics; a fork can become a new game): each model system runs through one named rule seam that a mod can replace or extend from data or the extended Lua API, with mod-owned state carried in saves and kept deterministic for multiplayer; vanilla runs the 1:1 rule.
  - [x] Foundation (merged 2026-10-10, DESIGN.md §3.3.1): `ntw_sim` seam.rs, campaign/seams.rs, mod_state.rs; population growth is the worked example
  - [ ] First, before the next refit: generate each seam's field, default, registry arm and `chains()` entry from one macro (a missing `chains()` entry silently drops the seam from the multiplayer check)
  - [ ] Read the `_rule_seams` table and `campaign.toml [rules]` into `CampaignRuleRegistry::seams` in `attach_rules`
  - [ ] Multiplayer peers compare the rule chains (and a hash of each script's content) at game start; saves record `chains()` and warn when loaded under a different rule set
  - [ ] DESIGN §3.3.1: Lua rule state keeps no globals between calls (unsaved, unhashed state); say how a new mechanic changes vanilla state (a seam that does nothing in vanilla can only write `ModState`)
  - [ ] `ModScope` campaign-unit scope; battle mod state with `BattleSeams`
  - [ ] Refits, one per system: population factors, public order, economy, recruitment, diplomacy, research, battle (`BattleSeams`)
- [ ] **Extended Lua API** (user, 2026-10-09): the original's API stays exact (same names, arguments, results and events, so shipped `.luac` and mod scripts run unchanged on the Lua 5.1 VM); a new namespace adds more on top: read and write any model value, hooks for every game event, new commands and rules, UI built from script, campaign and battle setup for custom campaigns, and no fixed limits. One implementation per rule: the old API calls the same model code as the new one.
- [ ] **No engine limits** (user, 2026-10-06; see Goal in `CLAUDE.md`). Defaults stay 1:1. Merged 2026-10-10 (`ntw_sim` limits.rs, MODDING_AUDIT.md §2.6): wide ID types, caps as `_kv_rules` data (army/navy #23/#24, custom battle 20 land, ships 6/8/10/20, reinforcements 20), required by `UiScriptHost::new`.
  - [ ] Make the hardcoded counts data-driven: audit done 2026-10-09 (analysis/modding/MODDING_AUDIT.md §2, ~25 sites,
        ~8 items: the 20-unit literals in army_setup.rs beside `MAX_UNITS_PER_FORCE`, `MAX_FIGURES` 240 men drawn per unit
        in battle/view.rs:40, small id types).
  - [ ] Record the original's limits in the notes (CONFIRMED from the exe where possible): faction, region, religion and
        culture caps, units per army or fleet, unit size scale, battle unit and soldier caps.
  - [ ] Turn the gameplay caps into moddable data values (not player settings; user, 2026-10-10), defaulting to the original's numbers: the 20-unit army and
        fleet, the unit-size options and the battle unit cap.
  - [ ] Saves, the UI (lists, scrolling, faction colours and flags for many factions) and multiplayer must handle counts
        past the original's limits.
  - [x] `ForceCaps::new_campaign`: CONFIRMED (self-checks 2026-10-12; `0x00488720` → `0x00878060` → `0x00873BD2`, MODDING_AUDIT)
  - [ ] Rest of MODDING_AUDIT §2.1 (`MAX_QUEUE`, road levels, 4 children, 3 research threads) and §2.2 render caps (`MAX_FIGURES` 240 men drawn per unit ...)
  - [ ] Test with a synthetic mod that goes past every limit (e.g. 300 factions, 1,000 regions, 40-unit armies,
        1,000-man units).

**From §0 (exit bar, 2026-10-15)**
- [ ] Model `0x00EDC000`'s other drops: a null on-screen name and an unknown unit_class / category (recruit-followups)
- [ ] Trace a writer of DATABASE_TABLE +0x38 / whether the early return `0x00E9BD95` is reachable (`0x00E9B700` "goes on with the next row" INFERRED, ntw_data database.rs ~296)
- [ ] Portrait deck builder `0x00A1F890` for maps without the start-pos portrait allocator (custom maps); port its Fill ORIGINAL BUG (count = last + 1) with it (polish11, CHARACTERS_FIDELITY §14)

## 12. Platform, release and quality
- [ ] Find the install automatically (Steam library folders) and ask if it's not found. Never write into it.
- **Decided (user, 2026-10-03): no Steam features** (no achievements, no Steam API, no Steam overlay). Ownership: players must own the
      game on Steam, because all assets come from their own install. The game starts only with a valid install and never ships assets.
- [ ] Performance targets on big battles (e.g. 10,000+ men), plus a release-build profile.
- [ ] Logging, crash reports and a settings reset.
- [ ] Packaging: ship only our executable, never Creative Assembly assets.
- [ ] Clippy and cleanup passes, and docs for building and running.
- [ ] File-size guard: a test (like `napoleon/tests/encoding.rs`) that fails when a non-test `.rs` source file passes 3,000 lines, so no file grows like `ui/campaign.rs` (7,985) again. Its allow-list is empty: the split (2026-10-08) left no source file over 3,000 lines.

## Polish
Non-blocking review findings, one line each (CLAUDE.md "Done means"). A worker editing a file clears that file's lines.
- [ ] tools/self_check.sh (polish27): a rename by another worker sharing Ghidra still FAILs this branch; a function named in notes without its address is not checked (no name-to-address source)
- [ ] napoleon battle/setup.rs ~975 (horsefright review): a unit flagged `is_cavalry` whose category is not mounted gets `MovementClass::Mounted`, where `0x006543D0` keys on the category alone; no vanilla unit hits it
- [ ] trade.rs ~267 (leftroutes review): `trade_home_value` builds each importer's routes with `trade_routes_of`, then `trade_pair_value_with` builds them again; compute once. The home term also applies with no domestic map entry, where the exe needs one
- [ ] Ghidra (formation3): V5 for 0x00653560, 0x0055AD60, 0x0054FDD0, 0x0057B000 once named; 0x005373F0 / 0x00502770 / 0x005281D0 deductions are structural (void* this, split register vars)
- [ ] Ghidra (formation4): the BattleUnit, BattleUnitTypeInfo and UnitSpecialAbilitySet structs are minimal; type their fields
- [ ] ntw_script ui/campaign/army.rs `unit_entry` ~48/63 (polish35): the repair fields and `IsNaval` use `force.is_navy`; trace which unit virtual picks `0x008DF820` vs `0x008DF190` and use the unit's own type
- [ ] Campaign HUD date (savename trace): the "Early/Late month" text can use the loc keys `random_localisation_strings_string_round_id_*` (analysis/fidelity/UI_FIDELITY.md §5.1)
- [ ] ntw_sim battle (fightcav review): model.rs ~1512 says "CONFIRMED in 0x0053C720" while passing `e.charging`, whose link to `0x0055AC20` is INFERRED (morale.rs ~439): tag the comment; morale.rs ~134 `ATTACKED_FRONT` also holds flank and rear values: rename
- [ ] (running: polish36) Ghidra (dipcounters): V5 write-ups for the diplomacy manager reader `0x00AE8E10`, writer `0x00AFC160`, `0x00B0E420`, `0x00B13840`, `0x00B67B20`
- [ ] Ghidra (polish31): `GrantMilitaryAccessToFaction` `0x00B444B0`'s refresh calls `0x00B795A0` and `0x00B67490` are unnamed; the faction registry `this` of `FindCampaignFactionByKey` `0x00955B70` has no struct
- [ ] Gait (gait-blend2): 6 Ghidra functions at 0-37% completeness, `0x005DD450` unnamed; merge `man_level` and `mount_level`; tree/water probes index the palette by ground; `fatigue_effect` runs twice per tick; tga.rs cmap padding maps to road
- [ ] Raise the generic-engine Ghidra functions' completeness: upkeep done (polish7), 9 functions left (analysis/modding/MODDING_AUDIT.md; FUNCTION_DOC_WORKFLOW_V5)
- [ ] Two-file key tests: which same-key row survives is covered (polish8); still unchecked: that each typed ntw_data table's `key()` is the key the exe merges by
- [ ] `CursorPosition` (ui_prelude.lua ~281) and CampaignUI's label `ScreenPos` (Labels.lua) give screen pixels while the campaign HUD's Position / MoveTo use the 1280x960 scripts' frame; read what the original returns on a wide screen.
- [ ] `unwrap()` audit on paths that read install or mod data (ntw_formats, ntw_data, pack loading): a malformed file or broken mod must log one error and skip, never panic. Unwraps on the code's own invariants and in tests stay.
- [ ] `last_income` is not written by the save writer (PROVISIONAL), its meaning changes after `settle_round` (categories 5, 7, 11 only), and `.items.last()` assumes the history ring is saved oldest-first (ntw_sim campaign).
- [ ] Army recruitment: queue cards never show "Paused" (`0x00A00775`); army-recruit-tab's 34 Ghidra functions at 0-53% completeness
- [ ] Debugger: who subscribes to the signal at the end of CancelRecruitmentItem (breakpoint `0x008DBB59`); it passes the already-freed item when the cancelled one was last (ORIGINAL BUG only once a listener reads it)
- [ ] Director (ntw_sim director.rs): queued transfers whose receiver is dead stay in the queue (whether the exe purges them is untraced); ntw_campaign director.rs `fill`: the old-version RNG step's place among the fills is unverified (no vanilla file uses it)
- [ ] director3 follow-ups: `PORT_SLOT_TYPE` "port" is hardcoded as in the exe (`0x00BE6EF0`), a data value if a mod needs another port slot type; Ghidra V5 for `0x008AAB60` CheckUnitRequiredTechnologiesResearched (44.7) and `0x00A91FC0` IsSlotHeldByOtherThanRegionOwner (54.4); CAMPAIGN_FIDELITY director3 notes: the type is at `CAI_BUILDING_SLOT` +0x100 (ctor writes [0x40], +0xFC = model slot), `0x00C1BF00` → `0x00A63FD0` reads +0xFC not the type; the belief doc omits the siege flag 0x20 (never set)
- [ ] host.rs CreateFromComponent (polish32): trace how `0x01026F90`'s list reader treats a non-string image entry (UNKNOWN; ours skips it and logs once)
- [ ] Diplomacy relations list (self-checks2): grey header with ▲ sort arrow and column divider; alternating row bands
- [ ] Ghidra (director-rng): apply the proposed names listed in CAMPAIGN_FIDELITY.md (director desires section) with their V5 write-ups; `vanilla_director_floats_all_parse` (ntw_campaign economy_fidelity.rs) prints every row
- [ ] Battle HUD (hud-state): draw the `set_greyscale_t0` / `red_pulse_t0` / `unitcard_burning_t0` UI techniques (UI material); text shaders per state (`+0x12C`, `0x01014FB0`; layout default untraced); `time_request_changed` `is_multiplayer` PROVISIONAL until multiplayer battles; the R reset key rebuilds the sim without telling the speed ring
- [ ] polish21 follow-ups: trace the exe reader of the saved bonus type-7 sums (the junction table is compiled; no evidence save has a type 7); FORT_ARRAY id: the save writes an ESF i32 but our `FortId` is u32 (check reader `0x00AEB190` and our save writer); type the deal and context argument classes of `0x00C4AB20` / `0x00C12EE0`; Ghidra `0x009DB7C0` V5 at 67
- [ ] A navy merged into another at the same port hands the port slot back while the survivor stays docked; the exe does the same (destructor `0x0088EF90` calls `0x00A8D830` on the +0xE0 port handle). Not traced: whether a merge path re-docks the survivor (polish21)
- [ ] formation review: Ghidra V5 left on register temps for `0x0053D1A0` (9/29), `0x0053C3D0` (41/61), `0x0053C180` (48/55)
- [ ] Battle view (polish18): the view has no entity anim state (+0x1B8), so ntw_formats `display_time` (the pre-switch checks, `0x00774228..0x0077425D`) is not called yet (PROVISIONAL on `LoopKind`); the meaning of states 0x15/0x16 (from data via +0x630) is untraced

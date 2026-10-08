# ntw_sim: the deterministic game model

`ntw_sim` is the simulation "model" of NapoleonRust: pure Rust (std only), with no Bevy, no I/O, no threads and no hash-map iteration.
The same inputs always produce the same outputs. The Bevy app (the "display") reads this state and draws it (see `docs/DESIGN.md` §1).

Build and test (use the separate target dir):

```
set CARGO_TARGET_DIR=%USERPROFILE%\Documents\NapoleonRust\target-w5
cargo test -p ntw_sim
cargo clippy -p ntw_sim --all-targets
```

**No game data is in the code.** Values such as `kv_morale` thresholds, `kv_fatigue` rates and unit stats come in as plain structs (`KvMorale`, `KvFatigue`, function parameters).
Their `Default` is all zeros. The tests use numbers that are obviously made up, and each test says so.
The only numbers written into the code are constants CONFIRMED from the executable, each with its citation.

Tags used below:
- **CONFIRMED**: read from the original executable's code.
- **INFERRED**: a reasonable reading of the evidence, not proven.
- **PLACEHOLDER**: our own stand-in where the original is UNKNOWN. Replace it when the real behaviour is found.

## Modules in plain words

| Module | What it does |
|---|---|
| `rng` | The game's dice. It is a 32-bit LCG (`state*214013+2531011`) that returns 16 bits. The helpers (`uniform_below`, `percent_0_100`, `int_range`, `unit_float`, `float_range`) copy the original's odd details, such as rejecting *low* values. |
| `fixed` | `Fixed20`: campaign-map positions stored as integers in units of 1/1048576. |
| `calendar` | Dates (year, season, month, early/late) and the 24-turns-a-year calendar. `advance_turn()` moves half a month. The season is kept as stored because its rule is unknown. |
| `battle::speed` | Battle speed ×0 / ×0.4 / ×1 / ×2 / ×4 and the cycle order. |
| `battle::morale` | The 8 morale states (Impetuous … Shattered). `evaluate()` recomputes a whole-number morale from base, stat and effects, then moves at most one state using strict upper/lower thresholds. `modifiers()` adds the casualty, attack-direction, category, cavalry and blood effects. |
| `battle::fatigue` | The 6 fatigue states (Fresh … Exhausted). Each action adds a fatigue amount, slopes scale it with integer percent math, and the result is clamped. A unit's fatigue is the mean over its soldiers. |
| `battle::rules` | `KvRules`, the `kv_rules` table with all 94 keys, plus C-style helpers: integer division toward zero (`c_div`) and x87 round-half-to-even (`x87_round`). |
| `battle::melee` | The real melee formula. `hit_number()` adds up every attack and defence term. `kill_chance()` turns it into a per-mille chance. `resolve_blow()` makes one 1..1000 roll to get Kill, Knockdown, Knockback, Stepback or Miss. `select_pair()` makes the pair-selection rolls. |
| `battle::missile` | Missile `range()`, `accuracy()`, shot `dispersion()`, `chance_to_hit()` and projectile `impact()` (a 0..100 roll gives Kill, Knockdown or Miss). |
| `battle::autoresolve` | Campaign autoresolve kill rates with the confirmed tweak defaults, plus a simplified fight-until-rout loop. |
| `battle::model` | `Battle` and `LandUnit`. `step()` is one 0.1 s tick, with units updated in id order following the original's per-unit order. It uses straight-line movement and runs melee through the real blow pipeline, approximated at unit level. It provides `battle_result()` and a deterministic `state_hash()`. |
| `campaign::ids` | Typed 32-bit ids (`FactionId(i32)`, `RegionId(u32)`, `CharacterId(i32)`, `ForceId(u32)`, `UnitId(i32)`). They keep the raw save value so saves round-trip. |
| `campaign::world` | `CampaignModel { calendar, rng, world, variables }` and `World { factions, regions, characters, forces }`, each a `BTreeMap` keyed by id. It also holds `Faction`, `Region`, `Settlement`, `Character`, `MilitaryForce`, `CampaignUnit`, `Stance`, `GovernmentType`, and a deterministic `state_hash()`. |
| `campaign::variables` | `CampaignVariables`: tunables named after real `campaign_variables` keys where one fits, otherwise `placeholder_*`. Every default value is made up. |
| `campaign::commands` | `CampaignCommand` (EndTurn, MoveForce, SetTaxRate, Recruit, DeclareWar, MakePeace), `CommandQueue`, `CampaignModel::apply()` and `apply_queue()`. A rejected command returns a `CommandError` and changes nothing. |
| `campaign::events` | `CampaignEvent`: variants named exactly like the original script events, so Lua can hook them later. `script_name()` gives the name. |
| `campaign::turn` | `end_turn()` plays one round in a PROVISIONAL, documented order. It also covers placeholder income and the recruitment countdown. |
| `campaign::diplomacy` | `World::stance`, `set_stance`, `declare_war` and `make_peace`. Both sides are always written (protectorate ↔ patron are mirrored). |

## Spec item → function → status

| Spec item | Function / type | Status |
|---|---|---|
| W1 §12.1 LCG, `next16` | `rng::CaRng::next16` | CONFIRMED |
| W1 §12.1 rejection sampler 0x005F0830 | `CaRng::uniform_below` | CONFIRMED (panics on n=0 or n>0xFFFF; the original would fault or hang) |
| W1 §12.1 0x007ADD80 | `CaRng::percent_0_100` | CONFIRMED |
| W1 §12.1 0x005F0880 (u32 math) | `CaRng::int_range` | CONFIRMED |
| W1 §12.1 0x005FC070 / 0x00A9DA10 | `CaRng::unit_float`, `float_range` | CONFIRMED |
| W1 §12.1 0x00535B30 stateless hash | `rng::hash_float16384` | CONFIRMED |
| Seeding of each RNG owner | `CaRng::new(seed)` | UNKNOWN (W1 §10.7); the seed is stored as the state |
| W3 §2.4 i32 with 20 fractional bits | `fixed::Fixed20` | CONFIRMED; `from_f64` rounding is INFERRED |
| W3 §3.1 `DATE`, `turn_in_year` | `calendar::Date`, `Date::turn_in_year` | CONFIRMED |
| W3 §3.1 `CAMPAIGN_CALENDAR`, turn_number = elapsed+1 | `calendar::Calendar` | CONFIRMED |
| Turn advance (early→late→next month) | `Calendar::advance_turn` | INFERRED from the date encoding |
| Season derivation | (not implemented, TODO) | UNKNOWN |
| W1 §12.2 speeds and cycle 0x005D02A0 | `battle::speed::BattleSpeed` | CONFIRMED |
| W1 §12.2 tick = 0.1 s | `battle::TICK_SECONDS` | INFERRED (high) |
| W1 §8 morale component layout | `morale::MoraleComponent` | CONFIRMED offsets, INFERRED meanings |
| Morale state names | `morale::MoraleState` | INFERRED (from `ums_*` keys) |
| Initial morale state and timers | `MoraleComponent::default` | PLACEHOLDER (Steady, timers -1) |
| `add_effect` 0x0054E3C0 semantics | `MoraleComponent::add_effect` | INFERRED (insert or replace by id) |
| W1 §12.3 full evaluation 0x00584020 | `morale::evaluate` | CONFIRMED order, strict compares, timer gates, clamp |
| Effect list cleared each evaluation; suppress flag not reset at start | inside `evaluate` | INFERRED |
| Behaviour mode 4 "special" | inside `evaluate` (kept unchanged) | UNKNOWN |
| Waver timer 0x0053E4D0 | `morale::waver_timeout` | PLACEHOLDER (returns `waver_base_timeout`, which it is confirmed to read) |
| Rout timer 0x0053A720 | `morale::rout_timeout` | PLACEHOLDER (returns `broken_finish_base_timeout`) |
| Sub-evaluators 0x53BC70, 0x53CBD0, 0x53E450, 0x53BB40, 0x53B970, 0x53B7B0, 0x53BC20 | `morale::sub_*` | PLACEHOLDER (no-ops) |
| FUN_0053E980, FUN_00532370, FUN_0055AC20 | `MoraleInputs::shock_persists`, `may_break`, `Attacker::doubles_front_value` | UNKNOWN, supplied by the caller |
| Light update 0x00585BE0 | `morale::light_update` | PLACEHOLDER (counts timers down by 1 per tick) |
| W1 §12.4 category table (+6/+4/+2/−4/−8) | `morale::modifiers` | CONFIRMED |
| W1 §12.4 front/flank/rear and formation adjust | `morale::modifiers` | CONFIRMED values; applying the front logic to front attackers only, and which unit's class is used, are INFERRED |
| W1 §12.4 fighting cavalry signed shift | `morale::fighting_cavalry_value` | CONFIRMED |
| W1 §12.4 total (>), recent/extended/blood (>=) ladders and shock flag | `morale::modifiers` | CONFIRMED |
| W1 §12.5 fatigue state machine | `fatigue::next_state` | CONFIRMED |
| W1 §12.5 gradient ladder and `(m*delta)/100` | `fatigue::gradient_multiplier`, `apply_gradient` | CONFIRMED |
| W1 §12.5 idle + rain/snow, −1 flag, clamp | `fatigue::action_delta`, `tick_delta`, `clamp`, `tick` | CONFIRMED |
| Action enum 0..0x4E → kv key mapping | `fatigue::FatigueAction` | UNKNOWN (named after the kv keys) |
| Climate terms | `FatigueInputs::climate_term` | UNKNOWN (the caller supplies it, 0 for now) |
| Unit fatigue = integer mean 0x006FB080 | `fatigue::unit_fatigue` | CONFIRMED (empty unit → 0 is INFERRED) |
| W1 §12.6 range (walls, ×0.8) | `missile::range` | CONFIRMED (the meaning of "state 7" is UNKNOWN) |
| W1 §12.6 accuracy (+20/+30/+15) | `missile::accuracy` | CONFIRMED (the meaning of the mode and flag is UNKNOWN) |
| W1 §12.6 dispersion, aspect 0.70710677 | `missile::dispersion` | CONFIRMED |
| W1 §12.10 chance to hit 0x00DAB9D0 | `missile::chance_to_hit` | CONFIRMED (squaring `Dh` in f32 is INFERRED) |
| W1 §12.10 impact 0x00DAADF0 (kc 14..94, roll 0..100, d≥7 kill, 1..6 knockdown) | `missile::impact`, `impact_kill_chance`, `roll_100` | CONFIRMED |
| kv_rules key list and types | `rules::KvRules` | CONFIRMED (the `special_ability_*` "other" type is kept as f32, INFERRED) |
| W1 §12.9 hit number 0x00DAB5F0 | `melee::hit_number` | CONFIRMED, every term and in order. A zero divisor gives 0 instead of a crash. |
| W1 §12.9 height delta 0x006CCAB0 | `melee::height_delta` | CONFIRMED |
| W1 §12.9 attack/charge × fatigue-effect multiplier; army level +4/+8 | `melee::scaled_stat`, `army_level_attack_bonus` | CONFIRMED numbers. The multiplier source record and the meaning of the level are open, and the model uses 1.0 and 0. |
| W1 §12.9 kill chance 0x00DADA40 (254/184/125, clamp 1..990, extra attackers) | `melee::base_kill_chance`, `kill_chance` | CONFIRMED (round-half-to-even for `round(kc*0.5)` is INFERRED) |
| W1 §12.9 xholds tiers and outcome 0x00DAA290 | `melee::xholds_tier`, `outcome_for_roll`, `resolve_blow` | CONFIRMED (one roll per blow, which is tested) |
| W1 §12.9 roll 0x00DADB20 | `melee::roll_1000` | CONFIRMED |
| W1 §12.9 pair selection 0x006AFE20 (priority, weights, roll #1, defender rolls, attackers_on_target) | `melee::priority`, `selection_weight`, `pick_weighted`, `pick_index`, `select_pair` | CONFIRMED order. The binary-search tie and fallthrough rule is INFERRED. |
| Encounter `dir` 0x006AD890 | `model::melee_dir` | PLACEHOLDER (angle vs facing: 45° and 135°) |
| W1 §12.7 tweak defaults | `autoresolve::AutoresolveTweaks::default` | CONFIRMED |
| W1 §12.7 kill rates 0x0078D200 | `autoresolve::kill_rates` | CONFIRMED |
| W1 §12.7 engagement loop 0x00759860 | `autoresolve::engage` | Per-step loss and clamp CONFIRMED. Fuzz use, rout test, summing melee and missile, and the result codes are INFERRED/PLACEHOLDER. `shaken` is unused. |
| W1 §8 tick counter / RNG in battle | `model::Battle { tick, rng }` | CONFIRMED |
| W1 §5.6 per-unit order, morale stagger `id%5 == tick%5` | `Battle::step` / `update_unit` | CONFIRMED order; steps 1–5 and 9 are UNKNOWN |
| W1 §5.6 inactive unit → fatigue reset | `update_unit` step 8 | CONFIRMED |
| Movement | `Battle::placeholder_movement` | PLACEHOLDER (straight lines) |
| Melee in the model | `Battle::melee_step`, `melee_exchange`, `resolve_unit_blow` | Each exchange is REAL (§12.9 RNG order, tested). APPROXIMATED: the frontage (40 men), the cadence (one exchange per engaged pair every 20 ticks), local encounter lists of 1–3 members per side from the outnumbering ratio, each soldier using its unit's stats, a flat height delta, Knockdown/back/Stepback having no effect yet, and strike-back with `attackers_on_target = 1` |
| Attack direction, casualty ratios, kill ratio | `Battle::morale_inputs` | PLACEHOLDER |
| Battle end (all units routing or shattered) | `Battle::battle_result` | Project rule |
| Desync hash | `Battle::state_hash` (FNV-1a) | Our own tool |
| W3 §3 `CAMPAIGN_MODEL` → calendar, `RandSeed`, `WORLD` | `campaign::CampaignModel` | CONFIRMED structure (only these children are modelled) |
| 32-bit cross-reference ids (W3 §3.3–§3.6) | `campaign::ids::*` | CONFIRMED types for faction (i32), character (i32), force (u32) and unit (i32). The region id type (u32) is INFERRED |
| W3 §3.3 `FACTION` v18: id, key, treasury, `GOV_IMP` | `campaign::Faction`, `GovernmentType` | CONFIRMED structure. Treasury = first `FACTION_ECONOMICS` scalar is INFERRED |
| W3 §3.4 stances neutral/war/allied/protectorate/patron | `campaign::Stance` | CONFIRMED strings. Which side stores protectorate and which stores patron is INFERRED |
| W3 §3.5 `CHARACTER` type strings, `LOCOMOTABLE` position | `campaign::Character`, `CharacterKind` | CONFIRMED. Treating the two i32s as current/max movement points is INFERRED |
| W3 §3.6 `MILITARY_FORCE`, `UNIT` men/max_men | `campaign::MilitaryForce`, `CampaignUnit` | CONFIRMED structure |
| W3 §3.7 `REGION`, slots with optional `BUILDING`, population | `campaign::Region`, `BuildingRef`, `Settlement` | CONFIRMED structure (population classes, roads and forts are not modelled) |
| `CAMPAIGN_COMMAND_QUEUE` / `CCQ_*` | `campaign::CommandQueue`, `CampaignCommand` | CONFIRMED to exist. Only `CCQ_END_TURN` and `CCQ_SET_GOVERNORSHIP_TAX_RATE` are known names; the other commands are our own |
| Script event names (W3 §6.4) | `campaign::CampaignEvent::script_name` | CONFIRMED names. `StanceChanged` is a project event (no script name) |
| End-of-turn phase order | `CampaignModel::end_turn` | **UNKNOWN** (W1 §10.4). PROVISIONAL order, documented in `turn.rs` and pinned by a test |
| Faction processing in id order | `end_turn` | PROVISIONAL |
| Income | `turn::faction_income` | PLACEHOLDER: `pop × placeholder_income_per_population × tax% + faction_gdp_other` |
| Movement-point refill at turn start | `end_turn` | PLACEHOLDER (refills to the maximum) |
| Movement cost | `commands::movement_cost` | PLACEHOLDER: straight line × `road_level_0_action_point_cost`, with no pathfinding |
| Recruitment | `CampaignCommand::Recruit`, `end_turn` | PLACEHOLDER: pay on issue, done after `placeholder_recruitment_turns`, and the unit goes into the region's garrison force |
| Tax rate per faction | `Faction::tax_rate_pct` | PLACEHOLDER (the original sets it per governorship) |
| War/peace rules | `World::declare_war`, `make_peace` | Symmetry is our invariant; third-party allies are a TODO (UNKNOWN) |
| New force/unit id allocation | `World::next_force_id`, `next_unit_id` | PLACEHOLDER (max + 1) |
| Campaign desync hash | `CampaignModel::state_hash` (FNV-1a) | Our own tool |

## Remaining placeholders (to replace as the reverse engineering progresses)

1. The waver and rout timer formulas, the 7 unknown morale sub-evaluators, the light morale update, and morale mode 4.
2. The unknown predicates FUN_0053E980, FUN_00532370 and FUN_0055AC20, the front-attack "excluded set" and the "+2 condition".
3. The fatigue action-enum mapping and climate terms, and per-soldier simulation (a unit is currently one representative soldier).
4. Movement, attack-direction classification (0x006AD890), and the definitions of the recent, extended and kill ratios.
   Melee's soldier-level details: who fights whom and how often, the non-lethal blow effects, and the fatigue-effect multipliers and army level.
   Missile fire is not yet wired into the model: there is no projectile path simulation.
5. The autoresolve loop details (fuzz, `shaken`, result codes).
6. The calendar season rule and RNG seeding.
7. Campaign: the end-of-turn phase order, the economy formulas (income, tax efficiency, growth), movement and pathfinding, recruitment rules and unit placement, per-governorship tax, id allocation, and diplomacy beyond war/peace (attitudes, third-party allies, treaties).

## The campaign model in plain words

The campaign model is the turn-based map game. Here is how it works:

- **State.** `CampaignModel` holds the calendar, the campaign RNG and the `World` (factions, regions, characters, armies/navies). Everything is stored in `BTreeMap`s keyed by the original's 32-bit ids, so it is always processed in id order.
- **Commands.** Nothing outside the model changes it directly. The display, the AI or a network peer sends a `CampaignCommand` to `CampaignModel::apply`. The command is checked first. If it is invalid, a `CommandError` comes back and the state is untouched. This copies the original's `CAMPAIGN_COMMAND_QUEUE`, and it is what makes replays and lockstep multiplayer possible.
- **Events.** Every change returns `CampaignEvent`s. Their names are the original's script event names (`FactionTurnStart`, `RegionTurnStart`, ...), so the Lua layer can later call `events.<Name>` handlers unchanged.
- **End turn.** `end_turn()` advances the calendar half a month and plays every faction once, in id order. The order of the phases inside a turn is **not known yet**. The order used here is written at the top of `campaign/turn.rs`, and the test `event_order_is_stable` pins it down.
- **Numbers.** All rule numbers (income per head, recruitment time and cost, movement cost) live in `CampaignVariables`. The defaults are made up. The real values must come from the player's own `campaign_variables` table, and the real formulas are still unknown.

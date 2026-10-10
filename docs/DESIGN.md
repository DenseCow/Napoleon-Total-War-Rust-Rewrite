# NapoleonRust: Rust + Bevy Architecture Design (Phase 2)

Status: **v2 (Phase 2 complete for the parts that exist).** Written by the manager from the Worker 1, 2 and 3 reports and from
the code that is now in `crates/`. All sections now describe merged code: the startpos loader (§3.5, f686919), the Lua
layer (§3.6, work/shooting-and-lua) and the rigid_model decoder + viewer (§3.1, §3.7, 33584ed).

Goal: **a complete 1:1 remake in Rust + Bevy using all the assets from the player's game folder** (the whole game, including frontend, UI and multiplayer; see CLAUDE.md "Goal"). That means the same formulas, constants, RNG, tick and turn order, data schemas and file formats as
Napoleon: Total War 1.3.0 (Build 2081, 2023 rebuild), implemented in our own Rust code from documented specs. At runtime the game reads
the player's own installed game files read-only. We ship no Creative Assembly assets.

How to read the tags: **CONFIRMED** = seen directly in the exe or the file bytes; **INFERRED** = a strong reading of the evidence,
not proven; **UNKNOWN / PLACEHOLDER** = not found yet, so our code uses a clearly marked stand-in.

---

## 1. The key design decision: "model" vs "display"

**Evidence from the original (CONFIRMED):**
- Worker 1 found separate source folders, `EmpireBattle\Source\model\` (entity, Rules, morale, locomotion) and display types such as
  `EMPIREBATTLE::ENTITY_DISPLAY` and `NAVAL_OBJECT_DISPLAY`.
- The battle runs as a **deterministic lockstep simulation** with tick-numbered desync logs ("MULTIMODEL DESYNC DETECTED AT TICK %d").
- So the original keeps the *simulation* (the "model") separate from *what you see* (the "display").

**Our design copies that split:**

```
┌──────────────────────────────┐        ┌───────────────────────────────────┐
│  ntw_sim  (pure Rust)        │        │  napoleon  (Bevy app)             │
│  - the authoritative "model" │ reads  │  - window, camera, input, UI      │
│  - deterministic, no Bevy    │◄───────│  - sprites/meshes = "display"     │
│  - 10 Hz battle tick         │ drives │  - turns player clicks into       │
│  - campaign turn processing  │───────►│    commands for the sim           │
│  - unit-tested               │        │                                   │
└──────────────────────────────┘        └───────────────────────────────────┘
```

**Why the authoritative model is NOT made of Bevy ECS components:**
1. **Determinism.** 1:1 behaviour needs an exact processing order. The original updates units in a fixed order and staggers morale by
   `unit_id % 5`. Bevy runs systems in parallel and iterates queries in an unspecified order. Plain `Vec`s sorted by id (and
   `BTreeMap`s, never `HashMap` iteration) give us an exact order.
2. **Testability.** `cargo test` on `ntw_sim` needs no window or GPU, so we can test the morale state machine against the spec line by line.
3. **Fidelity to the original structure.** The original's model/display split maps directly onto sim/Bevy.

**Where Bevy ECS *is* used:** everything visual and interactive. Each sim unit gets a Bevy entity with a view component holding its
unit id, plus `Transform`, sprite or mesh, selection state and so on. A system copies sim positions into `Transform`s every frame.
Interpolation between 10 Hz ticks makes movement look smooth, and that interpolation lives only in the display.

---

## 2. Cargo workspace layout (as it is now)

```
NapoleonRust/
├── Cargo.toml                 workspace root (members below; analysis/ is excluded)
├── crates/
│   ├── ntw_formats/   byte-level readers: .pack + Vfs, DB, .loc, ESF (+ writer), .rigid_model, .dds   std only
│   ├── ntw_sim/       deterministic model: RNG, Fixed20, calendar, battle, campaign           std only
│   ├── ntw_data/      typed DB records + kv tables, GameDatabase                              ntw_formats, ntw_sim
│   ├── ntw_campaign/  startpos.esf / save → ntw_sim::campaign::CampaignModel                  ntw_formats, ntw_sim, ntw_data
│   ├── ntw_script/    Lua 5.1 scripting layer                                                 (skeleton)
│   └── napoleon/      the Bevy game executable                                                bevy 0.19.1, ntw_sim, ntw_data, ntw_formats
├── docs/              DESIGN.md (this file), ARCHITECTURE_REPORT.md
├── analysis/          research tools + worker reports (standalone crates, not part of the game)
└── CLAUDE.md          project rules for Claude
```

**Dependency direction (CONFIRMED by the Cargo.toml files):** `ntw_data` depends on `ntw_sim`, not the other way round. The
simulation defines the plain structs it needs (`KvMorale`, `KvFatigue`, `KvRules`, `LandUnit` stats) and `ntw_data` fills them
from the tables. That keeps `ntw_sim` free of any file I/O, so it can be tested with hand-written numbers.

The crate names follow the original's module names where that's useful:

| Our crate / module | Mirrors original (CONFIRMED source dirs / namespaces) |
|---|---|
| `ntw_formats::pack` | VFS (`0x01051340`, boot/release/patch/mod packs) |
| `ntw_formats::esf` | ESF reader/writer used by startpos, saves, map data |
| `ntw_data` | `EMPIREUTILITY::*_RECORD` (483 record types), `UTILITYLIB::DATABASE_TABLE`, `UTILITYDLL::TWEAKER` |
| `ntw_sim::rng` | the CA LCG (214013 / 2531011), 732 inlined sites |
| `ntw_sim::battle` | `EmpireBattle` (model, Rules, tick, AutoResolver) |
| `ntw_sim::campaign` | `EmpireCampaign` (CAMPAIGN_MODEL, FACTION, REGION, CHARACTER, MILITARY_FORCE, CAMPAIGN_COMMAND_QUEUE) |
| `ntw_campaign` | the startpos/save loader that builds `CAMPAIGN_MODEL` from ESF |
| `ntw_script` | Lua 5.1 with **f32 numbers** (`UTILITYDLL::LUA::State`) |
| `napoleon::battle` | main loop battle mode (`0x00485B90`: front_end / campaign / battle) |
| `napoleon::ui` | `UiComponentLib` (later) |

---

## 3. Crate-by-crate design

### 3.1 `ntw_formats` (pure Rust, std only) — implemented
**Purpose:** read the original file formats exactly, read-only.

| Module | Key types | Spec source / status |
|---|---|---|
| `pack` | `PackFile` (header + index, entries read on demand by seek), `PackEntry`, `PackType`, `Vfs` | W2 §2, verified on all 11 shipped packs: `0x18 + index_bytes + Σsizes == file length` (CONFIRMED) |
| `db` | `DbTable::read(bytes, &Schema)`, `Schema`, `FieldType::{Str, OptStr, Bool, I32, F32, U16}`, `DbValue`, version guards with `IfAbsent` | W1 `DB_BUILDERS.md` §1 (exe loader `0x00E730D0`), W2 `DB_FORMAT_NOTES.md` (CONFIRMED) |
| `loc` | `LocFile`, `Localisation::from_vfs` (all `text\*.loc` merged) | W3 `DB_CAMPAIGN_TABLES.md` §1, exact-EOF parse of all 4 shipped files (CONFIRMED) |
| `esf` | `EsfFile`, `EsfRecord { name, version, children }`, `EsfRecordArray`, `EsfNode::{Bool, I8, I16, ...}`, `EsfWriter` (in memory only) | W3 §2 (byte-exact, CONFIRMED) |
| `rigid_model` | `.rigid_model` + `.rigid_model_header`: u32 mesh_count, meshes (optional 0x12345678 + version, material, 56/72/80-byte vertices, u32 indices), bbox; one file per LOD | GRAPHICS_EXE.md §2–4, checked against exe loaders 0x011D9D80 / 0x012234A0 / 0x011B2AB0 (CONFIRMED); 4,796/4,796 shipped models parse |
| `dds` | DDS header parse + CPU decoder (DXT1/3/5, uncompressed) | GRAPHICS_EXE.md §5; 8,237/8,239 shipped textures decode |

**Pack format** (CONFIRMED):
```
0x00 "PFH0" | 0x04 u32 type (0 boot, 1 release, 2 patch, 3 mod, 4 movie) | 0x08 u32 dep count | 0x0C u32 dep bytes
0x10 u32 file count | 0x14 u32 index bytes | 0x18 dep names | index: {u32 size; NUL-terminated '\' path} x N | payload back to back
```
No compression, no timestamps, no padding, no encryption.

**Load order** (`Vfs::open_install`): boot → release → patch → movie → mod, the order the exe's VFS init distinguishes (CONFIRMED,
W1). Inside one group, packs are sorted by file name (INFERRED: Windows `FindFirstFileW` order; no shipped file depends on it).
A path in a later pack wins. In the shipped game only 18 paths are duplicated, all `local_en.pack` vs `local_en_patch.pack`,
and the patch copy wins (CONFIRMED by sizes). Whether loose files in `data\` override packs is UNKNOWN; the loose files
(`campaigns\*\startpos.esf`, `campaign_maps\`, `*.lua`) are not in any pack, so the question does not arise yet.

**DB table format** (CONFIRMED on all 310 shipped tables):
```
[FC FD FE FF][u32 version]   only when version > 0 (otherwise version = 0)
u8 flag                      1 in every shipped table, meaning UNKNOWN
u32 row_count
rows                         no schema, no column names, no per-row length, no GUID
```
Field encodings: `str` = u16 length + UTF-16LE; `ostr` = u8 present flag + `str`; `bool` = u8; 4-byte `i32` or `f32`.
The file never says what its columns are, so **the column list must come from outside**. Our rule:
1. Column **order and types** come from Worker 1's decompiled exe row readers (`DB_BUILDERS.md`). These are authoritative.
2. **i32 vs f32** for each 4-byte column comes from the values (W2 `schemas_battle.md`, W3 `DB_CAMPAIGN_TABLES.md`), because the
   exe reader copies 4 raw bytes and does not say. All-zero columns stay UNKNOWN.
3. **Names** are INFERRED (W2, W3). The files carry none.

Why the reader never guesses: an empty string `00 00` looks exactly like two `false` bools, and an absent optional string `00` looks
like one bool. W2's byte-level inference got several tables wrong for exactly this reason (units cols 17–22, unit_stats_land cols
13–14, several campaign tables); the exe layouts fixed them. So `DbTable::read` follows the schema strictly, and a wrong schema
shows up as an error (leftover bytes or a short read), never as silently shifted data.

The design rules:
- These readers *never write* to the install. A `Vfs` opened on the Steam folder has no write methods at all.
- ESF records keep their `version` byte and *all* children in order. This is "lossless passthrough", per W3 §9.1, so a save we
  load and write back is byte-identical.

### 3.2 `ntw_data` — implemented
**Purpose:** turn raw tables into strongly-typed Rust structs whose fields mirror the original records.

**How a table is described.** Each table is written once, in `schemas.rs`, with the `db_record!` macro. From that one list the
macro generates the struct (one named field per column, in file order), its `Schema` (types + version guards) and `from_row`.
Because struct and schema come from the same list, they cannot drift apart. Each field's doc comment starts with
`#col @offset conf`: column number, BUILDER struct offset in the exe, and name confidence (H/M/L). Unknown columns are named
`unknown_<offset>`. Version-guarded columns use `=> since N` (with `copy` where the exe copies another column in old versions).

**Implemented tables** (row counts are from the shipped data; all parse to exact EOF — `tests/real_install.rs`, `#[ignore]`d,
run them with `cargo test -p ntw_data -- --ignored` on a machine with the game):

| Rust type | Table (file version) | Exe row reader | Columns | Notes |
|---|---|---|---|---|
| `UnitRecord` | `units` (v4, 442 rows land + naval) | `0x00E85B20` | 25 | key, dev name, category, class, costs, upkeep, models, MP category, AI role. Cols 6/7/9/15/20/22 UNKNOWN |
| `UnitStatsLand` | `unit_stats_land` (v5, 328 rows) | `0x00E84B00` | 89 | men, mounts, guns, personalities, entities, armour, mount, artillery train, gun_type, projectile, accuracy, reload, ammo, melee attack/charge/defence, morale, spacing, training level. ~30 flag columns unnamed |
| `Projectile` | `projectiles` (v1, 144 rows) | `0x00F3EF70` | 35 | range, velocity, damage, reload time, shots, shot type, explosion. W2's inferred layout was identical to the exe's |
| `GunTypeProjectile` | `gun_type_to_projectiles` (v0, 153 rows) | `0x00DD29F0` | 3 | several rows per gun type |
| `FactionRecord` | `factions` (v3, 77 rows) | see schemas.rs | — | colours stored twice in the exe record; last column is an absent `ostr`, not a bool |
| `RegionRecord` | `regions` (v1, 159 rows) | `0x00F3F950` | 6 | |
| `BuildingLevel` | `building_levels` (v0) | `0x00DD2470` | 24 | exe layout; corrects W3's guess at cols 3 and 16–17 (they are strings) |
| `Technology` | `technologies` (v1) | `0x00F086C0` | 13 | |
| `KvTable` / `KvRules` | `_kv_rules` (97 rows) | kv holder, W1 `kv_layout.tsv` | key, f32 | read per key with the exe's int/float choice (`KV_RULES_KEYS`) |
| → `KvMorale` | `_kv_morale` (69 rows) | | key, f32 | all truncated to i32 |
| → `KvFatigue` | `_kv_fatigue` (36 rows) | | key, f32 | all truncated to i32 |

**The kv truncation rule (CONFIRMED, W1 §9):** the exe does not keep the kv floats. For most keys the loader converts with
`cvttss2si` at `0x00F3A899`, which truncates toward zero (2.9 → 2, -2.9 → -2). Only a short list of `_kv_rules` keys stays float.
`kv::exe_truncate` reproduces `cvttss2si` exactly, including its out-of-range result `i32::MIN`.

**Foreign keys** (INFERRED from matching values, W2 §3.3), followed by `GameDatabase` helpers:
```
units.key ──► unit_stats_land.key ──► .projectile ──► projectiles.key                 (muskets, rifles)
                                  └─► .gun_type ──► gun_type_to_projectiles (n rows) ──► projectiles   (artillery)
                                  └─► .man_entity / .mount_entity ──► battle_entities  (speeds, mass, radius; not loaded yet)
                                  └─► .officer / .musician / .standard_bearer ──► battle_personalities
units.campaign_model / .model_2, unit_stats_land.mount / engine / limber model ──► graphics (see analysis/worker2/UNIT_VARIANT_AND_TEXTURES.md)
```

**`GameDatabase`:**
- `from_install(data_dir)` mounts the install's packs with `Vfs::open_install` and decodes every implemented table.
  `from_vfs(&vfs)` does the same on packs the caller mounted (for mods later).
- **Eager, not lazy.** The original loads tables lazily through getters (`0x00E20560`). We load everything at start-up: the
  implemented tables are well under 1 MB and decode in milliseconds, and a broken or modded table is reported at start-up instead
  of mid-battle. This does not change any game result.
- Lookups: `unit`, `unit_stats`, `projectile`, `faction`, `region`, `building_level`, `technology` by key;
  `land_unit(key)` returns a `LandUnitView { unit, stats, projectile }` with the FKs already followed; `gun_projectiles(stats)`.
- Converted kv values ready for the sim: `kv_rules_sim`, `kv_morale`, `kv_fatigue` (and the raw rows, including keys the exe never reads).
- `GameDatabase::test_fixture()` holds **made-up placeholder numbers** (all keys start with `fixture_`), for tests and for running
  the app without an install. It is clearly labelled as not original data.

**Next tables to add** (in priority order; all are decoded in W2's `db_schemas.tsv` and most have exe layouts in `DB_BUILDERS.md`):
`battle_entities`, `fatigue_effects`, `unit_movement_modifiers`, `entity_training_levels`, `unit_experience_thresholds` +
`unit_stats_land_experience_bonuses`, `unit_abilities` + junctions, `gun_types`, then the campaign families (building chains and
effects, technology effects, government types, taxes, trade, traits/ancillaries) and `campaign_difficulty_handicap_effects`,
then the AI tables (`campaign_ai_*`, `cdir_*`).

**Not decodable from the file alone:** `unit_special_ability_types` is truncated in the shipped file (22 rows declared, the last
string runs past EOF; CONFIRMED). Whether the exe tolerates this or never loads it is UNKNOWN. `models_building` / `models_naval`
were not inferred; use the DB_BUILDERS.md layouts when the graphics work needs them.

`Tweakers` (567 `TWEAKER<T>` defaults from W1 `tweakers.tsv`, e.g. autoresolve Kmel = 0.2) are not implemented yet; they are
compiled into the exe, not stored in a table, so they will become a constants module with addresses cited.

### 3.3 `ntw_sim` (the "model": deterministic, no Bevy) — implemented (battle being extended)

**`rng`** (CONFIRMED spec, W1 §12.1):
```rust
pub struct CaRng { state: u32 }
impl CaRng {
    pub fn next16(&mut self) -> u32 { /* state = state*214013 + 2531011; state >> 16 */ }
    pub fn uniform_below(&mut self, n: u32) -> u32   // rejects r <= 0xFFFF % n
    pub fn percent_0_100(&mut self) -> u32           // rejects r < 88, returns r % 101
    pub fn int_range(&mut self, lo: i32, hi: i32) -> i32
    pub fn unit_float(&mut self) -> f32              // next16 * 1.5259022e-05
    pub fn float_range(&mut self, a: f32, b: f32) -> f32
}
```

**`fixed`:** the `Fixed20(i32)` type, for campaign map positions stored as i32 with 20 fractional bits (W3 §2.4, CONFIRMED).

**`calendar`:** `Date { year, season, month, half }`, 24 turns per year, `turn_in_year = month*2 + (half==2)` (W3 §3.1, CONFIRMED).

**`battle`** (`model`, `rules`, `morale`, `fatigue`, `missile`, `melee`, `autoresolve`, `speed`). It has been
extended with missile fire (`shooting`, merged from work/shooting-and-lua); the table shows the design, the code is the source of truth.

| Item | Spec |
|---|---|
| `Battle { tick, rng, units: Vec<LandUnit> }` | tick counter at battle+0x58, RNG at battle+0x50 (W1 §8) |
| `Battle::step()` | one tick = **0.1 s** (INFERRED high, W1 §12.2) |
| per-unit tick order | mirrors `0x0057F070` (W1 §5.6); morale full update only when `unit_id % 5 == tick % 5` (CONFIRMED) |
| `morale` (8 states) | W1 §12.3/§12.4 (CONFIRMED structure). Waver/rout timer formulas UNKNOWN → marked placeholders |
| `fatigue` (6 states) | W1 §12.5 (CONFIRMED) |
| `missile` range / accuracy / dispersion, hit chance and projectile kill | W1 §12.6, §12.10 (CONFIRMED) |
| `shooting`: target pick, reload, volleys, `Battle::order_fire` | uses §12.6/§12.10 per shot (CONFIRMED formulas); hit roll, men per volley, reload-skill effect and tick slot are PLACEHOLDER/PROVISIONAL (added on work/shooting-and-lua) |
| `melee` hit number, kill chance, blow outcome, pair selection | W1 §12.9 (CONFIRMED from the disassembly: `0x006AFE20`, `0x00DAA290`, `0x00DAB5F0`, `0x00DADA40`); some combatant-flag meanings INFERRED, attack-direction computation still open |
| `autoresolve` kill rates + engagement loop | W1 §12.7 (CONFIRMED kill rates; loop roles INFERRED) |
| `speed`: {0, 0.4, 1, 2, 4}, cycling 0→0.4→1→2→4→0 | CONFIRMED |

**`campaign`** (implemented: `world`, `commands`, `turn`, `events`, `diplomacy`, `ids`, `variables`):

| Item | Spec / status |
|---|---|
| `CampaignModel { calendar, rng, world }` | `CAMPAIGN_MODEL` v10 (W3 §3) |
| `World { factions, regions, characters, forces }` | `BTreeMap`s keyed by the original **u32/i32 ESF object ids** (typed newtypes in `ids`), so saves round-trip and iteration is in id order |
| `Faction`, `Region` + `Settlement` + `BuildingRef`, `Character` + `CharacterKind`, `MilitaryForce` + `CampaignUnit`, `RecruitmentItem` | W3 §3.4–§3.7 (CONFIRMED structure; fields not yet complete) |
| `Stance` + `World::set_stance` | one `DIPLOMACY_RELATIONSHIP` per side (W3 §3.4); both sides always written together |
| `CampaignCommand` + `CampaignModel::apply` | the original `CAMPAIGN_COMMAND_QUEUE` / `CCQ_*` idea (CONFIRMED). Only `CCQ_END_TURN` and `CCQ_SET_GOVERNORSHIP_TAX_RATE` are known names; the rest are ours. A rejected command leaves the model unchanged |
| `CampaignEvent` | named exactly like the original script events (`events.lua` declares 168), so the Lua layer can forward them unchanged |
| `turn` (`TurnState`, `step`, `start_campaign`, `end_turn`) | factions play in the startpos `FACTION_ARRAY` order (INFERRED); a human faction stops between its start and end phases; the work is a queue of steps so the Lua host fires each phase's events before the next runs. Phase order **UNKNOWN** (W1 §10.4): documented **PROVISIONAL** order pinned by tests |
| `CampaignRules` (`rules`) | DB data copied by `ntw_campaign::rules_from_db`: `campaign_variables` (+ per-campaign overrides), units (cost, upkeep, `units` #6 as recruitment turns PROVISIONAL), building levels (cost, turns, effects, units allowed, upgrades, slot types), tax levels and effects, government effects, `agents` action points, unit faction permissions |
| `economy`, `commands`, `pathing`, `battles` | income / upkeep / public order (PROVISIONAL formulas over DB values), recruit / construct / tax commands with validation, grid A* movement (PROVISIONAL; `pathfinding.esf` not decoded), attack / merge / enter-settlement intents, autoresolve through `campaign::autoresolve` (stat-based resolver; its engagement loop is shared with `battle::autoresolve::engage`) with a real-time battle hook (`apply_battle_result`). See `analysis/campaign/CAMPAIGN_PLAY.md` |

**`campaign_ai`, `battle::ai`:** simple placeholders ("advance on nearest enemy"). The original uses a BDI architecture
(CONFIRMED structure) with 30 behaviour modules and per-personality weights in `campaign_ai_*` (W2 §9). Long-term goal.

### 3.3.1 Replaceable systems: rule seams (user, 2026-10-10; BACKLOG §11)
Goal: a mod changes how a whole system works (how population grows, a new resource, new battle rules)
without forking the engine, and every system we finish is built that way from the start, so nothing
needs a refit later. Vanilla runs the original's 1:1 rule. Code: `ntw_sim::seam`,
`ntw_sim::campaign::{seams, mod_state}`; worked example: `population.grow`.

- **A seam is one rule function**, named `<system>.<rule>` (`population.grow`, later `public_order.factors`,
  `economy.upkeep`, `recruitment.cost`, `diplomacy.deal_value`, `battle.morale_step`, ...). Its type is a
  `dyn Fn(&CampaignModel, ..., &mut ModWrites) -> Out + Send + Sync`; a `Seam` holds the rule in use
  (`Arc<dyn Fn>`) and its chain of implementation keys (`original`, then each mod's). One function per
  seam, not one trait per system: an extension then has nothing to forward and cannot drop a method by
  mistake. A system is the set of seams under its prefix. Granularity is where the original computes the
  rule (per region, per faction, per unit per tick), never per soldier per frame: hot paths batch.
- **Where it lives:** the model's game data, `CampaignRules::seams` (`CampaignSeams`, one field per seam;
  battle rules get a `BattleSeams` the same way). Not Bevy systems or schedules: the model is plain Rust,
  headless and deterministic (§1), and the display only reads it. Not saved: rebuilt on load from the same
  data, like the rest of `CampaignRules`.
- **One source of truth:** the model's turn, the UI's projections and the AI call a rule only through its
  seam (`model.rules.seams.<seam>.rule()(...)`); the original's function (`population::grow`) is reached
  directly only by the seam's default and by fidelity tests against real saves.
- **Replace or extend:** a mod's implementation is a `Maker`: given the rule before it, it returns the new
  rule. Replacing ignores the argument; extending calls it and changes its result. Several mods chain in
  load order. Implementations are registered by key in a `SeamRegistry` per seam (`CampaignRuleRegistry`):
  the engine's own, a fork's (in Rust at start-up), later one per Lua rule script. A key is never
  re-registered, and `original` is reserved.
- **Selected by data:** a list of `(seam key, implementation key)` rows in load order, resolved once by
  `CampaignRuleRegistry::seams` when the rules are built; an unknown seam or implementation is warned
  once and the rule before it kept. Source of the rows (to wire, BACKLOG): a `_rule_seams` table in the
  merged database (packs and loose `data\` in the mods' order, like `_kv_rules`) and the open campaign
  format's `campaign.toml` `[rules]`. An implementation's numbers come from the merged database
  (`campaign_variables`, `_kv_rules`, its own tables), so no separate parameter store.
- **Lua hook point (design only; the API is BACKLOG §11 "Extended Lua API"):** `ntw_script` registers a
  `Maker` per rule script a mod declares (key `script:<mod>/<file>`). The rule runs in its own sandboxed
  Lua 5.1 state owned by the implementation (behind a `Mutex`, since a rule is `Fn + Send + Sync`), not in
  the campaign script host, which owns the model (no cycle). It gets a read-only view of the model and
  `prev` as a callable, has no `os`, `io` or clock, and changes state only through `ModWrites`. A script
  that errors is logged once and the call falls back to `prev`.
- **New mechanics** (not a changed rule) use the same pieces: their state in `ModState`, their work in a
  turn or battle phase seam whose original is a no-op (e.g. `turn.region_round_end`), extended by the mod.
- **Mod-owned state:** `CampaignModel::mod_state` (`ModState`): values (`Bool`, `Int`, `Float`, `Text`,
  `List`) by `ModKey { owner, scope, name }`, scope = campaign, faction, region, character or force, in a
  `BTreeMap`. Saved in our own save (missing in an older save: empty), never capped; not written to the
  original's `.save` format (tools only). A rule never writes the model's mod state itself: it returns its
  changes in `ModWrites`, which the caller applies in order when the rule runs for real (the round end:
  after each region, before the next) and drops when it runs for a projection (the region panel), so a
  preview never changes state. Values of a mod no longer loaded are kept untouched, so re-enabling it
  resumes; values of a gone entity stay until their owner removes them.
- **Deterministic for multiplayer:** a rule is a pure function of its inputs; no hash-map iteration, no
  clock, no threads. Randomness: a seam whose original draws gets the campaign `CaRng` in its arguments;
  a mod that needs draws where the original has none keeps its own `CaRng` state in `ModState` (an `Int`),
  so the original's random sequence is not shifted. `state_hash` covers `ModState` in key order (only when
  it has values, so a vanilla campaign hashes as before); peers compare `CampaignSeams::chains()` with the
  mod list when a game starts.
- **Cost in vanilla:** one indirect call per rule use and an empty, unallocated `ModWrites`. Measured
  (release, `population.grow`, 20M calls): 12.8 ns direct, 15.1 ns through the seam, about 2 ns per call,
  under 1 µs per round end over a campaign's regions.
- **Adding a seam (every new or refitted system):** a `<SYSTEM>_<RULE>` key constant and the rule's `dyn Fn`
  type in `campaign::seams`; a `CampaignSeams` field defaulting to the original's function; a
  `CampaignRuleRegistry` field and its match arm; an entry in `chains()`; every caller through the seam; a
  test that the default gives exactly the original's results.

### 3.4 `napoleon` (Bevy 0.19.1 app, the "display") — vertical slice implemented

**What exists now:**
| Module | What it does |
|---|---|
| `config` | install path: `$NAPOLEON_INSTALL_DIR` or the default Steam folder; the game reads `<install>\data` |
| `data` (`DataPlugin`) | loads `GameDatabase::from_install` once at start-up into a `GameData` resource; falls back to the test fixture and says so in the HUD |
| `GameMode` state | `FrontEnd` (default) / `CampaignLoad` / `Campaign` / `Battle`, mirroring the mode handlers (`0x00485B90`); `--battle` starts the battle slice directly (test harness) |
| `frontend` (`FrontEndPlugin`) | the original main menu: `ntw_formats::ui_layout` layouts + `tga`/`dds` art + `.cuf` fonts + loc, run by the original UI Lua through `ntw_script::ui::UiScriptHost` (root.lua → `TransitionTo("main")`, button groups, transition maps, Quit). Notes: `analysis/frontend/` (UI_LAYOUT_FORMAT, FONT_FORMAT, FRONTEND_FLOW, UI_SCRIPTING). Harnesses: `--screenshot <png>`, `--ui-click id,...` |
| `battle` (`BattlePlugin`) | `setup` builds a `BattleSim` resource (the `ntw_sim::battle::Battle` + display info) for France vs Austria from real unit stats; `tick_battle` runs in **FixedUpdate at 10 Hz**; battle speed scales `Time<Virtual>` by {0, 0.4, 1, 2, 4}; `view` draws 2D rectangles; `input` selects and orders; `hud` shows text |

**Planned** (mirroring the original's mode handlers, `0x00485B90`):
```rust
#[derive(States)] enum GameMode { FrontEnd, CampaignLoad, Campaign, Battle }
```
| Plugin | What it will do |
|---|---|
| `FrontEndPlugin` (main menu exists) | the remaining front-end pages need their `FrontEnd.*` engine functions; background movie needs a Bink decoder |
| `CampaignPlugin` (exists) | `CampaignSim` (the Lua `ScriptHost` owning the model, built by `ntw_campaign` from a startpos or save); map, settlements and markers that follow the model; selection, right-click orders with a path preview; the original HUD layout (`uimpaign uiayout`) through `UiScriptHost` with the funds/date and End Turn button (`CCQ_END_TURN`); F5 save |
| `model_viewer` (exists) | `cargo run -p napoleon -- --view-model <name>` / `--list-models <text>`: shows original `.rigid_model` files with their diffuse textures (orbit camera, optional `--screenshot`) |
| 3D battle / campaign map | planned: terrain from BATTLE_TERRAIN.md / CAMPAIGN_MAP_GRAPHICS.md, soldiers from UNIT_VARIANT_AND_TEXTURES.md |

**How systems communicate:** player input → Bevy `Message`s (e.g. `OrderMove { unit_id, target }`) → a system pushes the order into the
sim's command queue. This is the original's `CAMPAIGN_COMMAND_QUEUE` / `CCQ_*` idea (CONFIRMED) and also what lockstep multiplayer needs.
Sim results (unit routed, battle ended) → sim event list → Bevy `Message`s → UI and sound react.

**Config:** today an environment variable. A `napoleon.toml` next to the exe (install path + video settings) is planned.

### 3.5 `ntw_campaign` (startpos / save loader)
Implemented (merged f686919). `ntw_campaign::read_file(path, &GameDatabase)` reads a start position
(`datampaigns<campaign>startpos.esf`, root `CAMPAIGN_STARTPOS`) or a save (`*.save`, root `CAMPAIGN_SAVE_GAME`, newer record
versions, no `CAMPAIGN_PREOPEN_MAP_INFO`) and returns the `CampaignModel` plus a list of warnings. It fills the RNG state
(`CAMPAIGN_MODEL/RandSeed`), calendar, factions + rebels, treasury, diplomacy, characters, armies/navies, units, regions,
settlements and buildings. Field positions: `analysis/worker3/STARTPOS_LAYOUT.md`; per-field evidence tags are in the crate docs.

Design rules: the loader reads `campaigns\<name>\startpos.esf` (loose file, not in a pack) or a save with
`ntw_formats::esf`, and produces an `ntw_sim::campaign::CampaignModel`. It keeps the original ids. Anything it does not
understand yet must be kept (lossless passthrough, W3 §9.1) so a later writer can produce a byte-identical save.

### 3.5.1 Custom campaign maps (open format; user, 2026-10-09)
Goal: making a new campaign on a new world (e.g. a Japan map) is easy, while original mods keep working.

- **One campaign model, two sources.** `ntw_sim::campaign::CampaignModel` and the campaign map display run only on
  our own structures. Sources: the original's importer (`startpos.esf` + pack tables, mods applied in the
  original's order) and our open format. Neither the model nor the map display may read `.esf` records, ESF ids or
  the original's pre-baked map files directly; the importer translates them.
- **The open format** is a folder, e.g. `campaigns/<name>/`:
  - `campaign.toml`: name, calendar, playable factions, victory conditions, map size.
  - `regions.png`: one colour per region (a colour → region key table in the TOML).
  - `heightmap.png`: the terrain; optional splat or texture layers for ground types, plus rivers and coasts.
  - Text files for settlements, ports and resources (positions), factions and their starting
    regions, characters, armies and navies, diplomacy and events.
  - Units, buildings and techs come from the normal database tables (packs or loose `data\`), so a custom
    campaign reuses or adds them like any mod.
- **Generated, not hand-made:** borders, region adjacency, region meshes, the pathfinding grid, sea lanes and
  coastlines are built from the images once at load (or by a tool), then cached with a clear invalidation
  rule (the hash of the source files). Today the original pre-bakes these with Creative Assembly's internal
  tools, which is why custom campaign maps were nearly impossible for it.
- **Exporter:** the original's campaign (with mods) can be written out in the open format, so modders start
  from the vanilla map.
- **Scripts:** custom campaigns use the same Lua campaign API as the original's scripts.
- **Assets:** we ship no Creative Assembly files. A Japan map is made by modders, or later converted from a
  game the user owns (another Total War game's formats need their own investigation).

### 3.6 `ntw_script` (Lua 5.1 scripting layer)

**Purpose:** run the original campaign scripts *unchanged*, read at runtime from the player's install
(`data/all_scripted.lua`, `data/campaigns/<c>/scripting.lua`, and the `data.pack` root files such as
`episodicscripting.lua`, `events.lua`, `export_*.lua`), against our `CampaignModel`.

**What the original does (W3 §6, CONFIRMED):** Lua **5.1**, with `lua_Number` = 4-byte **float**
(luac header `1B 4C 75 61 51 00 01 04 04 04 04 00`). `events.lua` declares one table per event; the
engine calls every function in `events.<Name>` with a `context` object. Scripts reach the game
through `GAME(context)` (the "game_interface", 63 methods), `conditions.*` (193 predicates),
`effect.*` (12 actions), `out.*` (logging) and UI tables (`UIComponent`, `CampaignUI`, ...).
`CoreUtils` and `message_handler` ship only as UI bytecode (`ui\coreutils.luac`,
`ui\templates\message_handler.luac`).

**Engine choice: `mlua` 0.12 with the `lua51` + `vendored` features** (the real PUC-Rio Lua 5.1.5
C source, compiled by `cc` with MSVC). Options considered:

| Option | Verdict |
|---|---|
| mlua + vendored Lua 5.1, numbers are `double`; round to `f32` at the binding boundary | **chosen**: the real 5.1 parser, `module()`, `package.loaders`, `setfenv` and error messages, with no extra tooling |
| Fork `lua-src` with `LUA_NUMBER float` | exact number semantics, but 5.1's `luaconf.h` hard-codes `double`, so we would carry a patched C tree; possible later upgrade |
| A pure-Rust Lua VM | no mature Lua **5.1** VM exists; writing one is a project of its own |

**Float fidelity (the gaps, tagged):**
- Numbers crossing Rust ⇄ Lua are rounded to `f32` (`as f32`, round-to-nearest, like a C `double→float`
  cast). INFERRED: this matches what the engine sees, since its C API reads `float`s.
- Arithmetic *inside* scripts runs in `double`. GAP (UNKNOWN impact, believed small): shipped scripts
  mostly count turns/victories and compare small integers, which are exact in both.
- Numeric *source* literals (e.g. `0.95`) stay `double` until they cross the boundary. Bytecode
  constants are widened exactly from `f32`, so `.luac` code sees the original's values.
- `tostring` of non-integers prints with `%.14g` of a double, the original prints a float (e.g.
  `0.1` vs `0.10000000149012`). GAP, only visible in log text.

**Loading bytecode:** `luac::convert_chunk` rewrites a 5.1 chunk from the original's layout
(`size_t` 4, `lua_Number` f32) to the host's (`size_t` 8, `double`), widening every number constant
exactly. Everything else (instructions, line info) is copied byte for byte.

**`require` resolution (INFERRED from the shipped `package.path` and requires):** a custom loader in
`package.loaders` tries each `package.path` template, strips a leading `data/`, and looks the path up
in the pack VFS (case-insensitive), then as `<path>c` (bytecode), then as a loose file under `data/`.
So `require "data.events"` finds `events.lua` in `data.pack`, `require "CoreUtils"` finds
`ui\coreutils.luac`, and `require "export_advice"` finds `export_advice.lua`.

**Binding to the model:** `ScriptHost` owns the `CampaignModel` (behind `Rc<RefCell<..>>`, never
borrowed while Lua runs). game_interface methods that change the game go through
`CampaignModel::apply` (`force_make_peace` → `MakePeace`, ...) or adjust the model directly where no
command exists yet (`treasury_mod`). Script-only state (time triggers, restricted units/buildings,
diplomacy locks, missions, `save_value`/`load_value` positional values) lives in `ScriptState`.
Every method without real behaviour is a **logging stub tagged UNKNOWN**, so scripts load and run.
Unknown `conditions.*` return `false` (or `0` for the ones the scripts compare as numbers).

**Resume notes (2026-10-03):**
- Done: Lua 5.1 host (`ScriptHost`), VFS `require`, `.luac` f32→f64 converter (real `ui\coreutils.luac`
  and `message_handler.luac` load), `events`/`context`, all 63 game_interface names (real:
  treasury_mod, force_make_peace/declare_war, force_diplomacy, time triggers, save/load_value,
  restricted units/buildings, trigger_custom_mission record; rest are UNKNOWN stubs), a few
  conditions, `effect`/UI/out stubs, `bit`. Tests: `tests/host.rs` (in-memory scripts) and
  `tests/real_install.rs` (eur_napoleon from the install, skips if absent).
- Next: more `conditions.*` from the model; apply `other_income_mod` once the income formula is
  known; mission tracking and rewards; fire script events from the Bevy campaign screen; save
  `ScriptState` with the campaign save; other campaigns' scripts (spa, ita, egy, tut) as tests.

---

### 3.7 Rendering and original 3D models
Implemented so far (merged 33584ed): `ntw_formats::rigid_model` and `ntw_formats::dds` (§3.1) and the `napoleon` model viewer
(§3.4). Exe-side ground truth: `analysis/worker1/GRAPHICS_EXE.md`. Coordinates: D3D is left-handed; the viewer negates Z (positions,
normals, tangents, box) and swaps two indices per triangle to show models in Bevy's right-handed Y-up space. Textures are decoded
on the CPU into Bevy images. Not done yet: normal/gloss maps and the original shaders, LOD selection by the `warscape_rigid_lod`
tables, animated / naval models, soldiers (unit variants), terrain.

File-format research that feeds this section, done outside that worker:
- `analysis/worker2/UNIT_VARIANT_AND_TEXTURES.md`: `.unit_variant` (VRNT), `variant_part_mesh` (VMPF), DDS survey, unit → model chain.
- `analysis/worker5/BATTLE_TERRAIN.md`: battleterrain.pack structure.
- `analysis/worker3/CAMPAIGN_MAP_GRAPHICS.md`: campaign map display files, rigid_spline, heightmap and region data.

## 4. Vertical slice (Phase 5) — done

`cargo run -p napoleon` plays France vs Austria with real unit stats and kv tables from the install.

| Step | Implemented in | Fidelity |
|---|---|---|
| World: a flat battlefield | `ntw_sim::battle` | simplified |
| 2 factions, 1 army each, 3 to 4 units | `napoleon::battle::setup` | **real `units` + `unit_stats_land` rows** (fixture without an install) |
| Movement | straight-line walk per tick | simplified (original locomotion UNKNOWN) |
| Combat | ranged uses the **real range/accuracy rules**; melee uses the **real hit-number / kill-chance formula** (W1 §12.9) | partial 1:1 (pair selection and locomotion simplified) |
| Morale | **real 8-state machine + %5 stagger**, real `_kv_morale` | 1:1 structure |
| Fatigue | **real 6-state machine**, real `_kv_fatigue` | 1:1 structure |
| RNG | **real CA LCG** | 1:1 |
| Basic AI | "advance on nearest enemy, attack" | placeholder |
| Battle result | a side wins when all its enemies have routed or shattered | simplified |

Rendering: 2D coloured rectangles per unit, top-down. Original 3D models are being worked on (§3.7).

---

## 5. Testing strategy
- `ntw_sim`: unit tests per spec item. Examples: the RNG sequence from a known seed, every morale state transition, fatigue clamps,
  missile range branches, autoresolve kill rates against hand-computed values, and command validation leaving the campaign
  model unchanged on error.
- Determinism test: run the same battle twice from the same seed and require identical state hashes at every tick.
- `ntw_formats`, `ntw_data`: tests with tiny hand-made byte buffers (no CA data in the repo). `#[ignore]` integration tests
  (`tests/real_install.rs`) read the real install when it is present: `cargo test -- --ignored`.
- Every commit: `cargo build`, `cargo test`, `cargo clippy` for the whole workspace. Build with `CARGO_TARGET_DIR` set to a
  short path because long paths break the MSVC linker.

## 6. Open items / UNKNOWN (carried from the worker reports)
- Resolved since v1: real-time melee resolution (W1 §12.9) and missile hit chance / projectile kill (W1 §12.10).
- Still open: melee attack-direction computation (`0x006AD890`), locomotion, campaign turn phase order, economy/tax formulas,
  battle RNG seeding, morale timer formulas, UI binary layout field order. Each is a follow-up task for exe research.
- DB: semantics of ~30 flag columns in `unit_stats_land` and units cols 6/7/9/15/20/22; i32 vs f32 of all-zero columns;
  the DB header flag byte; pack order inside one pack type; loose-file override.

# Modding and generic-engine audit (BACKLOG §11)

Read-only audit, 2026-10-09, of `main` at `6d475507`. No code changed. Scope: non-test code
(`tests/`, `tests.rs`, `examples/`, `#[cfg(test)]` modules and probe harnesses skipped).
Line numbers are at that commit.

Each entry: **where** — what is fixed — why it blocks a mod or another game — **fix** (default stays
the original's value). Verdict: **default** = a legitimate 1:1 default the data can already
override, **block** = a mod or a new game cannot change it without editing code.

## 1. Napoleon-specific hardcoding

### 1.1 Campaign keys driving rules (the original's exe switches, kept 1:1)

These are CONFIRMED exe behaviour: the original itself switches on the campaign or faction key. They
are faithful, but a mod campaign cannot opt into (or out of) the feature. One fix covers them all:
a per-campaign feature table (`campaign.toml` / importer-filled `CampaignRules::features`), filled by
the importer from the campaign key exactly as the exe does, and read by the rules instead of the key.

| Where | Fixed | Verdict | Fix |
|---|---|---|---|
| `ntw_sim/src/campaign/capture.rs:152`, `:237-239` | `spa_napoleon` loot/occupy options; `align_pro_french`/`align_anti_french` swap on looting | block | feature `looting_shifts_alignment` + the two religion keys in the feature row |
| `ntw_sim/src/campaign/economy.rs:184-188` | `spa_napoleon`: `_Guerrilla`/`_Auxiliary` unit-key suffixes add cost mods | block | feature row: list of (unit-key suffix, effect key) |
| `ntw_sim/src/campaign/economy.rs:732` | `spa_napoleon` region growth/religion branch | block | feature flag |
| `ntw_sim/src/campaign/religion.rs:57` | `spa_napoleon`: faction-wide `zeal_europe` adds to missionary rank | block | feature flag |
| `ntw_sim/src/campaign/trade.rs:222` | `spa_napoleon` + faction `spa_france` earn home trade value | block | feature row naming the faction |
| `ntw_sim/src/campaign/trade.rs:315`, `:547` | `spa_napoleon`: `trade_node_supply_mod`, no commodity price update | block | feature flags |
| `ntw_sim/src/campaign/commands.rs:1059` | `mp_eur_napoleon` AI `france` +1 recruitment point | block | feature row (faction, bonus) |
| `ntw_campaign/src/victory.rs:35-39`, `:140` | victory type 5 looks up `spa_france` | default (type 5 is data, the faction is not) | faction key in the feature row |
| `ntw_ai/src/campaign/mod.rs:209-216` | AI manager key `nap_<prefix>_<faction>`, fallback `nap_eur_full` | block for a new campaign key (falls to the Europe manager) | manager/personality key per campaign in the campaign data, default the current rule |

### 1.2 Campaign list, default factions and theatres (frontend)

| Where | Fixed | Verdict | Fix |
|---|---|---|---|
| `ntw_script/src/ui/frontend.rs:24` `NAPOLEONS_CAMPAIGNS` and `:550-556` (`EnumerateNapoleonsCampaigns`) | the 4 SP / 3 MP campaign keys + DLC check | block: a custom or mod campaign can never be listed (§11 "custom campaigns in the menu") | build the list from the discovered campaigns (pack `campaigns/*/startpos.esf` + `campaigns/<name>/campaign.toml`), vanilla order first |
| `ntw_script/src/ui/frontend.rs:28-35` `default_faction` | campaign → default faction | block (`None` for anything else) | a `default_faction` field per campaign; importer fills the CONFIRMED values |
| `ntw_script/src/ui/frontend.rs:41-46` `campaign_unlocked` | `nap_unlock` thresholds per key | default (unknown keys unlock) | unlock rule per campaign, optional |
| `ntw_script/src/ui/frontend.rs:812-818` and `ntw_script/src/ui/campaign/map.rs:14-21` `theatre_of` | map key → theatre (`nap_europe` → `europe_main`) **twice**, by two different rules (map key vs campaign-key prefix, the second PROVISIONAL) | block, and a duplicated rule; any unknown campaign shows as `europe_main` | one lookup from `campaign_map_playable_areas` (data) keyed by the campaign's map; delete `theatre_of` |
| `ntw_campaign/src/header_map.rs:42` | strips `_main` from the theatre key | default | keep, but behind the importer (see §3) |
| `napoleon/src/campaign/mod.rs:46` | default `--campaign eur_napoleon` | default | fine (CLI default) |
| `napoleon/src/battle/setup.rs:491` | test-battle sides `france` / `austria` | default (dev harness) | read from the battle setup / CLI |

### 1.3 Era and unit-class assumptions (fixed enums of the original's engine)

The original compiled these lists into the exe, so they are CONFIRMED and 1:1. They are the main
block for "any Total War game": a mod with new categories, classes, shot types or abilities silently
maps them to a fallback.

| Where | Fixed | Verdict | Fix |
|---|---|---|---|
| `ntw_sim/src/battle/strength.rs:22-55` `Category` | 7 unit categories; **any unknown category is treated as artillery** (`_ => Artillery`, faithful to `0x00EED2B0`) | block: a new category (e.g. `ashigaru`) becomes artillery | category table in data (key → behaviour flags: mounted, gun crew, naval), vanilla rows = the 7 |
| `ntw_campaign/src/rules.rs:358-366`, `ntw_sim/src/campaign/characters.rs:676-690` (`category_order`), `ntw_ai/src/battle/classes.rs:15-28` | the same category → number map written **three more times** (with different fallbacks: 6, 1, NONE) | block + duplicated rule | one `category_number()` in `ntw_sim` from the data table |
| `ntw_ai/src/battle/classes.rs:32` `CLASS_NAMES: [&str; 46]`, `ntw_campaign/src/regiments.rs:26` `UNIT_CLASSES: [&str; 45]`, `ntw_formats/src/group_formation.rs` `UNIT_CLASSES` | the unit-class enum, **three copies** (46 vs 45 entries: `naval_transport` only in the AI one) | block: a new `unit_class` row has no index (AI: `NONE`; regiment names: dropped) | read the `unit_class` table (the regiments comment says it is the table's file order), one list in `ntw_data` |
| `ntw_sim/src/battle/strength.rs:184-186` | `artillery_horse` +250, `artillery_foot` +150 class bonus | default (exe constants) | per-class value column in the category/class data table |
| `ntw_sim/src/battle/attributes.rs:71-76` `SHOT_TYPE_NAMES: [&str; 26]` | shot-type enum; unknown names → `None` | block: a new projectile kind (e.g. `fire_arrow`) is unusable | shot types from `projectiles` data with an open id; the 26 stay the default order |
| `ntw_sim/src/battle/attributes.rs:86-93` `ABILITY_NAMES: [(&str,u8); 22]` | special-ability enum | block: no new formations/abilities | ability table in data; behaviour keyed by flags, not the enum value |
| `ntw_sim/src/battle/attributes.rs:144` `DRILL_NAMES: [&str; 6]` | `fire_volley`…`rank_fire` | block for non-gunpowder eras (melee-only or bow drills) | drill table in data |
| `ntw_sim/src/battle/rules.rs:95-99` | misfire chances only for musket/cannon × matchlock/flintlock/percussion | default (battle constants) | key misfire by the projectile's ignition from data; a bow simply has none |
| `ntw_sim/src/battle/melee.rs:196`, `rules.rs:36,58-59` | bayonet bonus / bayonet-ring reload penalty | default (applies only to units with the flag) | none needed |
| `napoleon/src/battle/fx.rs:524-541` | muzzle effect chosen by the gun family's name (`howitzer`, `mortar`, `cannon`, `gun`), else `MusketFire` (PROVISIONAL) | block: a bow fires `MusketFire` smoke | effect key per projectile/weapon in data (the original's `projectile_*` tables) |
| `ntw_sim/src/battle/ground.rs:23` `GROUND_TYPE_NAMES: [&str; 25]`, `:174` `speed_modifiers: [[f32; 4]; 25]` | 25 ground types, index ≥ 25 → no movement modifier | default (exe enum) | ground types from `unit_movement_modifiers`/ground data with an open id |
| `ntw_sim/src/battle/fatigue.rs:133` `Weather` | fixed weather enum | default | weather from `ntw_data/src/weather.rs` data (it already reads rows) |

### 1.4 Campaign content enums and fixed lists

| Where | Fixed | Verdict | Fix |
|---|---|---|---|
| `ntw_sim/src/calendar.rs:14-60`, `advance_turn` `:100-110` | 2 turns per month (Early/Late), 4 `Season` codes; `turns_per_year` is stored (24) but **not used** by `advance_turn` | block: Empire/Shogun 2-style calendars (2 or 4 turns per year) impossible | a calendar table per campaign (`turns_per_year`, the date step per turn, season per step); the ESF value feeds it, 24 stays the default |
| `ntw_sim/src/campaign/world.rs:988` `GovernmentType` (3), `:1021` `from_db_key` → `None` | 3 government types | block: a mod government type is dropped | government types from `government_types` data; behaviour columns (elections, classes) already partly in `rules.government_classes` |
| `ntw_sim/src/campaign/world.rs:1101` `CharacterKind` (General … Missionaries; 85 uses), `from_esf_name` → `None` | fixed agent types | block: no new agent types | agent table in data (abilities: `can_spy`, `can_duel`… already exist as keys) |
| `ntw_sim/src/campaign/agents.rs:215` `Weapon` | duel = pistols or swords | default | from the duel attributes in data |
| `ntw_sim/src/campaign/rules.rs:303-316` `TaxClass` (upper/lower), `details.rs:167` `TAX_LEVELS: [&str; 5]`, `rules.rs:323` default `tax_normal`, `:425` | 2 tax classes, 5 tax levels | default (taxes tables exist) | read the levels and classes from `taxes_levels`/`taxes_keys` (the frontend already names those tables) |
| `ntw_sim/src/campaign/research.rs:61,133` `threads: [i32; 3]` | 3 research threads (military/industry/enlightenment) | block: other games have other trees | research categories from `technology` data; a `Vec` |
| `ntw_sim/src/campaign/details.rs:285-300` `DIPLOMACY_OPTIONS: [&str; 14]` and the attitude-reason list above it | fixed diplomacy options / reasons | default (ESF field order) | keep the ESF order inside the importer (§3); model holds keys |
| `ntw_sim/src/campaign/effects.rs:493` | `management_army`/`navy`… minister attributes by post kind | default | post → attribute column in `ministerial_positions` data |
| `ntw_campaign/src/rules.rs:446-471` `SHIPPED: [(&str,u32);18]` + `hms_elephant`/`rocket_ship`/`steam_frigate` guesses (PROVISIONAL) | ship gun counts by model name | block: a mod ship model gets the digits of its key or 0 | decode the `models_naval` gun list (BACKLOG trace item), drop the table |
| `ntw_ai/src/battle/classes.rs`, `ntw_script/src/ui/army_setup.rs:57` | MP army-setup categories `mp_artillery`…`mp_naval_small_ship` | default (UI tab order) | from the `unit_set` / MP category data |

### 1.5 Not found (good)

No faction, region, religion or culture key is hardcoded in model code outside §1.1 (religions,
cultures, factions and regions come from the start position and the DB). Effect/bonus keys
(`gdp_mod_all`, `tw_growth_*`, `autoresolve_*` …) are the original's effect-bundle and
`campaign_variables` keys looked up in data, so they are defaults a mod overrides.

### 1.6 Smaller fixed key lists (same fix: read the table)

- `ntw_sim/src/campaign/economy.rs:858` `DESERTION_EXEMPT_CLASSES` (4 class keys, CONFIRMED exe codes) — default; a flag column on the class table.
- `ntw_sim/src/campaign/economy.rs:366-370` `SLOT_GDP_EFFECTS` / `SLOT_TW_EFFECTS` — default (effect keys per slot type); slot-type table.
- `ntw_sim/src/campaign/negotiation.rs:411`, `ntw_script/src/ui/campaign/diplomacy.rs:360`, `:465` — the 5 attitude names **three times** (duplicated rule); one list in `ntw_sim`.
- `ntw_script/src/ui/campaign/mod.rs:796` season loc keys by code — follows the `Season` enum (§1.4).
- `ntw_script/src/ui/campaign/agents.rs:14` `AGENT_BUTTON_ABILITIES` (5) — follows `CharacterKind` (§1.4).
- `ntw_sim/src/campaign/agents.rs:20`, `:37` `ABILITIES` (12) / `ATTRIBUTES` (14) — character attribute keys; attribute table.
- `ntw_sim/src/campaign/treaties.rs:46` `ATTITUDE_EVENTS` (30), `details.rs:263` `ATTITUDE_FACTORS` (24) — ESF field orders; keep in the importer (§3).

## 2. Limits

Legend: **orig** = the original's real limit (keep the value as the default, move it to data or a
setting); **ours** = our own shortcut (remove or raise; no 1:1 reason to keep it).

### 2.1 Gameplay caps

| Where | Limit | Kind | Fix |
|---|---|---|---|
| `ntw_sim/src/campaign/rules.rs:327` `MAX_UNITS_PER_FORCE = 20`; used at `commands.rs:880, 899, 972, 1539`, `embark.rs:751`, `ntw_ai/src/campaign/mod.rs:595` | 20 units per army/fleet (INFERRED: not found in a table) | orig | `CampaignRules::max_units_per_force` setting (default 20), every user reads the rules value; trace the exe constant first |
| `ntw_script/src/ui/army_setup.rs:239`, `:442` (`unwrap_or(20)`), `:810` (`n >= 20`) | the same 20 written as literals in the custom-battle army setup | orig + duplicated rule | read the same setting |
| `ntw_script/src/ui/battle_setup.rs:192-194` `max_units` → `(20, [6, 8, 10, 20])` | battle units per side by unit scale (`MaxUnitsFromUnitScaleFactor`) | orig | battle-cap setting/table, default these values |
| `ntw_script/src/ui/battle_setup.rs:186-188` `army_funds` `[5000,10000,14000]` / naval `[5000,14000,24000]` | custom-battle funds by size | orig | data table (default these) |
| `ntw_sim/src/battle/unit_scale.rs:36` `STEPS [0.25,0.5,0.75,1.0]`, `:50` `MAX = 1.0`; `ntw_script/src/ui/battle_setup.rs:183` `UNIT_SCALES` (second copy) | unit-size options; men never above the card | orig + duplicated rule | setting list with values > 1.0 allowed (the BACKLOG "unit-size options" item); one copy |
| `ntw_sim/src/campaign/commands.rs:1626` `MAX_QUEUE = 10` | recruitment/construction queue length (CONFIRMED `0x00B62040`) | orig | rules value (default 10) |
| `ntw_sim/src/campaign/characters.rs:94-96` `MAX_TRAITS = 6`, `MAX_ANCILLARIES = 3` | per character | orig, **already a tweak** (`max_traits`/`max_ancillaries`) | none |
| `ntw_sim/src/campaign/commands.rs:673-678` `road_level` clamped to 0..=3 | 4 road levels (`road_level_0..3_action_point_cost`) | orig | levels from the road building chain; AP cost by `road_level_<n>` for any n |
| `ntw_sim/src/battle/morale.rs:133` `MAX_ACTIVE_EFFECTS = 43` | morale effect list (CONFIRMED exe check) | orig (engine detail, not content) | keep; becomes a `Vec` without the cap only if a mod adds morale effects |
| `ntw_sim/src/battle/model.rs:87` `MELEE_MAX_LOCAL_PER_SIDE = 3` | soldiers per local melee encounter (APPROXIMATION) | ours | trace the exe rule (BATTLE_FIDELITY) |
| `ntw_sim/src/campaign/family.rs:45` `child_ages: [i32; 4]`, `:98` 4 child slots | 4 children per family member (ESF layout) | orig | `Vec`; the save writer keeps the ESF slot count for the original's format |
| `ntw_sim/src/campaign/research.rs:61` `threads: [i32; 3]` | 3 research threads | orig | `Vec` keyed by research category (§1.4) |

### 2.2 Render and engine caps (ours)

| Where | Limit | Kind | Fix |
|---|---|---|---|
| `napoleon/src/battle/view.rs:40` `MAX_FIGURES = 240`, used `:817`, `:920` | **a unit with more than 240 men draws only 240** | ours | raise to the unit's men (instancing already per man) or make it a graphics setting; blocks the "1,000-man unit" test |
| `napoleon/src/battle/fx.rs:28` `MAX_PARTICLES = 24_000` (PROVISIONAL, drops oldest) | global particle cap | ours | graphics setting; trace the original pool size |
| `napoleon/src/terrain/trees.rs:54` `MAX_SPECIES = 32` | species per billboard texture (warns, drops the rest) | ours | storage buffer or several batches |
| `napoleon/src/campaign/scene.rs:186` `MAX_TEXTURE_WIDTH = 4096` | campaign supertexture level drawn | ours | from the GPU limit; an 8K custom map needs it |
| `napoleon/src/audio/mod.rs:905` `MAX_VOICES = 128` | mixer voices | orig-like (Miles) | setting; check the Miles provider limit |
| `ntw_ai/src/campaign/bdi.rs:115` `MAX_STEPS = 6000` | BDI planner step guard | ours (safety) | keep, scale with world size |

### 2.3 Small id types and narrowing casts

The campaign ids are `u32` (`ntw_sim/src/campaign/ids.rs:14-80`): no small id type in the model.
Remaining narrow types:

| Where | Type | Kind | Fix |
|---|---|---|---|
| `ntw_sim/src/campaign/pathing.rs:60` `region: Vec<u16>`; `ntw_campaign/src/pathing.rs:104`, `:152`, `:212` (`r as u16`); `ntw_sim/src/campaign/polypath.rs:79` `region_id: u16`; `rtcut.rs:84`; `ntw_campaign/src/grid_obstacle.rs:101` | region index per path cell / polygon, **silent wrap** past 65,535 | orig (file format of the pre-baked grid) leaking into the model | `u32` in the model grid; narrow only inside the importer/writer with a checked conversion |
| `ntw_ai/src/battle/classes.rs:131` (`i as u8`), `ntw_ai/src/campaign/mod.rs:137` `class: u8`, `ntw_campaign/src/rules.rs:83` `category: u8`, `ntw_sim/src/battle/model.rs:158` `formation_class: u8` | unit class / category numbers | orig enum | `u16`/`u32` once classes come from data (§1.3) |
| `ntw_sim/src/battle/model.rs:227` `army_index: u8`, `side: u8` throughout battle | armies per side, sides | orig (2 sides) | fine for now; `u16` armies if reinforcement armies are unlimited |
| `ntw_campaign/src/names.rs:165` `Vec<u16>` deck, `i as u16` | name deck size, silent wrap past 65,535 names | ours | `u32` |
| `ntw_sim/src/campaign/commands.rs:1012`, `ntw_campaign/src/save.rs:1105`, `:1116-1117` | tax rate as `u8` (clamped 0..255) | orig (ESF field) | keep in the save writer, `i32` in the model |
| `ntw_sim/src/campaign/details.rs:412` `diplomacy_options: [u32; 14]`, `economy.rs:676-679` `happiness: [i32; 13]`, `repression: [i32; 6]` | ESF-shaped fixed arrays in model types | orig (ESF layout) | keyed maps in the model; the ESF order lives in the importer/writer (§3) |

### 2.4 Loops over fixed counts

None found: no loop in model, AI or UI code iterates a literal number of factions, regions,
religions or cultures (searched `0..N`, `.take(N)`, `len() < N`, `.min(N)` with N ≥ 10, and faction
bitmasks). The only literal loops are format internals (`ntw_campaign/src/shroud.rs:47`, 64-bit
quad-tree leaves) and render code. Faction, region, religion and culture counts are already
data-driven; the remaining count limits are the ones in §2.1–2.3.

### 2.5 Not covered

Multiplayer has no code yet (no player cap to audit). The campaign UI's handling of many factions
(flag/colour lists, scrolling) was not exercised; it belongs to the BACKLOG "Test with a synthetic
mod" item.

## 3. `.esf` and original-map coupling (DESIGN.md §3.5.1)

### 3.1 What is already clean

- **The model does not read ESF.** `ntw_sim` uses `ntw_formats` only for cloth, animation, battle
  spec and building models; no `ntw_formats::esf` import. `CampaignModel` holds no raw ESF record,
  no byte blob and no passthrough field. Ids are plain `u32` newtypes (`ntw_sim/src/campaign/ids.rs`).
- **Map-derived model data is in model types**: the path grid (`ntw_sim/src/campaign/pathing.rs`,
  `polypath.rs` `CellInput`/`PolyInput`, `rtcut.rs`), region neighbours and trade nodes are built by
  the importer crate (`ntw_campaign/src/pathing.rs::build_grid`, `ntw_campaign/src/trade.rs:388`
  `attach_map`) into `World::terrain` / `World::region_neighbours`.

So the second source mostly needs a new importer and a generator, not a model rewrite. The coupling
is in the **orchestration, saves, AI, UI and display**, below.

### 3.2 Coupling to move behind the importer

| Where | Depends on | Why it blocks a second source | Fix |
|---|---|---|---|
| `napoleon/src/campaign/scene.rs:253-321` | the app itself sequences: read `campaigns/<c>/startpos.esf` → `ntw_campaign::read` → `CampaignMap::load` (`campaign_maps/<map>/`) → `build_grid` → `attach_map` → `header_map::TheatrePictures` | the app knows both ESF and the map-folder layout; a second source would need a second copy of this sequence | one `CampaignSource` trait in `ntw_campaign` (`load() -> (CampaignModel, CampaignInfo, MapData)`), two impls (original, open format); the app calls only that |
| `napoleon/src/campaign/play.rs:36` `source: Arc<Vec<u8>>`, `:703-707` | **saving = re-parsing the source ESF and patching it** (`ntw_campaign::save::write_save_named`, `script_values::write_restrictions`, `header_map::update_maps`; the "lossless passthrough", `ntw_campaign/src/save.rs:1-35`) | a campaign from the open format has no source tree, so **it cannot be saved at all**; the AI block, pathfinder obstacles, population/GDP, `PENDING_BATTLE` and other unmodelled state survive only as passthrough | our own save format serialising `CampaignModel` + script values directly (save compatibility with the original is out of scope); the ESF writer becomes the optional exporter. Prerequisite: every passthrough-only block the game needs gets a model field |
| `ntw_campaign/src/lib.rs` `read_info` → `CampaignInfo { map_key, campaign_key, header }`; used by `ntw_script/src/ui/frontend.rs:314, 444, 459, 568, 754, 811, 829` and `ntw_script/src/ui/campaign/mod.rs:1266-1276` | front end and campaign UI open `campaigns/<key>/startpos.esf` (or a `.save`) for the year, map key, theatre and turn | an open-format campaign has no startpos: the list, the Load Game page and the theatre lookup all miss it | `CampaignInfo` provided by the `CampaignSource` (and stored in our own save header); the UI asks the source, never the file |
| `napoleon/src/campaign/mod.rs:60-65` (`--campaign list`), `ntw_script/src/ui/frontend.rs:24` | campaigns discovered as `campaigns/*/startpos.esf` / a fixed list | open-format campaigns are invisible | discovery through the sources (§1.2) |
| `napoleon/src/campaign_ai.rs:53` + `ntw_ai/src/campaign/keys.rs:15-135` | the AI re-parses the startpos ESF for each faction's manager/personality keys (`FACTION` record) and `CAI_REGION_BASE_VALUE`s | `ntw_ai` depends on ESF record layout; a second source has no `FACTION` record | the importer puts `ai_manager`, `ai_personality`, `region_base_value` into the model (`Faction`, `Region`); drop the ESF reading from `keys.rs` |
| `ntw_script/src/ui/campaign/mod.rs:1385-1390` `theatre_bounds` | reads `campaign_maps/<map>/regions.esf` itself | the UI parses a pre-baked map file | `MapData::theatre_bounds` from the source |
| `ntw_script/src/ui/campaign/map.rs:165-185`, `ntw_campaign/src/header_map.rs:40-46` | radar `Map`/`Overlay`/`Radar` pictures and the save-header pictures `<x>_map.tga` / `<x>_lookup.tga` in the map folder (paths from `campaign_map_playable_areas`) | a custom campaign must hand-make them; DESIGN.md §3.5.1 wants them generated | the generator renders them from `regions.png` + region colours (cached by source hash); the importer passes the original's files through |
| `ntw_formats/src/campaign_map.rs:865-930` `CampaignMap` (a format type) consumed directly by the display: `napoleon/src/campaign/scene.rs` (31 uses), `region_labels.rs` (13), `detail.rs` (10), `arrows.rs` (6) | `RegionMap` (regions.esf), `Heightmap` (tga), `SuperTexture` (.stpi/.stpd), `SplineFile` borders/roads/rivers/traderoutes, `pathfinding.esf`, `sea_grids.esf`, `rigid_trees`, `coastline_group<n>.rigid_mesh` | the display is written against the original's pre-baked file formats; the generator would have to emit those formats, or the display needs a second path | a format-neutral `MapData` (heightmap samples, ground layers / texture source, border/road/river/coast polylines or meshes, labels, trees) in `ntw_campaign` or a small new crate; `CampaignMap` → `MapData` in the importer, images → `MapData` in the generator; the display reads only `MapData` |
| `napoleon/src/campaign/detail.rs:147` | opens `campaign_maps\<map>\display\supertexture\supertexture.stpd` **from the install's loose folder with `std::fs`, bypassing the Vfs** | a map mod shipping its own supertexture in a pack gets the pack's index (`.stpi`, via `GameFiles`) but the vanilla tile data: **mismatched close-up tiles with a map mod** (not visible on vanilla, where the file ships loose only) | read through `GameFiles` with a seekable reader, later through `MapData`'s texture source |
| `napoleon/src/campaign/arrows.rs:155`, `scene.rs:1045` | arrow model and river texture from the map folder | a custom map without them has no arrows / river texture | fall back to a shared default asset |
| `ntw_sim/src/campaign/polypath.rs:84-110` (`header: [u8; 8]` cost bytes per cell, polygon kinds), `rtcut.rs` | the model's path search runs on the original's polygon-per-cell representation (CONFIRMED 1:1) | the generator must produce polygons + per-direction cost bytes from images, not just a cell grid | keep the model type (it is ours); the generator outputs `CellInput`s (BACKLOG "Generator" item) |
| `ntw_sim/src/campaign/world.rs:999-1172` `esf_name()` / `from_esf_name()` on `GovernmentType`, `Stance`, `CharacterKind`; rules maps keyed by them (`characters.rs:297, 607`, `commands.rs:1552`, `pool.rs:165, 469, 495`, `visibility.rs:159, 170`, `religion.rs:41-45`) | ESF spellings (`"General"`, capital G) are the model's keys; `religion.rs:41` and `visibility.rs:428` test `esf_name().contains("missionary")` | names belong to the importer; a different source spelling breaks the rules lookups | model keys = DB keys (`agents` table); the ESF spelling map moves to `ntw_campaign` |
| `ntw_sim/src/campaign/details.rs:141` `theatre_id`, `:263` `ATTITUDE_FACTORS`, `:293` `DIPLOMACY_OPTIONS`, `:412` `diplomacy_options: [u32; 14]`, `treaties.rs:46` `ATTITUDE_EVENTS`, `economy.rs:676-679` | ESF field orders and indices stored in model types | the open format would have to reproduce ESF index order | keyed maps in the model; the index orders live in the importer/writer |
| `ntw_campaign/src/victory.rs:44` `options(startpos: &EsfRecord, …)` | victory conditions read from the start-position tree | no ESF for the open format | the importer fills the model's victory options once; `campaign.toml` supplies them |
| `ntw_script/src/ui/army_setup.rs:51` | custom-battle UI imports `ntw_campaign::regiments::UNIT_CLASSES` | the UI depends on the importer crate for a data list | the one class list from data (§1.3) |

Tests (`napoleon/src/battle/setup.rs:1102`, `campaign/play.rs:769`, `region_labels.rs:367`,
`arrows.rs:678`) name `eur_napoleon` / `nap_europe`; fine.

### 3.3 What the second source must provide (the interface)

A `CampaignSource` returns: `CampaignModel` (with AI keys, region base values, victory options,
calendar and campaign features filled), `CampaignInfo` (key, name, year, map key, default faction,
unlock rule, theatres), and `MapData` (bounds and theatre bounds, heightmap, ground texture source,
polylines/meshes for borders, roads, rivers, coasts, trade routes, labels, trees, path `CellInput`s,
sea grid, radar/lookup pictures). Saving goes through our own model serialiser, so it works for both
sources.

### 3.4 Status (branch `work/campaign-source`, 2026-10-10)

Done (code in `ntw_campaign::source`, `map_display`, `own_save`; tests in `own_save_install.rs`):
- `scene.rs` sequence → one `source::open(files, Start::New | Start::Save, db)`; `OriginalSource` is
  the one `CampaignSource` so far.
- Saving → our own format (`OWN_SAVE_FORMAT.md`); the passthrough ESF tree is gone from play.
- AI keys and region base values → model fields filled by the importer; the game's AI no longer
  reads the start position. `keys.rs` still holds one ESF reader, `read_difficulties` (each faction's
  difficulty values, used only by the `campaign_ai` install test); it moves behind the importer when
  the model gets the difficulty values.
- Front end and `--campaign list` → `source::campaign_info` / `original_campaigns`.
- `CampaignMap` in the display → `MapDisplay` (regions, heights with their scale, `MapLine`s in
  logic units, a `GroundTexture`, coast, trees, the river texture and arrow model bytes); the
  display reads no map file and knows no map-folder path. The pathfinding and sea-grid files stay
  with the importer (movement grid).
- `detail.rs:147` Vfs bypass → fixed by the port: the ground texture reads its tiles by range from
  wherever `GameFiles::locate` finds `supertexture.stpd` (the winning pack, else the loose file).

Left (separate items): the UI's `theatre_bounds` fallback still parses `regions.esf`
(`ntw_script/src/ui/campaign/mod.rs`; the game sets the bounds itself, the fallback serves the
harness); arrows and rivers have no shared default asset when a map lacks them; the radar and
header pictures, path `CellInput`s and sea grid from a generator; ESF spellings and index orders in
the model (`esf_name`, `ATTITUDE_FACTORS`, `DIPLOMACY_OPTIONS`); victory options from the
start-position tree; `army_setup.rs`'s `UNIT_CLASSES` import.

## 4. Priorities (highest impact first)

1. **Own save format** (§3.2 `play.rs:703`): without it an open-format campaign cannot be saved; it
   also frees the model from the passthrough. Needs the passthrough-only blocks modelled first.
2. **`CampaignSource` + `CampaignInfo` + `MapData`** (§3.2 `scene.rs`, frontend `read_info`, the
   display on `CampaignMap`): the one seam every later §11 custom-campaign item plugs into.
3. **Campaign feature table** replacing the `spa_napoleon` / `mp_eur_napoleon` / `spa_france` switches
   (§1.1, 9 sites), plus the frontend campaign list, default faction and theatre (§1.2; also removes
   the duplicated theatre rule).
4. **Unit category and class from data, one copy** (§1.3: `Category` with unknown → artillery, the
   category map ×4, the class list ×3): the biggest block for non-Napoleon rosters, and a
   one-source-of-truth fix on its own.
5. **Caps to settings**: `MAX_UNITS_PER_FORCE` and its literal copies in `army_setup.rs`, battle unit
   caps, unit-size steps above 1.0, queue length (§2.1); and `MAX_FIGURES = 240` (§2.2), which
   silently under-draws any unit above 240 men.
6. **Calendar from data** (§1.4: 2 turns per month fixed, the stored `turns_per_year` ignored).
7. **Era enums to data**: shot types, abilities, drills, ground types, the muzzle-effect choice
   (§1.3), research threads, government and agent types (§1.4).
8. **AI keys into the model** (§3.2 `ntw_ai/src/campaign/keys.rs`) and ESF spellings / orders out of
   the model.
9. **Map-mod bug**: `detail.rs:147` reads the supertexture tiles past the Vfs.
10. Narrow types: path-grid `region: u16` with silent `as u16`, name deck `u16` (§2.3).

**Estimated fix items:** §1 about 14 (feature table, frontend list, category/class table,
shot/ability/drill/ground tables, fx effect key, calendar, government/agent/research/tax tables,
ship-guns trace, small key lists); §2 about 8 (army-cap setting, battle caps and funds, unit-size
steps, queue length, `MAX_FIGURES`, render caps, narrow ids, family/research `Vec`s); §3 about 9 (own
save format, modelling the passthrough blocks, `CampaignSource`/`CampaignInfo`, `MapData` + display
port, AI keys, UI file reads, generated pictures, ESF names/orders out of the model, victory options;
the `detail.rs` Vfs fix). About 31 in total, several small.

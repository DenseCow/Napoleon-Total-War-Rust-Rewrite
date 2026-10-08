# WORKER 2 REPORT: Napoleon: Total War data files (pack format, DB tables, battle data)

Tags: **CONFIRMED** = observed directly in the bytes. **INFERRED** = strong conclusion. **UNKNOWN** = the data cannot tell us.
Scope changes during the run: ESF, campaign maps, Lua and UI moved to Worker 3 (see `HANDOFF_TO_WORKER3.md`); the campaign DB tables and .loc also moved to Worker 3; Worker 1 is confirming DB field types from the exe loaders.
Cross-references: **campaign tables → `worker3\DB_CAMPAIGN_TABLES.md`**, **exe-confirmed schemas → `worker1\DB_BUILDERS.md`**, ESF/startpos/Lua API → `worker3\WORKER3_REPORT.md`, KV holder layout → `worker1\kv_layout.tsv`.

Tooling provenance:
- Earlier **Python** (stdlib, before the "no Python" rule): `pack_index.py` produced `pack_indexes\*.txt` and `pack_summary.txt`. `db_infer.py` / `dbpeek.py` were early experiments; their results were superseded.
- **Rust** `data_tools` (std only, no crates; standalone crate with an empty `[workspace]`): it produced everything else. That includes `pack_summary_rust.txt` (it re-verifies the pack format independently), `db_list.tsv`, `db_schemas.tsv`, `db_examples.txt`, `battle_tables\*.tsv`, `ai_tables\*.tsv`, and the schemas in `schemas_battle.md` / `schemas.md`.

---
## 1. Data inventory (CONFIRMED)
Install `data\`: 11 `.pack` files (21.9 GB total) plus loose files.

| pack | type | files | size | contents |
|---|---|---|---|---|
| boot.pack | 0 boot | 19 | 10 MB | frontend UI textures, 1 shader |
| data.pack | 1 release | 19,235 | 2.85 GB | **db\ (310 tables)**, ui\ (11,854), animations\ (3,834), 497 .luac and 10 .lua scripts, battle templates, historical battles, groupformations.bin, campaign\, testdata\, logs\cai\ |
| local_en.pack | 1 release | 1,852 | 570 MB | advisor mp3, fonts .cuf, text\localisation.loc, text\ui.loc |
| local_en_patch.pack | 2 patch | 112 | 40 MB | updated advisor mp3, **text\localisation.loc, text\ui.loc** (overrides) |
| media.pack | 4 movie | 74 | 2.5 GB | .bik movies |
| battleterrain.pack | 1 | 3,627 | 2.26 GB | battle terrain presets/tiles/maps |
| buildings.pack | 1 | 11,058 | 3.15 GB | building rigid models |
| rigidmodels.pack | 1 | 4,756 | 1.41 GB | campaign/battle props, naval models |
| variantmodels.pack / variantmodels2.pack | 1 | 5,551 / 76 | 3.1 / 2.2 GB | unit variants and textures |
| sound.pack | 1 | 40,635 | 4.26 GB | sfx, voices per nationality, music |

Full per-pack indexes (size, absolute offset, path): `pack_indexes\<pack>.txt`. Directory and extension histograms: `pack_summary.txt` / `pack_summary_rust.txt`.
Loose files: `campaigns\*\{scripting.lua,startpos.esf}` (8 campaigns), `campaign_maps\nap_*\` (5 maps, 1,417 files counting UI), `all_scripted.lua`, `battle_scripted.lua`, `language.txt` = "EN". **No patch packs exist outside `data\`**, and the only patch pack is `local_en_patch.pack`. No `db\` entries exist outside data.pack, so the DB tables have no patch overrides.

## 2. Pack format (CONFIRMED on all 11 packs)
```
0x00 [4]  "PFH0"
0x04 u32  pack type: 0 boot, 1 release, 2 patch, 4 movie   (3 = mod per exe strings, not present)
0x08 u32  dependency-name count          (0 in all packs)
0x0C u32  dependency-name block bytes    (0 in all packs)
0x10 u32  file count
0x14 u32  index block bytes
0x18      dependency names (NUL-terminated ASCII)
....      index: file_count x { u32 size; char path[] NUL-terminated, '\' separators, lower/mixed case }
....      payload: files back-to-back in index order, uncompressed, no padding, no per-file header
```
Evidence: boot.pack `50 46 48 30 | 00 00 00 00 | 00 00 00 00 | 00 00 00 00 | 13 00 00 00 (19 files) | ef 03 00 00 (index 1007 B) | d4 00 00 00 "commontextures\default_black.dds\0" ...`. For every pack, `0x18 + index_bytes + sum(sizes) == file length` exactly. That proves there are no timestamps, no compression, and no alignment in PFH0. Paths are ASCII. There is no hash or encryption.
Reader: `data_tools\src\pack.rs` (`PackIndex::open`, `read(entry)` via seek). A Rust loader can mount packs by mapping lowercase path → (pack, offset, size).

**Load order (INFERRED):** boot (type 0) is mounted first. Release packs (type 1) follow, then patch packs (type 2), which override release, and movie packs (type 4) are separate. Evidence: exactly 18 paths are duplicated across packs, and all 18 are local_en.pack vs local_en_patch.pack (text\localisation.loc, text\ui.loc, 16 advisor mp3s). The patch versions are larger and newer, so patch must win. Worker 1 CONFIRMS that the VFS init (0x01051340) distinguishes boot/release/patch/bink/mod packs and reports errors such as "boot pack has dependencies". The exact ordering within one type (alphabetical?) is **UNKNOWN / NEEDS WORKER 1**. Loose `data\` files (`non_pack` in exe strings) probably override packs. That is INFERRED; the loose `.lua` and `.esf` files only exist loose.

## 3. DB table format + key tables
### 3.1 Format (CONFIRMED on all 310 tables), full spec in `DB_FORMAT_NOTES.md`
```
[FC FD FE FF][u32 version]   only when version > 0
u8 0x01                      marker (all tables)
u32 row_count
rows (no schema, no names, no per-row length)
```
Field encodings: `str` = u16 length + UTF-16LE; `ostr` = u8 flag + optional str; `bool` = u8; 4-byte `i32` / `f32`. No GUIDs, no ASCII strings and no i16 occur in any solved table. Versions: 288 tables v0, 16 v1, 2 v2, 2 v3 (incl. factions), units v4, unit_stats_land v5. The table list with versions and row counts is in `db_list.tsv` (310 tables; Worker 1's exe list has 679 names including `_table`/`_tables` and record types, so the exe knows more tables than ship).
Schema recovery: the tables carry no schema, so `data_tools db-infer` solves for a type sequence. It guesses the row-1 start, computes exact reachable row ends by DP over cursor tuples, chains row boundaries, then solves all rows in lockstep. A schema is accepted only if every row parses, it ends exactly at EOF, and every 4-byte column is consistently int-like or float-like. A post-pass splits a 4-byte column of 0/1 bytes such as `0x00010100` into 4 bools.
Results: **see `db_schemas.tsv`** (status per table) and `db_examples.txt` (3 sample rows per table). The final run solves 307/310. The three failures are `models_building` and `models_naval` (skipped: huge float rows, visual-only, layout in DB_BUILDERS.md) and `unit_special_ability_types`. That last one is **truncated in the shipped file**: 22 rows are declared and the last string runs past EOF (CONFIRMED; every row is a single `s`).

**Ground truth and corrections.** Worker 1's decompiled row readers (`worker1\DB_BUILDERS.md`) are the authoritative column order. My inferred schemas were checked against them:
- **projectiles (35): identical.** gun_types, gun_type_to_projectiles, battle_entities, experience/threshold tables, fatigue_effects, unit_movement_modifiers, unit_abilities, unit_special_abilities, entity_training_levels, projectile_shot_type_enum and battle_personalities are also identical.
- **unit_stats_land:** one error. My cols 13+14 (`ostr ""` + `bool`) are really one 4-byte field, exe #13, whose value is always 1. So the table has 89 columns, not 90; all my later columns shift down by one.
- **units:** cols 17–22 had the same byte total but the wrong split. The exe has `b b b 4 b 4`, where I had `b i b i b b`; my "257" was 2 bools read as an int.
- **Campaign tables (Worker 3 owns them):** the earlier `db_schemas.tsv` entries for building_levels, ancillaries, character_traits, government_types, cultures, cultures_subcultures, slots, historical_characters and campaign_variables were wrong or ambiguous. They had shifted ints, i16 pairs in place of i32/f32, and empty-string vs bool confusion. **See W3's `DB_CAMPAIGN_TABLES.md` and W1's DB_BUILDERS.md, which supersede them.** My tool now embeds the exe layouts for those tables (`db::known_schema`), so the regenerated `db_schemas.tsv` matches. Note that W1 corrects W3 on building_levels: col 3 and cols 16–17 are strings.
- **factions:** the exe layout `... s s s s o{v>=3}` agrees with the data. The last byte is 00 in every row, which is an absent optional string, not a bool.
- The lesson, as W1 put it: an empty string `00 00` looks like two false bools, and an absent optional string `00` looks like one bool. Parsing to EOF is necessary but **not sufficient**. My inferrer now also rejects ints ≥ 65536 whose low 16 bits are zero (byte-shift signature), rejects ints that look like UTF-16 text, and tries i16 only as a last resort. Even so, byte inference cannot resolve zero runs, so exe layouts win.
- What the exe cannot say is i32 vs f32. I classified every 4-byte column of unit_stats_land, units and projectiles from the data (`schemas_battle.md`). All-zero columns stay UNKNOWN.

### 3.2 Key battle tables: column-level detail in `schemas_battle.md` (also folded into `schemas.md`)
| table | ver | rows | cols | status |
|---|---|---|---|---|
| unit_stats_land | 5 | 328 | 89 (exe) | decoded; ~45 columns named with high or medium confidence (men, mounts, guns, armour, accuracy, reload, ammo, melee attack, charge, defence, morale, spacing, entity/projectile FKs); 30 flag columns unnamed |
| units | 4 | 442 | 25 (exe) | decoded; costs and upkeep INFERRED |
| projectiles | 1 | 144 | 35 | decoded; range, velocity, reload time and shot counts INFERRED |
| gun_types / gun_type_to_projectiles | 0 | 62 / 153 | 8 / 3 | fully decoded |
| battle_entities | 0 | 20 | 21 | decoded (speeds, mass, radius) |
| _kv_rules / _kv_morale / _kv_fatigue / _kv_naval_morale | 0 | 97/69/36/62 | key,f32 | **fully decoded with names** (the keys are the names) |
| fatigue_effects, entity_training_levels, unit_experience_thresholds, unit_stats_land_experience_bonuses, unit_movement_modifiers, unit_abilities, unit_special_abilities | 0 | small | | fully decoded (`battle_tables\`) |
| unit_stats_naval | 2 | 114 | | decoded structurally (naval, low priority) |
**There are no separate melee_weapons / missile_weapons / armour / shields tables in NTW (CONFIRMED by the table list).** Melee stats, armour and the projectile reference sit inline in `unit_stats_land`; missile weapons are `projectiles` + `gun_types`.

Example rows (unit_stats_land; full transposed views were made with `data_tools db unit_stats_land`):
- Inf_Line_Austrian_German_Fusiliers: men 160, armour 3 (leather), accuracy 40, reload 40, ammo 10, projectile musket_flintlock (flintlock), melee weapon socket_bayonet, melee_attack 6, charge 10, defence 6, morale 6, training "trained", entity infantry_euro_medium.
- Cav_Heavy_French_Cuirassiers: men 60, mounts 60, armour 8 (plate), melee 13, charge 19, defence 11, morale 11, well_trained, entity infantry_euro_heavy on horse_heavy.
- Art_Foot_French_12_lber: men 24, guns 4, accuracy 50, reload 40, ammo 30, gun_type cannon_12_pounder_France, morale 3.
Projectiles: musket_flintlock range 80, velocity 150, reload 20 s (col 24); rifle range 125, reload 45; 12-lb round shot range 600; canister 65 balls, range 150.

### 3.3 Foreign keys (INFERRED from matching values)
`units.key` → `unit_stats_land.key` / `unit_stats_naval.key`; `units.category` → `unit_category`; `units.class` → `unit_class`;
`unit_stats_land.{officer, musician, standard_bearer}` → `battle_personalities.key`; `.man_entity` / `.mount_entity` → `battle_entities.key`; `.mount_variant` → `mounts.key`; `.gun_type` → `gun_types.key`; `.projectile` → `projectiles.key`; `.training_level` → `entity_training_levels.key`;
`gun_type_to_projectiles(gun_type → gun_types, projectile → projectiles)`; `projectiles.shot_type` → `projectile_shot_type_enum`; `.missile_type` → `projectiles_missile_type_enum`; `.explosion` → `projectiles_explosions`;
`fatigue_effects.category` → `unit_category`; `unit_to_unit_abilities_junctions(unit → units, ability → unit_abilities)`; `unit_class_to_unit_ability_junctions`;
`effect_bonus_value_*_junction(effect → effects, target)` connect campaign effects (buildings, techs, traits) to battle stats, e.g. `effect_bonus_value_projectile_junctions` = `mod_reload_rate_carbine, reload_mod, musket_carbine`. Campaign-side FKs: see Worker 3.

## 4. Localisation
Moved to Worker 3 (format and key mapping). From the pack index (CONFIRMED): `text\localisation.loc` and `text\ui.loc` exist in both local_en.pack and local_en_patch.pack, and the patch copies win (section 2). `units.col1` holds an English dev name; on-screen strings come from the .loc files.

## 5. Scripts and scripting API → Worker 3 (`worker3\WORKER3_REPORT.md`, `worker3\lua_api.txt`).
My only finding (CONFIRMED): data.pack holds 497 `.luac` and 10 `.lua` (export_triggers, export_ancillaries, export_historic_characters, export_missions, export_advice, events, episodicscripting, scripting_library[_wellington], export_historic_events). The loose `all_scripted.lua` / `battle_scripted.lua` are small source files that `require` them.

## 6. Campaign / startpos / save data → Worker 3.

## 7. Campaign systems (DB evidence; detailed decoding by Worker 3)
The table families present (CONFIRMED names and row counts in `db_list.tsv`): regions(159)/regions_continents, factions(77)/faction_groups, building_chains/levels/effects/units_allowed, technologies(+effects, required tech, faction junctions, threads), government_types(+effects), ministerial_positions, taxes_levels/classes/effects, trade_nodes/commodities/resources, diplomacy_* strings, diplomatic_relations_{religion,government_type,attitudes}, agents/agent_attributes/abilities, traits/ancillaries (+triggers, effects), missions, events, public_order_factors, region_economics_factors, town_wealth_growth_factors, population_classes, religions, seasons, campaign_variables(+junction per campaign), campaign_difficulty_handicap_effects.
**Difficulty handicaps** (`campaign_difficulty_handicap_effects`: difficulty i32 -2..+?, is_ai bool, effect str, value f32; CONFIRMED values): at -2 (easy, AI rows): policing_cost_mod +30, recruitment_cost_mod_land_all +20, research_rate_mod -12, upkeep_cost_mod_land_all +18. At 0 (AI): recruitment -5 land / +8 naval, upkeep +10. At 0 (player rows): building_cost_mod_* -8, gdp_mod_all +10. Full table: `ai_tables\campaign_difficulty_handicap_effects.tsv`.

## 8. Battle systems (what the data shows)
- **Units / soldiers:** a unit = N men (`num_men` 24..160 at the shipped scale) each a `battle_entities` body (walk 1.25..1.55 m/s, run 3.15..4.05, charge 3.9..5.15, radius 0.35, mass 80..100 kg; horses walk 2.6..2.7, run 10..11, charge 11.5..12.25, mass 650..900). There are separate officer, musician and standard-bearer personalities. This confirms a unit → soldier hierarchy.
- **Formations / spacing:** 4 spacing floats per unit (infantry 0.8/1.75/2.2/4 m). `groupformations.bin` (data.pack root) holds AI group formations (format not analysed). `unit_abilities` lists formations and abilities (square, wedge/diamond, fire_and_advance, chevaux_de_frise, earthworks, ...).
- **Movement:** `unit_movement_modifiers` ground multipliers (road 1.5, grassland 1, mud 0.6..0.8, mud_wet 0.4..0.5, forest 0.4..0.9). `_kv_fatigue` gradient multipliers (shallow 133%, steep 166%, very steep 200%).
- **Melee:** stats melee_attack / charge_bonus / melee_defence / armour per unit. KV rules: `factor_attackdir_front 0 / flank 8 / rear 18`, `hnbonus_bayonet 15`, `hnbonus_melee_cavalry_v_infantry 15`, `..._v_squareinfantry -25`, `melee_height_delta ±1.5`, `relative_melee_height_delta_divisor 0.04`, knockback / knockdown / stepback thresholds per hit-level band (`melee_hn_to_xholds_*_max -6/0/6/12`; knockdown 30/50/60/80/100), `armour_melee_piercing_divisor 2`, `armour_melee_penetrating_divisor 4`, `defense_melee_*` 2/4, `relative_melee_experience_multiplier 2`, `relative_melee_fatigue_multiplier 3`, `melee_charge_factor_power_divisor 1`. **Exact hit formula: NEEDS WORKER 1 / exe.** The exe debug strings show parts of it, e.g. "Kill Chance = (60XProjectile Damager=)%d + (Attacker Accuracy(%d) / 1.5)" and "Marksmanship = (Attacker Core Marksmanship + Projectile Marksmanship Bonus) X Accuracy Modifier Factor" (from worker1 strings_all_rs.tsv).
- **Ranged:** accuracy 0..80 and reload_skill 0..90 per unit; projectile range / velocity / reload time / shots. KV: `missile_distance_for_half_chance_hit 50` (artillery 125, naval 150), `attackpower_long_range_multiplier 0.65`, `extreme 0.3`, `projectile_damage_distance_multiplier 20`, `projectile_damage_armour_divisor 1`, `defense_divisor 2`, firing drills (platoon fire reload modifier 50, improved platoon 50, fire_and_advance 30), misfire chances (cannon flintlock 0.01, matchlock 0.02, musket percussion 0.06), `missile_cover_factor_*` all 0.
- **Morale (`_kv_morale`, CONFIRMED values):** state thresholds impetuous ≥50, eager 24..50, confident 8..30, steady 0..10, shaken -3..2, wavering -5..-1, broken -15..-2. Casualty penalties: recent 6%→-1, 10%→-4, 15%→-6, 33%→-8, 50%→-15; extended 10%→-2 … 80%→-15; total 20%→-1, 40%→-3, 60%→-10, 80%→-35, 90%→-50. Modifiers: flanks exposed -1/-2, rear attack -5, flank attack -3, general dead -2 / died recently -6, panic -50, surprised -7, artillery fire -4, projectile fire -2, tired -1, very tired -2, exhausted -4; encouraged by flanks secure +4, hill +4, fortification +6, inspired +6, column formation +5; charge_bonus +5; blood bonuses +2/+4/+7; timeouts waver 40, broken 600, charge 90, surprise 300; routing effect distance 75 front / 30 flank; fear range 50. The aggregation formula is exe-side (NEEDS WORKER 1).
- **Fatigue (`_kv_fatigue`):** per-tick accumulation by activity (idle -8, walking 0, running 9, charging 15, combat 10, shooting 8, reloading 8, artillery reloading 10, working 20, tight formation +6, under fire 5/6, idle in rain -4, snow -2). Thresholds: fresh 0, active 1800, winded 3600, tired 7200, very tired 10800, exhausted 21600, max 28800. `fatigue_effects` penalties per category (infantry tired -10%, very tired -25%, exhausted -50% on 3 stats and -5/-10/-15% on a 4th). The tick length is UNKNOWN (exe).
- **Experience:** thresholds by kills (150, 400, 1100, 1700, 2400, 3100) or casualties; per-rank bonuses up to rank 9 (`+7, +7, +18, +6, +18, -3, 360, ×1.9`).
- **Abilities:** inspire/rally 30 s duration?, 180 s recharge?, 2 uses; artillery boosts 60/120/3; KV inspire melee +2, marksmanship +10, morale +4; rally bonus 40..65.
- **Terrain / weather / sieges:** battle_terrain_* tables, battle_weather_types, battle_climate_weather_descriptions, battlefield_buildings (+categories, transformations), battlefield_deployable_siege_items, battle_cities. Structurally decoded; semantics not studied (low priority).
- **Battle setup:** battle_types (9 rows), battle_type_setup_limits, battle_type_faction_presets, battles (historical), famous_battle_pools. Objectives and reinforcements live in `.battle_script` files and the exe: UNKNOWN from the DB.

## 9. AI evidence (CONFIRMED tables; values in `ai_tables\`)
- Campaign AI: `campaign_ai_personalities` (default, eur_france, spa_spender); `campaign_ai_personality_junctions` (258 rows, 245 distinct tunables such as BASE_CONSTRUCTION_BIAS_{ECONOMIC 0.2, MILITARY 0.5, ...}, BASIC_SPENDING_BIAS_*, BAWP_WEIGHTING_* (alliance/war weights), COMPOSITE_VALUE_ANALYSER_*, HLP_DISTANCE_MULTIPLIER_*, LOOT_SETTLEMENT_*, INVASION_*, FINANCIAL_ANALYSIS_*). `campaign_ai_managers` (10 per-campaign/per-faction managers) with `campaign_ai_manager_behaviour_junctions` (236 rows: manager × behaviour → priority f32). There are 30 behaviours: WAR_AND_PEACE, DIPLOMACY_MANAGER, EXPANSION_BEHAVIOUR, REGION_DEFENCE, REGION_GROUP_DEFENCE, REGION_COAST_DEFENCE, RECAPTURE, RAID, HELP_ALLY_AT_WAR, TAXATION, RESEARCH_TECHNOLOGY, GLOBAL_CONSTRUCTION, REGIONAL_DEVELOPMENT, TRADE_*, NAVY_*, CULL_EXCESS_FORCES, MERGE_UNITS, MINIMUM_SAVING, FORT_MAINTAINANCE, MISSIONARY, PORT_BLOCKADE, MILITARY_RANK, RECALL, EXCESS_RECRUITMENT. **This is the campaign AI architecture: a set of behaviour modules with per-personality weights.** The weighting logic is exe-side (`CAI_*` classes; Worker 3 notes that `CAI_INTERFACE` is the biggest startpos block).
- Campaign director (`cdir_*`): `cdir_configs` (91 key/value strings, e.g. `CDIR_CVN_AUTORESOLVER_MODIFIER_ARMY_DAMAGE_BIG_LOSER 0.5`, `..._WINNER -0.25`), `cdir_unit_qualities` (434 rows: unit → quality value per balance group, e.g. rocket troops 800), `cdir_unit_balances` / `_groups` / `_group_qualities`, `cdir_desire_priorities`. This is autoresolve and AI army-composition data.
- Battle AI: no battle-AI tuning table exists in the DB (CONFIRMED by table names). `battle_personalities` is about soldier roles (officer/drummer), not AI. Battle AI lives in the exe (Worker 1 strings: `AI_MELEE_ATTACK_ANALYSER`, ...). `logs\cai\` (80 files in data.pack) are campaign AI debug logs (not analysed).

## 10. Data ↔ exe interaction (what the exe must do; questions for Worker 1)
1. Mount packs by type (boot → release → patch), path → (pack, offset, size), with later packs overriding. Q: the order within a type? Do loose files override?
2. The DB loader reads `[FCFDFEFF ver] 01 count` and then a fixed per-table field sequence (ANSWERED by W1: DB_BUILDERS.md). Still open: are the all-zero 4-byte columns unit_stats_land #37 and #51 i32 or f32, and what do the 30 flags (#52..#65, #69..#84) and units #6/#7/#9/#20/#22 mean? Their runtime consumers would name them.
3. KV tables are looked up by key string and mostly truncated to int (worker1 kv_layout). The battle code then combines unit stats, KV divisors and factors. Q: the melee hit/kill formula, the missile hit formula (the "Kill Chance" / "Marksmanship" debug strings), how armour vs piercing/penetrating divisors apply, how the morale sum maps onto the `ums_*` thresholds, the fatigue tick period, and the experience bonus column semantics.
4. `entity_training_levels` f32 (0.1..1.7): where is it used? (formation jitter? reaction delay?)
5. `effect_bonus_value_*` junctions: the campaign → battle modifier pipeline (e.g. reload_mod on musket_carbine).
6. `unit_special_ability_types` is truncated in the file. Does the loader tolerate it (reading fewer rows), or is the table unused?

## 11. UNKNOWNs
- Column names for every DB table (not stored anywhere in the data; INFERRED only). There is no schema, .csv or .ods in any pack.
- i32 vs f32 for all-zero columns; the exact split of shared zero runs.
- The semantics of 30 flag and ~8 misc columns in unit_stats_land (exe numbering), and of units #6/#7/#9/#15/#20/#22.
- Every combat, morale and fatigue *formula* and tick rate (exe).
- Pack priority inside the same type; loose-file override behaviour.
- models_building / models_naval: not inferred (skipped); use the DB_BUILDERS.md layouts.

## 12. Data needs for the Rust design
The minimum set our `sim` crate needs, in order, and how the tables link:
1. **Unit definition** = `units` (key, category, class, cost, upkeep) ⨝ `unit_stats_land` (men, mounts, guns, armour, accuracy, reload, ammo, melee_attack, charge, defence, morale, spacing, training, entity/projectile/gun FKs).
2. **Soldier body** = `battle_entities` via `man_entity` / `mount_entity` (speeds, radius, mass).
3. **Missile weapon** = `projectiles` (range, velocity, reload time, shots, damage?) via `unit_stats_land.projectile`; artillery via `gun_types` + `gun_type_to_projectiles`.
4. **Global rules** = `_kv_rules`, `_kv_morale`, `_kv_fatigue` (a single `Tunables` resource keyed by name, truncated to int where the exe does so) + `fatigue_effects` + `unit_movement_modifiers` + `entity_training_levels`.
5. **Progression** = `unit_experience_thresholds` + `unit_stats_land_experience_bonuses`.
6. **Abilities / formations** = `unit_abilities` + `unit_to_unit_abilities_junctions` + `unit_special_abilities` + `unit_class_to_unit_ability_junctions`.
7. **Campaign** (Worker 3): factions → regions → building_levels → effects (`*_effects_junction` + `effect_bonus_value_*`) → technologies; `campaign_difficulty_handicap_effects`; AI personality and behaviour weights (`campaign_ai_*`, `cdir_*`).
Mirror each table as a Rust struct with fields in file order (the type codes from `db_schemas.tsv`). Keep the original key strings as IDs and resolve FKs into typed indices at load time. `data_tools/src/db.rs` already has the byte-exact reader for loading the user's own installed files.

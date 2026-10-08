# NTW DB schemas: battle tables (Worker 2)

**Authoritative column order and types: Worker 1 `worker1\DB_BUILDERS.md` (decompiled exe row readers).** This file adds:
(a) the int-vs-float classification of each 4-byte column, from the data (the exe reader copies 4 raw bytes);
(b) inferred column names;
(c) real example values.
Every schema below was re-checked with `data_tools db <table>`, which now uses these exe layouts by default (`db::known_schema`). Each one parses all rows and ends exactly at EOF.
Name confidence: H = semantics unambiguous from the values, M = likely, L = guess, ? = unknown. **All names are INFERRED** (the files carry no column names).
Classification rule: a column is f32 when its values only make sense as floats (exponent bytes 0x3F..0x46 / 0xBF.., e.g. 0x42480000 = 50.0). It is i32 when all values are small integers. An all-zero column is "i32?" (UNKNOWN: it could be f32 0.0).

## unit_stats_land (v5, 328 rows, 89 columns; exe reader 0x00E84B00; LAND_UNIT_RECORD)
Examples: A = Art_Foot_French_12_lber, C = Cav_Heavy_French_Cuirassiers, I = Inf_Line_Austrian_German_Fusiliers.

| # | type | inferred name | conf | A / C / I (range over 328 rows) |
|---|---|---|---|---|
| 0 | s | key (FK units) | H | |
| 1 | i32 | num_men | H | 24 / 60 / 160 (24..160) |
| 2 | i32 | num_mounts | H | 0 / 60 / 0 (0..80) |
| 3 | i32 | num_guns (engines) | H | 4 / 0 / 0 (0..8) |
| 4 | s | officer (FK battle_personalities) | H | euro_officer |
| 5 | o | musician (FK battle_personalities) | H | – / euro_cavalry_bugler_sword / euro_drummer |
| 6 | o | standard bearer (FK battle_personalities) | H | – / – / euro_standard_bearer |
| 7 | s | animation culture set | M | euroline (only value) |
| 8 | s | man_entity (FK battle_entities) | H | infantry_euro_medium / infantry_euro_heavy / infantry_euro_medium |
| 9 | s | man skeleton/animation type | M | rider_sabre / rider_sabre / man_musket |
| 10 | s | weapon anim group | M | generic_gun / generic_cavalry_sword / generic_musket |
| 11 | i32 | armour | M | 2 / 8 / 3 (1..8) |
| 12 | s | armour type | H | leather / plate / leather |
| 13 | i32 | ? (always 1) | ? | 1 (this was mis-split as `ostr "" + bool` in my earlier inference) |
| 14 | o | mount (FK mounts, exe +0x80) | H | horse_artillery_mixed_brown / horse_covered_light_brown / – |
| 15 | o | mount_entity (FK battle_entities) | H | horse_medium / horse_heavy / – |
| 16 | o | mount type | H | mount_horse |
| 17 | s | numeric text A (mount-related) | L | "0" / "6" / "0" |
| 18 | s | numeric text B | L | "0" / "1" / "0" |
| 19 | o | gun-train entity (FK battle_entities) | H | gun_train_2_horse |
| 20 | o | ammo caisson entity (FK battle_entities) | H | ammo_caisson_small |
| 21 | o | limber model | H | limber_model_France |
| 22 | o | engine model | H | Field_Cannon_12lb_France |
| 23 | b | is_artillery / has_engine | M | true / false / false |
| 24 | o | gun_type (FK gun_types, exe +0x11C) | H | cannon_12_pounder_France |
| 25 | o | missile weapon class | M | – / – / musket |
| 26 | i32 | accuracy | H | 50 / 0 / 40 (0..80) |
| 27 | i32 | reload skill | H | 40 / 0 / 40 (0..90) |
| 28 | s | ? (always "0") | ? | |
| 29 | s | firing mechanism | H | matchlock / none / flintlock (matches the `misfire_*` KV keys) |
| 30 | o | projectile (FK projectiles, exe +0x154) | H | – / – / musket_flintlock |
| 31 | i32 | ammunition | M | 30 / 0 / 10 (0..100) |
| 32 | o | firing animation set | M | cannon_crew / – / foot_musket |
| 33 | s | melee weapon type | H | sword / sword / socket_bayonet |
| 34 | i32 | melee attack | H | 2 / 13 / 6 (1..17) |
| 35 | i32 | charge bonus | H | 3 / 19 / 10 (1..44) |
| 36 | i32 | melee defence | H | 2 / 11 / 6 (0..14) |
| 37 | i32? | (always 0; shield?) | ? | 0 |
| 38 | s | melee animation category | M | one_handed / mounted_sword / foot_bayonet |
| 39 | s | ? (text "0") | ? | |
| 40 | s | drill set | H | drill_set_artillery / drill_set_cavalry / drill_set_infantry_line |
| 41 | s | training level (FK entity_training_levels) | H | trained / well_trained / trained |
| 42 | i32 | morale | H | 3 / 11 / 6 (3..14) |
| 43 | i32 | ? small int | L | 1 / 3 / 3 (1..8) |
| 44 | f32 | spacing: file, close order (m) | M | 8 / 2 / 0.8 |
| 45 | f32 | spacing: rank, close order | M | 12 / 4.5 / 1.75 |
| 46 | f32 | spacing: file, loose | M | 16 / 4 / 2.2 |
| 47 | f32 | spacing: rank, loose | M | 16 / 9 / 4 |
| 48 | f32 | artillery-only spacing A | L | 1.5 / 0 / 0 |
| 49 | f32 | artillery-only spacing B | L | 3 / 0 / 0 |
| 50 | f32 | ? (art 7, cav 10, inf 20) | L | 7..20 |
| 51 | i32? | (always 0) | ? | |
| 52..65 | b×14 | ability/behaviour flags | L | #54 true for line inf; #57, #59 artillery |
| 66 | f32 | ? | L | 50 (always) |
| 67 | f32 | ? | L | 75..100 |
| 68 | f32 | ? | L | 100..150 |
| 69..82 | b×14 | flags | L | #71 true for cav+inf; #77, #80 line inf |
| 83 | b {v≥1} | flag | ? | |
| 84 | b {v≥2} | flag | ? | |
| 85 | o {v≥3} | second musician (FK battle_personalities, exe +0x214) | H | – / – / euro_flutist |
| 86 | o {v≥4} | destroyed caisson model | H | Ammo_Caisson_France_destructed |
| 87 | o {v≥4} | caisson destruction anim | H | Ammo_Caisson_France_destruction |
| 88 | b {v>4} | ? | ? | |

The 30 flags (#52..#65, #69..#84) need names from their exe consumers (NEEDS WORKER 1 / exe). Candidate meanings from the unit cards and abilities: can_skirmish, can_form_square, fire_and_advance, can_hide_in_forest, guerrilla deploy, light infantry, can_build_stakes, etc.

## units (v4, 442 rows land+naval, 25 columns; exe 0x00E85B20)
| # | type | inferred name | conf | I / C / A / 2_Decker_74 |
|---|---|---|---|---|
| 0 | s | key | H | |
| 1 | s | English dev name | H | German Fusiliers |
| 2 | s | category (FK unit_category) | H | infantry / cavalry / artillery / naval_line_of_battle |
| 3 | s | class (FK unit_class) | H | infantry_line / cavalry_heavy / artillery_foot / naval_third_rate |
| 4 | i32 | recruitment cost | M | 700 / 980 / 950 / 1940 |
| 5 | i32 {v≥1; else = #4} | cost 2 (MP / custom) | M | equal to #4 in sampled rows |
| 6 | i32 | ? (inf 2, art 3, cav 4, ships 8) | L | |
| 7 | i32 | ? (≈0.8 × cost) | L | 580 / 780 / 760 / 1590 |
| 8 | i32 | upkeep | M | 140 / 250 / 190 / 390 |
| 9 | i32 | ? (23..55) | L | 27 / 29 / 23 / 55 |
| 10 | o | unit card group / commander type | L | infantry_generic / cavalry_cuirassiers / artillery_medium_cannon / naval_captain |
| 11 | s | campaign model | M | placeholder / 2deck_74 |
| 12 | s | model 2 | M | |
| 13 | s | icon / info key | H | |
| 14 | o | recruitment scope | L | global |
| 15 | i32 | ? | ? | 0 |
| 16 | s | MP category | H | mp_infantry / mp_cavalry / mp_artillery |
| 17..19 | b×3 | flags (all true in samples) | ? | |
| 20 | i32 | ? (0, 2, 5, 7, ...) | ? | 0 |
| 21 | b | ? | ? | |
| 22 | i32 {v≥2} | ? (0, 1, 2, 4, ...) | ? | |
| 23 | o {v≥3} | AI / unit role | M | line_infantry / heavy_cavalry / artillery |
| 24 | b {v>3} | ? | ? | |

## projectiles (v1, 144 rows, 35 columns; exe 0x00F3EF70)
This matches the exe exactly (also my independent inference).

| # | type | inferred name | conf | musket_flintlock / cannon_12_pounder_shot / howitzer_5_In_shell |
|---|---|---|---|---|
| 0 | s | key | H | |
| 1 | s | category | H | missile / artillery / artillery |
| 2 | s | shot type (FK projectile_shot_type_enum) | H | bullet / round_shot / explosive_shell |
| 3 | s | missile type (FK projectiles_missile_type_enum) | H | bullet / cannon_ball / cannon_ball |
| 4 | o | projectile model | M | projectile_musketball / projectile_cannonbal |
| 5 | o | explosion (FK projectiles_explosions) | M | – / – / shell_12lb |
| 6 | o | trail texture | L | alpha_bullet / alpha |
| 7 | s | spin type | M | none / none / shell_spin |
| 8 | i32 | projectiles per shot | H | 1 / 1 / 1 (canister 35..65) |
| 9 | o | sub-projectile (shrapnel) | M | – / – / shrapnel |
| 10 | s | trajectory class | L | low / low / fixed |
| 11 | i32 | effective range (m) | H | 80 / 600 / 400 (rifle 125, pistol 40) |
| 12 | i32 | minimum range | M | 0 / 0 / 100 |
| 13 | i32 | max elevation? | L | 88 / 30 / 20 |
| 14 | f32 | muzzle velocity | M | 150 / 250 / 200 |
| 15 | f32 | marksmanship/accuracy modifier | L | 0 / 0 / -50 |
| 16 | f32 | spread | L | 0 / 0 / 0 (canister 4..8) |
| 17 | f32 | damage | L | 0.75 / 30 / 18 |
| 18 | o | penetration class | L | ap / low / high |
| 19 | o | incendiary chance class (KV projectile_incendiary_chance_*) | M | – / low / medium |
| 20 | o | impact behaviour | M | – / impact / explode |
| 21 | b | bounces? | L | false / true / false |
| 22 | b | ? | ? | |
| 23 | f32 | ? | ? | 0.1 |
| 24 | i32 | reload time (s) | M | 20 / 25 / 25 (rifle 45, carbine 15, pistol 10) |
| 25 | i32 | shots per volley | L | 1 (pistol 2) |
| 26 | o | size class | L | medium |
| 27 | f32 | ? | ? | 0.1 / 0.1 / 0.2 |
| 28 | f32 | ? | ? | 0 |
| 29 | s | weapon class | H | small_arm / gun / gun |
| 30 | s | calibre size | H | small / large / large |
| 31 | o | fire sound event | H | MusketFire / LandGunFire / LandGunFire_howitzer |
| 32 | o | ? | ? | |
| 33 | o | impact effect | M | musket_ball / default_ball |
| 34 | o {v≥1} | weapon family | H | musket_flintlock / cannon / howitzer |

## Other battle tables (types agree with the exe; full values in `battle_tables\`)
- `gun_types` (62): 8 strings: key, size class, model, engine class, weapon anim group, destroyed model, destruction anim, battle category.
- `gun_type_to_projectiles` (153): gun_type, projectile, muzzle flash.
- `battle_entities` (20): `s s s f×10 s f×6 i32`. Example infantry_euro_medium: walk 1.4, run 3.6, accel 2.4, decel 5, charge 4.05, 0.8, turn rates? 20/15/10, radius 0.35, "circle", 1, mass 90, 2, 70, 70, 90, 1. horse_heavy: walk 2.6, run 10, charge 11.5, mass 900. (Names M/L.)
- `entity_training_levels` (6): `s f`: elite 0.13, well_trained 0.1, trained 0.2, poorly_trained 0.38, rabble 0.5, mob 1.7 (meaning UNKNOWN).
- `unit_experience_thresholds` (18): `s i`: xp1..6 land kills 150/400/1100/1700/2400/3100; casualties 350/650/1050.
- `unit_stats_land_experience_bonuses` (10): `s i×7 f`: rank "9" = 7, 7, 18, 6, 18, -3, 360, 1.9.
- `fatigue_effects` (30): `s s f×4`. The exe builds the key as `s1;s2` (threshold;category). Infantry tired -0.1, -0.1, -0.05, -0.1; exhausted -0.5, -0.5, -0.15, -0.5.
- `unit_movement_modifiers` (28): `s f×4`: road 1.5, grassland 1, mud 0.6/0.65/0.8/0.8, forest 0.4/0.4/0.8/0.9.
- `unit_abilities` (26): `s b o b`; `unit_special_abilities` (4): `s f f i` (inspire/rally 30, 180, 2; artillery boosts 60, 120, 3).
- `projectile_shot_type_enum` (26): `s b b o b b`; `projectiles_missile_type_enum` (13): `s`; `mounts` (28): `s`; `battle_personalities` (51): `s×5`.
- `_kv_rules` (97), `_kv_morale` (69), `_kv_fatigue` (36), `_kv_naval_morale` (62): `s f`, with key = name. The exe truncates most to int (worker1 `kv_layout.tsv`). `_kv_rules_ext` (`s i i`) and `_kv_morale_ext` (`s i×4`) contain only 5/10 values (editor metadata? INFERRED).

# Appendix: all 310 DB tables (from `db_schemas.tsv`, Rust data_tools final run)

Status: **exe** = layout taken from Worker 1's DB_BUILDERS.md (authoritative). **inferred** = byte-level inference that parses all rows to EOF with plausible numeric columns; types are reliable except for zero-run ambiguity, so check DB_BUILDERS.md before relying on one. **FAIL** = not solved. Campaign tables: Worker 3's `DB_CAMPAIGN_TABLES.md` has names and semantics and supersedes this list. Type codes: str, ostr, bool, i32, f32 (4-byte, classified from the values; an all-zero column prints as i32).

| table | ver | rows | status | columns (types in order) |
|---|---|---|---|---|
| _kv_fatigue | 0 | 36 | inferred | str,f32 |
| _kv_morale | 0 | 69 | inferred | str,f32 |
| _kv_morale_ext | 0 | 64 | inferred | str,i32,i32,i32,i32 |
| _kv_naval_morale | 0 | 62 | inferred | str,f32 |
| _kv_rules | 0 | 97 | inferred | str,f32 |
| _kv_rules_ext | 0 | 92 | inferred | str,i32,i32 |
| abilities | 0 | 14 | inferred | str,ostr,str,bool |
| achievements | 0 | 76 | inferred | str |
| advice_levels | 2 | 1349 | inferred | str,str,i32,i32,str,str,str,i32,i32,i32,bool,bool,bool,str,ostr,bool,bool,bool,str,str,bool |
| advice_threads | 0 | 1272 | inferred | str |
| advisors | 1 | 9 | inferred | str,str |
| agent_attribute_situations | 0 | 16 | inferred | str,str |
| agent_attributes | 0 | 14 | inferred | str,str |
| agent_culture_details | 0 | 53 | inferred | str,str,str,ostr |
| agent_spawning_to_building_chains | 0 | 0 | inferred |  |
| agent_spawning_to_government_types | 0 | 19 | inferred | str,str,f32 |
| agent_spawning_to_policies | 0 | 0 | inferred |  |
| agent_spawnings | 0 | 17 | inferred | str,f32 |
| agent_to_agent_abilities | 0 | 39 | inferred | str,str,ostr,bool |
| agent_to_agent_attributes | 0 | 30 | inferred | str,str,i32,bool |
| agent_to_bribe_actions | 0 | 3 | inferred | str,str |
| agent_to_building_levels | 0 | 0 | inferred |  |
| agents | 1 | 17 | inferred | str,i32,i32,i32,bool,str,ostr,bool,str,ostr,i32,i32 |
| aide_de_camp_speeches | 1 | 37 | inferred | str,str,i32,bool,f32 |
| ancillaries | 0 | 275 | exe | str,str,str,bool,bool,bool,i32,i32,i32 |
| ancillary_included_subcultures | 0 | 1235 | inferred | str,str |
| ancillary_info | 0 | 275 | inferred | str |
| ancillary_to_ability_effects | 0 | 0 | inferred |  |
| ancillary_to_attribute_effects | 0 | 58 | inferred | str,str,i32 |
| ancillary_to_attribute_situation_effects | 0 | 0 | inferred |  |
| ancillary_to_effects | 0 | 317 | inferred | str,str,f32 |
| ancillary_to_excluded_ancillaries | 0 | 238 | inferred | str,str |
| ancillary_to_included_agents | 0 | 333 | inferred | str,str |
| ancillary_types | 0 | 50 | inferred | str,str |
| anim_reference_poses | 0 | 3 | inferred | str,str,str |
| battle_bridge_subculture_jcts | 0 | 13 | inferred | str,str,str |
| battle_cities | 0 | 4 | inferred | str,f32,f32,f32,f32,f32,f32,i32,i32 |
| battle_city_buildings | 0 | 31 | inferred | str,str,i32,i32 |
| battle_city_subculture_jct | 0 | 12 | inferred | str,str |
| battle_climate_groupings | 0 | 5 | inferred | str |
| battle_climate_weather_descriptions | 0 | 660 | inferred | str,str,str,str,i32,i32,i32,i32,i32,i32,i32 |
| battle_entities | 0 | 20 | inferred | str,str,str,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,str,f32,f32,f32,f32,f32,f32,i32 |
| battle_groundcover_density_maps | 0 | 437 | inferred | str,str,str |
| battle_groundcover_distribution_maps | 0 | 138 | inferred | str,str,i32 |
| battle_personalities | 0 | 51 | inferred | str,str,str,str,str |
| battle_script_strings | 1 | 296 | inferred | str,str,str |
| battle_sequences | 0 | 12 | inferred | str,i32 |
| battle_sky_types | 0 | 1675 | inferred | str,str,str,str,ostr,str,bool,bool |
| battle_terrain_farm_walls | 0 | 16 | inferred | str,str,str,str,str,str,str,str,str,str,str |
| battle_terrain_farms | 0 | 46 | inferred | str,str,str,str,str,str,str,str,str,str,str,ostr,str,str,str |
| battle_terrain_set_climates_jcts | 0 | 23 | inferred | str,str |
| battle_terrain_set_groupings | 0 | 25 | inferred | str,str |
| battle_terrain_sets | 0 | 81 | inferred | str,i32,str,str,str,ostr,str,str,str,str,str,str,str,str,str |
| battle_type_faction_presets | 0 | 92 | inferred | str,i32,i32 |
| battle_type_setup_limits | 0 | 144 | inferred | str,str,str,str,i32,i32,i32,i32,i32,i32,i32 |
| battle_type_unit_to_faction_presets | 0 | 1120 | inferred | str,i32,str,i32 |
| battle_types | 0 | 8 | inferred | str |
| battle_weather_types | 0 | 10 | inferred | str,i32,i32,i32,f32,f32,i32,bool |
| battlefield_building_categories | 0 | 18 | inferred | str,str,str |
| battlefield_building_transformations | 0 | 4 | inferred | str |
| battlefield_buildings | 1 | 542 | inferred | str,str,str,str,i32,ostr,ostr,i32 |
| battlefield_deployable_siege_items | 0 | 3 | inferred | str,i32,i32,str,str,str |
| battlefield_snow_props | 0 | 27 | inferred | str,bool |
| battles | 1 | 87 | inferred | str,str,bool,str,ostr,i32,i32,bool,bool,bool,bool,ostr,i32 |
| battles_to_battle_sky_types_junctions | 0 | 858 | inferred | str,str |
| bribe_actions | 0 | 4 | inferred | str,str |
| building_chain_to_slots | 0 | 82 | inferred | str,str |
| building_chains | 0 | 48 | inferred | str,ostr,ostr,ostr |
| building_culture_gov_type_variants | 0 | 0 | inferred |  |
| building_culture_variants | 0 | 264 | inferred | str,str,ostr,ostr,ostr,ostr,ostr |
| building_description_texts | 0 | 197 | inferred | str |
| building_effects_junction | 0 | 354 | inferred | str,str,f32 |
| building_faction_variants | 0 | 44 | inferred | str,str,ostr,ostr,ostr |
| building_factionwide_effects_junctions | 0 | 98 | inferred | str,str,f32 |
| building_level_required_technology_junctions | 0 | 21 | inferred | str,str |
| building_levels | 0 | 137 | exe | str,str,i32,str,i32,i32,i32,i32,i32,i32,i32,i32,i32,i32,i32,i32,str,str,i32,bool,i32,i32,i32,i32 |
| building_research_thread_junction | 0 | 0 | inferred |  |
| building_resources_junction | 0 | 0 | inferred |  |
| building_units_allowed | 0 | 1708 | inferred | str,str,i32,bool |
| building_upgrades_junction | 0 | 89 | inferred | str,str |
| campaign_ai_manager_behaviour_junctions | 0 | 236 | inferred | str,str,f32 |
| campaign_ai_managers | 0 | 10 | inferred | str |
| campaign_ai_personalities | 0 | 3 | inferred | str,bool |
| campaign_ai_personality_junctions | 0 | 258 | inferred | str,str,f32 |
| campaign_anim_action_to_sets | 0 | 19 | inferred | str,str,str |
| campaign_anim_sets | 0 | 2 | inferred | str |
| campaign_anim_transitions | 0 | 12 | inferred | str,str,str,str |
| campaign_anims | 0 | 16 | inferred | str,str,bool,bool,str |
| campaign_character_anim_set_agent_junctions | 0 | 25 | inferred | str,str,ostr,ostr |
| campaign_character_anim_sets | 0 | 8 | inferred | str,str |
| campaign_character_anim_walk_anim_junctions | 0 | 17 | inferred | str,str,str,str |
| campaign_character_anims_junctions | 0 | 163 | inferred | str,str,str,str,str,ostr,ostr,f32 |
| campaign_difficulty_handicap_effects | 0 | 88 | inferred | i32,bool,str,f32 |
| campaign_ground_types | 1 | 19 | inferred | str,f32,bool,bool,bool |
| campaign_map_famous_battles | 0 | 0 | inferred |  |
| campaign_map_playable_areas | 0 | 5 | inferred | str,bool,ostr,ostr,ostr,str,i32,i32,i32 |
| campaign_map_settlements | 0 | 159 | inferred | str,str,str,i32,str |
| campaign_map_slots | 0 | 145 | inferred | str,str,str,i32,bool |
| campaign_map_slots_templates_rotations | 0 | 4 | inferred | str |
| campaign_map_tooltips | 0 | 252 | inferred | str,str |
| campaign_map_towns_and_ports | 0 | 233 | inferred | str,str,str |
| campaign_variables | 0 | 121 | exe | str,f32 |
| campaign_walk_anim_sets | 0 | 8 | inferred | str,str,str,str,str,str,str,f32,str,f32,str,str,f32,str,str |
| campaigns_campaign_variables_junctions | 0 | 25 | inferred | str,str,f32 |
| cdir_campaign_junctions | 0 | 9 | inferred | str,ostr |
| cdir_configs | 0 | 91 | inferred | str,ostr,str,ostr |
| cdir_desire_priorities | 0 | 2 | inferred | str,str,i32 |
| cdir_faction_junctions | 0 | 78 | inferred | str,ostr |
| cdir_unit_balance_group_qualities | 0 | 9 | inferred | str,str,str,i32 |
| cdir_unit_balance_groups | 0 | 4 | inferred | str,bool |
| cdir_unit_balances | 1 | 10 | inferred | str,i32,i32,str,f32,f32,i32 |
| cdir_unit_qualities | 0 | 434 | inferred | str,str,str,i32 |
| character_trait_levels | 0 | 475 | inferred | str,i32,str,i32 |
| character_traits | 0 | 163 | exe | str,i32,bool,i32,str |
| climate_to_tilesets | 0 | 2 | inferred | str,str |
| climates | 0 | 33 | inferred | str,i32,i32,i32,bool |
| commodities | 0 | 8 | inferred | str,f32,f32 |
| commodities_demand_drivers | 0 | 6 | inferred | str |
| commodities_demand_junction | 0 | 16 | inferred | str,str,f32,f32 |
| commodity_slot_junction | 0 | 0 | inferred |  |
| commodity_unit_names | 0 | 10 | inferred | str |
| cultures | 0 | 9 | exe | str,i32,ostr |
| cultures_subcultures | 0 | 16 | exe | str,str,i32,str |
| cursors | 0 | 35 | inferred | str,str,i32,i32,i32,i32,bool,i32,i32,bool |
| diplomacy_factor_strings | 0 | 25 | inferred | str |
| diplomacy_negotiation_faction_override_strings | 0 | 537 | inferred | str,str,str,str,str |
| diplomacy_negotiation_strings | 0 | 1387 | inferred | str,str,str,str |
| diplomacy_strings | 0 | 869 | inferred | str |
| diplomatic_relations_attitudes | 0 | 5 | inferred | str,i32 |
| diplomatic_relations_government_type | 0 | 16 | inferred | str,str,i32,i32 |
| diplomatic_relations_religion | 0 | 121 | inferred | str,str,i32,f32 |
| disaster_to_ground_types | 0 | 2 | inferred | str,str |
| disasters | 0 | 7 | inferred | str,f32,str,bool,str,str,f32,f32 |
| diseases | 0 | 5 | inferred | str,str,f32,f32,f32 |
| effect_bonus_value_agent_junction | 0 | 62 | inferred | str,str,str |
| effect_bonus_value_basic_junction | 0 | 143 | inferred | str,str |
| effect_bonus_value_building_chain_junctions | 0 | 95 | inferred | str,str,str |
| effect_bonus_value_commodity_junction | 0 | 7 | inferred | str,str,str |
| effect_bonus_value_population_class_and_religion_junction | 0 | 30 | inferred | str,str,str,str |
| effect_bonus_value_population_class_junction | 0 | 30 | inferred | str,str,str |
| effect_bonus_value_projectile_junctions | 0 | 1 | inferred | str,str,str |
| effect_bonus_value_religion_junction | 0 | 12 | inferred | str,str,str |
| effect_bonus_value_resource_junction | 0 | 11 | inferred | str,str,str |
| effect_bonus_value_shot_type_junctions | 0 | 9 | inferred | str,str,str |
| effect_bonus_value_unit_ability_junctions | 0 | 16 | inferred | str,str,str |
| effect_bonus_value_unit_category_junction | 0 | 35 | inferred | str,str,str |
| effect_bonus_value_unit_class_junction | 0 | 6 | inferred | str,str,str |
| effects | 0 | 412 | inferred | str,ostr,i32 |
| empires | 0 | 6 | inferred | str |
| empires_regions_junct | 0 | 0 | inferred |  |
| entity_training_levels | 0 | 6 | inferred | str,f32 |
| events | 1 | 8 | inferred | str,str,i32,bool,str,ostr,str,i32 |
| events_effect_group_junct | 0 | 0 | inferred |  |
| events_hist_chars_junct | 0 | 0 | inferred |  |
| events_to_policies_junction | 0 | 0 | inferred |  |
| events_view_group_junct | 0 | 8 | inferred | str,str |
| faction_groups | 0 | 2 | inferred | str,bool,str,bool,bool,bool,bool,bool,ostr,bool,bool,bool,bool,bool,bool,ostr,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,i32,i32,i32,i32,i32,i32,bool |
| faction_rebellion_units_junctions | 0 | 566 | inferred | str,str |
| faction_uniform_colours | 0 | 73 | inferred | str,i32,i32,i32,i32,i32,i32,i32,i32,i32 |
| factions | 3 | 77 | exe | str,i32,str,str,str,str,str,str,ostr,bool,bool,bool,str,str,ostr,ostr,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,str,ostr,f32,f32,f32,ostr,str,bool,bool,str,str,str,str,ostr |
| family_trees | 0 | 3 | inferred | str,str,str,str,str,f32,i32,i32,str,str,str,str,str,str,str,str,str,str,str,str,str,str |
| famous_battle_pools | 0 | 49 | inferred | str,f32,f32,i32,str |
| fatigue_effects | 0 | 30 | exe | str,str,f32,f32,f32,f32 |
| fort_underlay_climate_jcts | 0 | 207 | inferred | str,str,bool,str |
| government_types | 0 | 4 | exe | str,bool,bool,i32,str,str |
| government_types_to_effects | 0 | 24 | inferred | str,str,f32 |
| governorships | 0 | 5 | inferred | str |
| groupings | 0 | 25 | inferred | str |
| groupings_continents_junct | 0 | 2 | inferred | str,str |
| groupings_cultures_junct | 0 | 0 | inferred |  |
| groupings_empires_junct | 0 | 0 | inferred |  |
| groupings_factions_junct | 0 | 9 | inferred | str,str |
| groupings_military | 0 | 34 | inferred | str |
| groupings_regions_junct | 0 | 0 | inferred |  |
| groupings_subcultures_junct | 0 | 0 | inferred |  |
| gun_type_to_projectiles | 0 | 153 | inferred | str,str,str |
| gun_types | 0 | 62 | inferred | str,str,str,str,str,str,str,str |
| historical_character_traits | 0 | 928 | inferred | str,str |
| historical_characters | 0 | 505 | exe | str,bool,str,str,str,i32,i32,str |
| lighting_setups | 0 | 4 | inferred | str,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,i32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32 |
| loading_screens | 0 | 11 | inferred | str,str,str,str |
| message_event_strings | 0 | 1023 | inferred | str,str,str,str,str,bool,bool,str,str |
| ministerial_effectiveness_modifiers | 0 | 30 | inferred | i32,str,i32 |
| ministerial_position_default_names | 0 | 3 | inferred | str,str |
| ministerial_positions | 0 | 26 | inferred | str,i32 |
| ministerial_positions_by_gov_types | 0 | 934 | inferred | str,str,str,str,str |
| ministerial_positions_to_effects | 0 | 260 | inferred | str,i32,str,i32,i32 |
| ministerial_positions_to_governorships | 0 | 5 | inferred | str,str |
| mission_activities | 0 | 15 | inferred | str,str |
| mission_effects | 0 | 4 | inferred | str,str |
| mission_sources | 0 | 3 | inferred | str,str |
| missions | 0 | 22 | inferred | str,str,str,str,str,str,ostr,i32,i32,bool |
| models_artilleries | 0 | 34 | inferred | str,str,i32,i32,i32,i32,bool,f32,i32,f32,f32,i32,f32,i32,i32,f32,i32,ostr,bool,bool,f32,i32,f32,f32,i32,f32,i32,i32,f32,ostr,i32,bool,bool,f32,i32,i32,f32,i32,i32,i32,i32,f32,i32,i32,bool,f32,i32,f32,f32,i32,f32,i32,i32,f32,i32,i32,bool,f32,i32,f32,f32,i32,f32,i32,i32,f32,i32,i32,bool,f32,i32,f32,f32,i32,f32,i32,i32,f32,i32,i32,bool,f32,i32,f32,f32,i32,f32,i32,i32,f32,i32,i32,bool,f32,i32,f32,f32,i32,f32,i32,i32,f32,i32,i32,bool,f32,i32,f32,f32,i32,f32,i32,i32,f32,i32,i32,bool,f32,i32,f32,f32,i32,f32,f32,i32,i32,ostr,i32,bool,bool,bool,bool,i32,bool,bool,f32,f32,i32,f32,f32,i32,i32,i32,i32,bool,f32,i32,f32,f32,i32,f32,f32,i32,i32,ostr,i32,bool,bool,f32,i32,i32,f32,i32,i32,i32,i32,f32,i32,i32,bool,f32,i32,f32,f32,i32,f32,f32,i32,i32,i32,i32,bool,f32,i32,f32,f32,i32,f32,f32,i32,i32 |
| models_building | 0 | 538 | FAIL |  |
| models_naval | 0 | 26 | FAIL |  |
| mount_variants | 0 | 46 | inferred | str,str,f32 |
| mounts | 0 | 28 | inferred | str |
| movie_event_strings | 0 | 57 | inferred | str,str,str,str |
| mp_general_command_ratings | 0 | 35 | inferred | str,i32 |
| names | 0 | 13945 | inferred | str,str,str,str,i32,bool,str |
| names_forts | 0 | 559 | inferred | str,str,str |
| names_groups | 0 | 31 | inferred | str,str |
| names_royalty | 0 | 273 | inferred | str,str,i32,str,i32,i32 |
| particle_effects | 0 | 104 | inferred | str |
| pdlc | 3 | 4 | inferred | str,i32,ostr,i32 |
| policies | 0 | 2 | inferred | str,str |
| population_class_to_applicable_effects | 0 | 17 | inferred | str,str |
| population_classes | 0 | 3 | inferred | str,bool,bool,bool |
| projectile_impacts | 0 | 8 | inferred | str,ostr,ostr,ostr,ostr,ostr,ostr,ostr,ostr,ostr,ostr,ostr,ostr,ostr,ostr,ostr,str |
| projectile_shot_type_enum | 0 | 26 | inferred | str,bool,bool,ostr,bool,bool |
| projectile_trails | 0 | 5 | inferred | str,str,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32 |
| projectiles | 1 | 144 | exe | str,str,str,str,ostr,ostr,ostr,str,i32,ostr,str,i32,i32,i32,f32,f32,f32,f32,ostr,ostr,ostr,bool,bool,f32,i32,i32,ostr,f32,f32,str,str,ostr,ostr,ostr,ostr |
| projectiles_explosions | 1 | 35 | inferred | str,str,str,f32,f32,f32,f32,ostr,i32,ostr,f32,f32,ostr |
| projectiles_missile_type_enum | 0 | 13 | inferred | str |
| public_order_factors | 0 | 30 | inferred | str,str,ostr |
| quotes | 0 | 413 | inferred | str,str,str |
| quotes_people | 0 | 170 | inferred | str,str |
| random_localisation_strings | 0 | 1016 | inferred | str |
| region_economics_factors | 0 | 13 | inferred | str,str |
| region_unit_resources | 0 | 37 | inferred | str,str |
| regions | 1 | 159 | inferred | str,str,i32,i32,i32,str |
| regions_continents | 0 | 8 | inferred | str |
| religion_conversion_mods | 0 | 74 | inferred | str,str,f32 |
| religions | 0 | 11 | inferred | str,i32,str |
| resources | 0 | 20 | inferred | str,ostr,str,i32,str |
| sea_climate_details | 0 | 33 | inferred | str,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32 |
| sea_surfaces | 0 | 5 | inferred | str,f32,f32,f32,f32,i32,bool,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32 |
| seasons | 0 | 4 | inferred | str,str,str |
| ship_names | 0 | 2246 | inferred | str,str,str,ostr |
| slots | 1 | 21 | exe | str,bool,bool,bool,bool,bool |
| slots_art | 0 | 72 | inferred | str,str,ostr,bool,ostr,bool,ostr,bool,bool,bool,bool,bool,bool,ostr,ostr,i32 |
| slots_gdp_values | 0 | 59 | inferred | str,i32,i32,f32 |
| slots_templates_models | 0 | 28 | inferred | str,str,str |
| small_vegetation_climates_jct | 0 | 95 | inferred | str,str |
| special_edition_enums | 0 | 4 | inferred | str,i32 |
| stances | 0 | 5 | inferred | str,str |
| state_gift_values | 0 | 3 | inferred | str,i32 |
| subtitles | 0 | 113 | inferred | str,str |
| taxes_classes | 0 | 2 | inferred | str |
| taxes_effects_jct | 0 | 55 | inferred | str,str,f32 |
| taxes_keys | 0 | 10 | inferred | str,str,str |
| taxes_levels | 0 | 5 | inferred | str,i32 |
| technologies | 1 | 68 | inferred | str,str,i32,i32,str,i32,i32,i32,i32,bool,bool,str,i32 |
| technology_effects_junction | 0 | 106 | inferred | str,str,f32 |
| technology_faction_junctions | 0 | 3212 | inferred | str,str |
| technology_required_building_levels_junctions | 0 | 0 | inferred |  |
| technology_required_technology_junctions | 0 | 19 | inferred | str,str |
| technology_threads | 0 | 5 | inferred | str |
| terrain_tilesets | 0 | 2 | inferred | str |
| town_wealth_growth_factors | 0 | 10 | inferred | str,str,str |
| trade_details | 0 | 3 | inferred | str,str |
| trade_node_groups | 0 | 8 | inferred | str |
| trade_nodes | 1 | 24 | inferred | str,str,i32,f32,f32,str |
| trade_theatre_commodities | 0 | 0 | inferred |  |
| trait_ability_effects | 0 | 0 | inferred |  |
| trait_attribute_effects | 0 | 157 | inferred | str,str,i32 |
| trait_attribute_situation_effects | 0 | 0 | inferred |  |
| trait_categories | 0 | 12 | inferred | str,ostr |
| trait_info | 0 | 163 | inferred | str,str |
| trait_level_effects | 0 | 564 | inferred | str,str,f32 |
| trait_to_antitraits | 0 | 93 | inferred | str,str |
| trait_to_excluded_cultures | 0 | 0 | inferred |  |
| trait_to_included_agents | 0 | 179 | inferred | str,str |
| trait_triggers | 0 | 166 | inferred | str,str,str |
| trees | 0 | 185 | inferred | str,bool,bool,bool,bool,bool,bool,bool |
| trees_climates_jct | 0 | 183 | inferred | str,str,ostr,bool |
| trigger_effects | 0 | 164 | inferred | str,str,str,i32,i32 |
| trigger_event_to_excluded_agent_types | 0 | 0 | inferred |  |
| trigger_events | 0 | 167 | inferred | str |
| uniform_to_faction_colours | 0 | 1026 | inferred | str,str,i32,i32,i32,i32,i32,i32,i32,i32,i32 |
| uniforms | 0 | 1120 | inferred | str,str,str,str |
| unit_abilities | 0 | 26 | inferred | str,bool,ostr,bool |
| unit_category | 0 | 12 | inferred | str |
| unit_class | 0 | 45 | inferred | str,str |
| unit_class_to_population_class_priorities | 0 | 22 | inferred | str,i32,i32,i32 |
| unit_class_to_unit_ability_junctions | 0 | 7 | inferred | str,str |
| unit_experience_thresholds | 0 | 18 | inferred | str,i32 |
| unit_info_card_abilities_strings | 0 | 60 | inferred | str |
| unit_movement_modifiers | 0 | 28 | inferred | str,f32,f32,f32,f32 |
| unit_regiment_names | 0 | 957 | inferred | str,str,str,i32 |
| unit_required_technology_junctions | 0 | 46 | inferred | str,str |
| unit_special_abilities | 0 | 4 | inferred | str,f32,f32,i32 |
| unit_special_ability_types | 0 | 22 | FAIL | str (file truncated) |
| unit_stats_land | 5 | 328 | exe | str,i32,i32,i32,str,ostr,ostr,str,str,str,str,i32,str,i32,ostr,ostr,ostr,str,str,ostr,ostr,ostr,ostr,bool,ostr,ostr,i32,i32,str,str,ostr,i32,ostr,str,i32,i32,i32,i32,str,str,str,str,i32,i32,f32,f32,f32,f32,f32,f32,f32,i32,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,f32,f32,f32,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,ostr,ostr,ostr,bool |
| unit_stats_land_experience_bonuses | 0 | 10 | inferred | str,i32,i32,i32,i32,i32,i32,i32,f32 |
| unit_stats_naval | 2 | 114 | inferred | str,i32,i32,i32,ostr,ostr,ostr,ostr,ostr,ostr,ostr,ostr,str,str,ostr,ostr,ostr,i32,i32,i32,i32,i32,str,f32,f32,f32,i32,f32,i32,i32,i32,i32,f32,f32,f32,i32,i32,i32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,f32,i32,i32,bool,f32,i32,i32,f32,i32,i32,f32,i32,i32,f32,i32,i32,f32,i32,i32,f32,i32,i32,f32,i32,i32,f32,i32,i32,f32,i32,i32,f32,ostr,i32,i32,bool,bool,bool,bool,i32,i32,bool,bool,bool,bool,i32,i32,bool,bool,bool,bool,i32,i32,i32,i32,bool,f32,ostr,i32,i32,bool,bool,bool,bool,i32,i32,i32,i32,bool,f32,bool,bool,bool,str,i32,str |
| unit_stats_naval_crew | 0 | 6 | inferred | str,i32,i32,i32,i32,str,i32,bool,str,str,str,str,str,i32,f32,i32 |
| unit_stats_naval_crew_to_factions | 0 | 78 | inferred | str,str,str,str,str,str,str,str |
| unit_stats_naval_experience_bonuses | 0 | 10 | inferred | str,i32,i32,i32,i32,i32,f32 |
| unit_to_unit_abilities_junctions | 0 | 838 | inferred | str,str |
| units | 4 | 442 | exe | str,str,str,str,i32,i32,i32,i32,i32,i32,ostr,str,str,str,ostr,i32,str,bool,bool,bool,i32,bool,i32,ostr,bool |
| units_to_exclusive_faction_permissions | 0 | 1012 | inferred | str,str,bool |
| units_to_gov_type_permissions | 0 | 51 | inferred | str,str,str,str |
| units_to_gov_types_conversion_jcts | 0 | 0 | inferred |  |
| units_to_groupings_military_permissions | 0 | 207 | inferred | str,str |
| units_to_special_editions_juncs | 0 | 0 | inferred |  |
| unrest_cause_to_demands | 0 | 48 | inferred | str,str,str,str |
| videos | 1 | 74 | inferred | str,str,i32 |
| videos_subtitles_junctions | 0 | 113 | inferred | str,i32,i32,str |
| warscape_animated | 0 | 980 | inferred | str,str,str |
| warscape_animated_lod | 0 | 1307 | inferred | str,str,f32,str |
| warscape_equipment_items | 0 | 57 | inferred | str,str |
| warscape_equipment_themes | 0 | 61 | inferred | str,ostr,ostr,bool,ostr,ostr |
| warscape_naval_lod | 0 | 26 | inferred | str,str,i32,str |
| warscape_rigid | 0 | 2238 | inferred | str,str,str |
| warscape_rigid_lod | 0 | 4244 | inferred | str,str,str,str |
| warscape_rigid_lod_range | 0 | 15 | inferred | str,f32 |
| warscape_trees | 0 | 366 | inferred | str,str,str |
| warscape_underlay_textures | 0 | 17 | inferred | str,str,str,i32 |
| wind_levels | 0 | 10 | inferred | str,str,f32,f32,i32 |

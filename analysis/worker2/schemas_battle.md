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

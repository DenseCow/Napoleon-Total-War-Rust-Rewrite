# Battle effects (BACKLOG §2 "Battle effects")

Worker S2, §2. This file covers the **visual battle effects**: musket/cannon smoke, muzzle flashes,
dust, projectiles with visible flight, explosions, impacts, decals, and the battlefield markers.

**It is NOT** the campaign *technology* effects store (`ntw_sim::campaign::effects`,
`analysis/fidelity/EFFECTS_FIDELITY.md`) — that is a different thing and is not touched here.

Sibling file: `analysis/graphics/SHADERS.md` (the 136 shipped `fx\` shaders, the run-time macro set at
`0x011A7F10` and the shared scene parameters at `0x01121D30`). Do not edit it; it is the shaders slot.

Tags: **CONFIRMED** / **INFERRED** / **UNKNOWN** / **BLOCKED**; stand-ins PLACEHOLDER / PROVISIONAL.
Every CONFIRMED item below names the kept evidence (an install test in `crates/ntw_formats/tests/`
or `crates/ntw_data/tests/`, a shipped data file, or a unit test name).

## Where I am / what's next

- DONE §1 **muzzle flashes, musket smoke, cannon smoke, dust**, all driven by the real XML
  (`ntw_formats::effects`, `crates/napoleon/src/battle/fx.rs`, `fx_draw.rs`).
- DONE §1b **impact effects** (blood on a man, the air burst of a shell, the ground scorch of a
  round shot), played from the shipped effect group names.
- DONE round 2 **the two PROVISIONAL lookups are now the shipped tables**: the firing group is
  `projectiles` column 31 and the air burst / ground scorch are the shot's own
  `projectiles_explosions` row (§6).
- DONE round 3 **the trail bridge is closed** (§7): `projectiles` column 32 is the trail's *effect
  group* and **column 6** (`trail_texture`) is the foreign key into `projectile_trails`. The
  thread round 2 called a dead end was a missing column index, not a missing link.
- CLOSED, negative, round 3 §3 **the muzzle attachment point is not in the shipped data** (§5 row 12),
  by three exhaustive scans. `MUZZLE_HEIGHT` stays PROVISIONAL with a named target.
- DONE round 3 §3 **sprite facing** (§8): 22 of the 283 land-battle emitters are *not*
  camera-facing (13 `BILLBOARD`, 8 `LOCAL_Y_AXIS`, 1 `WORLD_Y_AXIS`; CONFIRMED,
  `every_sprite_facing_mode_is_a_named_one`) and the reader was collapsing all three into `Other`.
  The 9 `*_Y_AXIS` ones (distortion sheets, ripples, `shockwave_large`) are now drawn upright
  (INFERRED from the names); the `BILLBOARD` earth and debris stay camera-facing (§9).
  `VELOCITY_FACING` is a clean negative over all 435 emitters.
- CLOSED, negative §3 **decals / craters**: no shipped effect file names `decal.fx` and there is no
  `RENDER_METHOD_DECAL` (§5 row 18). The textures ship; nothing references them.
- CLOSED, negative, round 3 §3 **per-emitter particle caps**: no group's single release exceeds its
  own `max_num_quads`, and `max_effects` is the constant 50 on all 283 emitters (§5 row 19). So the
  one global cap is all the data supports, and nothing was added.
- CLOSED, negative, round 4 §3 **`BILLBOARD` is indistinguishable from `CAMERA_FACING`** (§9), from
  the shipped shader's own HLSL rather than from the attribute names.
- DONE round 4 recovery §10 **two mutation probes were left switched on**, so the data column was
  being discarded in `fire_group` and the upright basis discarded in `quad_mesh`. Both are fixed and
  both regressions were introduced this round (§10.1, §10.2).
- **Round 5 §11: the air burst's SIZE — PROVISIONAL stand-in.** CONFIRMED: `AirExplosion_sml`,
  `_med` and `_lrg` are **byte-identical groups** in the shipped file, so a group name cannot size a
  burst. We now scale the sprites by the shot's own `projectiles_explosions` fourth number (2..25 m)
  over 10 m. That the number is a radius is INFERRED; that the original sizes its *sprites* by it
  (rather than, say, only its gameplay blast) is a guess, so the scaling is PROVISIONAL (§14).
- **DONE round 5 §12: `NAPOLEON_FX_CHECK` — the queued checks answered with numbers.** A new
  `battle::fx_probe` measures an effect group headlessly and reports a verdict per claim with the
  measurement behind it, so a check line becomes "the numbers say PASS" instead of a judgement about
  pixels. Also `NAPOLEON_FX_SHOT_AT=<battle seconds>`, which waits for the lines to close.
- PARKED round 5 §5.1 `MUZZLE_HEIGHT`: explicitly **for the debugger**. Time-boxed, no further data
  or Ghidra step attempted.
- NEXT §2 projectiles with visible flight (a 0-A battle-rule question — reported, not invented),
  §4 battlefield markers (selection rings, order markers, destination ghosts).

---

## 1. The effects database: `effects\landbattle.xml` (CONFIRMED)

**The whole particle system is data-driven from three files in `data.pack`, not from code.** Nothing
here is in the exe; the exe only parses these XML files (root element `EFFECTS_MANAGER`).

| pack path | bytes | what it is |
|---|---|---|
| `effects\landbattle.xml` | 1,938,509 | 283 `SCRIPTED_EFFECT_INFO` (single particle emitters) + 152 `SCRIPTED_EFFECT_GROUP` (named composites of emitters) |
| `effects\navalbattle.xml` | 829,306 | 133 emitters + 57 groups, same shape |
| `effects\campaignmap.xml` | 130,667 | 19 emitters + 16 groups, same shape |
| `effects\unit_dust_parameters.txt` | 398 | `entity frequency` table for foot/horse/artillery dust |

**Evidence:** `cargo test -p ntw_formats --test effects_install -- --ignored` —
`land_battle_effects_parse` (283 / 152 / 514 group entries, all resolving) and
`all_effect_files_and_dust_parameters_read` (283 / 133 / 19 emitters, 152 / 57 / 16 groups, the
15 dust rows). Both pass on the user's install.

### 1.1 `SCRIPTED_EFFECT_INFO` — one emitter (CONFIRMED attribute names)

283 of them. Each has three child blocks.

Attributes (all CONFIRMED names, read straight from the file):
`num_particles_per_point`, `gravity_range`, `min_lod_range`, `max_lod_range`, `wind_range`,
`thickness`, `lighting`, `adjust_direction_by_offset_position`, `clamp_sea_level`, `quality_level`,
`ribbon_segment_rate`, `ribbon_tiling1`, `ribbon_tiling2`, `ribbon_scrolling1`, `ribbon_scrolling2`,
`ref_count`.

- `SCRIPTED_EFFECT_EMISSION_CONTROL`: `emission_type` (`EMISSION_TYPE_POINT` 277×,
  `RADIAL` 5×, `SPHERICAL` 1×), `association_behaviour`, `loop_delay_range_specified`,
  `loop_delay_range`, `emission_time_range_seconds`, plus
  `RELEASE_INFO` (`release_type`, `release_interval_range`, `release_position_variation_range`) and
  13 `EMISSION_MODIFIER_*` values (start/end RGB, opacity, lifetime, velocity, initial scale,
  two target scales, initial rotation, rotation speed), each with a `type`
  and a `value`.
- `SCRIPTED_EFFECT_PARTICLE_ATTRIBUTE_RANGES`: `ribbon_life_range`, `radial_blur_strength`,
  `radial_blur_min_distance`, `radial_blur_max_distance`, `life_range`, then
  `MOVEMENT_INFO` (`velocity_range`, `dampening_range`, `dir_range_XYZ`, `explicit_direction_specified`,
  `conic_range_fan`, `conic_range_depth`), `ANIMATION_INFO` (`start_frame_variation`,
  `animation_length`, `loop_count_range`, `total_frames`, `cell_width`, `cell_height`),
  `COLOUR_INFO` (start/end RGB + `colour_range_a`, with `*_linked_channels`),
  `FADE_INFO` (`fadein_range_life_unary`, `fadeout_range_life_unary`,
  `fadeout_ribbon_range_life_unary`), `SCALE_INFO` (`initial_scale_range_metres`,
  `primary_scale_life_unary_range`, `primary_target_scale_range_metres`,
  `secondary_scale_life_unary_range`, `secondary_target_scale_range_metres`,
  `secondary_scale_ribbon_life_unary_range_end`, `scales_mesh_vector`, `scales_tracked_dir`,
  `use_x_variation_only`), `ROTATION_INFO` (`rotation_range`, `rotations_per_second_range`,
  `rotation_mesh_vector`, `rotation_forced_dir`).
- `SCRIPTED_EFFECT_RENDERING_VARS`: `texture_1..texture_8` (pack paths), `fx`
  (`particle.fx`, `particle2.fx`, `particle_distortion.fx`, `explosion_particle.fx`,
  `projectiletrail.fx`, `decal.fx`, `ribbon.fx`, `spline.fx` — the eight that are ours),
  `render_method` (`RENDER_METHOD_ALPHA`, `RENDER_METHOD_DISTORTION`, additive, ...),
  `align_to_velocity`, `sprite_facing_mode` (`CAMERA_FACING`, ...),
  `max_effects`, `max_num_quads`.

Every numeric value is written as one of three literal forms (CONFIRMED):
- `variance(base,var)` — a value with a symmetric ±var spread;
- `vector(x,y)` and `vector(variance(...),variance(...))` — 2- or 3-component;
- a bare number.

**The emission modifiers (load-bearing, CONFIRMED).** 283 emitters × 14 modifier elements = 3,962
rows: 3,906 are `EMISSION_MODIFIER_TYPE_NONE`, 37 `ADD`, 19 `MUL`, and the `NONE` rows carry the
neutral value 0 (207 of the 283 velocity, 277 of the 283 lifetime, 265 of the 283 opacity). A `NONE`
modifier with value 0 therefore means **"leave this attribute alone"**, not "scale it by 0": read as
a multiplier it would give every particle a life of zero seconds and the whole effect system would
emit nothing. The unit tests `a_none_modifier_leaves_its_attribute_alone` and
`modifier_values_are_vectors_or_a_single_number` pin this down, and
`land_battle_effects_parse` re-counts 37 / 19 / 3,906 on the install.

### 1.2 `SCRIPTED_EFFECT_GROUP` — a named composite (CONFIRMED)

152 of them: `EFFECT_GROUP_AMBIENT_SETTINGS` (`spawn_interval`, `cell_size`, `cell_count`,
`wind_offset`) plus 1..N `SCRIPTED_EFFECT_GROUP_ENTRY name= effect=`. **The group is the unit the
engine plays** — the DB refers to groups, never to single emitters.

The 104 names in `db\particle_effects_tables\particle_effects` are a **single string column**
(104 rows, table version 0) and they are group names (CONFIRMED: `AirExplosion_lrg`, `blood_gen`,
`building_destroy_effect_med1`, ...; all 104 resolve to a group of `landbattle.xml` in the install
test `particle_effects_table_lists_land_battle_groups`, a clean check over the whole column). 152
groups exist in the XML, 104 are listed in the DB.

### 1.3 The firing groups (CONFIRMED, verbatim, re-checked on the install)

```text
MusketFire = musket_1 + musket_2 + musket_3 + flash_dir_tiny + Uber_smoke_white
rifleFire  = musket_1 + musket_2               + flash_dir_tiny
pistolFire = pistol_1 + pistol_2               + flash_dir_tiny
LandGunFire= landgun_med_1 + landgun_med_3 + landgun_fastsmoke + flash_dir_sml
             + explodesmall_glow + cannon_fire_distortion + sparks_cannon
             + cannon_fire_particle_ring_small + landgun_med_2_whispy
             + Uber_smoke_white + landgun_ground_smoke
CannonFire = landgun_med_1 + landgun_med_3 + landgun_fastsmoke + flash_dir_sml
             + explodesmall_glow + cannon_fire_distortion + cannon_fire_anim_smoke
             + landgun_med_2_whispy + Uber_smoke_sea
LandGunFire_small / _large differ only in flash_dir_tiny/med and landgun_med_3_small / sparks_lrg
```

### 1.4 `effects\unit_dust_parameters.txt` (CONFIRMED, whole file, 398 bytes)

Tab-separated `// type <TAB><TAB><TAB><TAB> entity frequency`, then:

```text
infantry_walking 0.05   infantry_running 0.10   infantry_charging 0.15   infantry_melee 0.10
cavalry_walking  0.05   cavalry_running   0.15   cavalry_charging 0.30   cavalry_melee   0.10
elephants_walking 0.15  elephants_running 0.30   elephants_charging 0.45 elephants_melee  0.10
artillery_walking 0.1   artillery_running 0.2    artillery_melee    0.2
```

---

## 2. The projectile effect tables (what plays when a shot lands)

Three shipped DB tables name the effect **groups** a projectile plays. Reading them is the way to
get explosions, impacts, blood and trails onto the battlefield; they are all `db\` tables next to
the ones the sim already reads.

| pack path | rows | what it names |
|---|---|---|
| `db\projectiles_explosions_tables\projectiles_explosions` | v1, 35 | per projectile key: a fuse class, a shockwave class, a **fragment projectile** (a `projectiles` key, not a group), the air-burst group and — on 10 rows — the ground scorch. Read without a schema; see §6.2 |
| `db\projectile_impacts_tables\projectile_impacts` | v0, 8 | per **ball class** (`carcass`, `default_ball`, `musket_ball`, `naval_grape`, `incendiary`, `large_ball`, `quicklime`, `test`): 15-18 impact groups, one per surface (`Musket_impact_hard/soft`, `Cannon_Groundimpact_gen_sml/med`, `CannonImpactWater_med`, `CannonImpactShip`, `Watersplash_sml`, `blood_spray`, `blood_gen`, `building_impact`, `impact_tree`, `impact_sails`) then a size class. **Which column is which surface is UNKNOWN** |
| `db\projectile_trails_tables\projectile_trails` | v0, 5 | **schema decoded**: `ssffffffffff` = key, blend mode, then 10 floats. Rows: `alpha`, `alpha_bullet`, `alpha_shrapnel`, `e3_rocket`, `none` (all-zero = no trail) |

**The link into them is `projectiles` itself (CONFIRMED):** `projectiles.explosion` (column 5) names
a `projectiles_explosions` key, and `projectiles`'s column 33 (`impact_ball`) names a
`projectile_impacts` key. 18 of the 35 explosion rows are named by a projectile; the other 17 are
in the table for another caller.

**CONFIRMED:** every one of those names is a `SCRIPTED_EFFECT_GROUP` of `effects\landbattle.xml`
(`AirExplosion_lrg`, `AirExplosion_med`, `AirExplosion_sml`, `CannonImpactShip`,
`CannonImpactWater_med`, `Cannon_Groundimpact_explosive`, `Cannon_Groundimpact_gen_med`,
`Cannon_Groundimpact_gen_sml`, `Musket_impact_hard`, `Musket_impact_soft`, `ShipExplosion`,
`Watersplash_sml`, `blood_gen`, `blood_spray`, `building_impact`, `carcass_hit`, `explode_cone`,
`explode_grenade`, `explode_quicklime_med`, `impact_sails`, `impact_tree`, `quicklime_hit`).
**Evidence:** `cargo test -p ntw_formats --test effects_install --test effects_data -- --ignored`.
`projectile_effect_names_are_land_battle_groups` reads all three tables' UTF-16 strings straight out
of the shipped bytes and checks each against the parsed library;
`ntw_data --test effects_data` reads the rows through the real readers and checks every group they
name against `landbattle.xml`.

**UNKNOWN:** the *numeric* columns of both tables, and the surface order of `projectile_impacts`' group
columns. Three rounds of exact-fit schema search did not land a full column layout, so
`ntw_formats::projectile_fx` reads the string columns structurally and keeps the numbers as raw
values rather than guessing names for them. That was enough for the effects: **every group a shot
plays is now read from the data** (§6), so the round-1 size stand-ins are only fallbacks.

---

## 3. The shaders the emitters name, and the scene parameters

From `SHADERS.md` (CONFIRMED there, kept here because it is the list our emitters draw with):

| category | shipped `fx\` files |
|---|---|
| particles and props | `particle.fx`, `particle2.fx`, `particle_distortion.fx`, `explosion_particle.fx`, `projectiletrail.fx`, `ribbon.fx`, `decal.fx`, `spline.fx` |
| battlefield markers / UI | `sprite.fx`, `grid*.fx`, `strengthbar.fx`, `formationlayouticons.fx` |
| shared includes | `lighting.fx_fragment`, `lighting_brdf.fx_fragment`, `fog.fx_fragment`, `util.fx_fragment`, `screeneffects.fx_fragment` |

The engine feeds the same named scene parameters to every shader (registration at `0x01121D30`):

| parameter | purpose |
|---|---|
| `light_direction`, `light_colour` | sun direction and RGB/intensity, from the battle `.environment` `LIGHTING` |
| `g_specular_scale`, `g_ambient_scale` | specular and ambient strength (`LIGHTING`) |
| `ambient_cube_lr/fb/tb` | six-face ambient light (`LIGHTING`) |
| `g_fog_distance_start`, `g_fog_distance_strength`, `g_fog_*` | distance and height fog (`FOG`) |
| `g_volume_fog_colour`, `g_volume_fog_density` | volumetric fog (`FOG`) |
| `g_hdr_bloom`, `g_hdr_cutoff`, `g_hdr_exposure` | HDR and bloom (`LIGHTING`) |
| `g_gamma_output` | gamma curve — source **UNKNOWN** |
| `g_brightness` | display brightness — source **INFERRED** (feeds the final `colour_matrix`) |

Run-time macro set `0x011A7F10`: `GAMMA_VALUE`, `MIN/MAG/MIP_FILTER`, `BORDER_ADDRESS_SUPPORT`,
GPU flags, `MRT_SUPPORT`, `LOW_QUALITY_SHADERS`. We do not compile D3DX effects; `particle.wgsl` is
our own shader written from what `particle.fx` does.

---

## 4. What we ship today

| piece | where | notes |
|---|---|---|
| the effect database reader | `ntw_formats::effects` (`crates/ntw_formats/src/effects.rs`) | every attribute above, with the `NONE` modifier rule |
| the dust frequency table | `ntw_formats::effects::DustParameters` | `effects\unit_dust_parameters.txt` |
| the projectile effect tables | `ntw_formats::projectile_fx` (`crates/ntw_formats/src/projectile_fx.rs`) | `projectiles_explosions`, `projectile_impacts` and `projectile_trails`, cut structurally with no schema; loaded into `GameDatabase::projectile_explosions` / `projectile_impacts` / `projectile_trails` |
| a shot's trail | `GameDatabase::trail_group` + `GameDatabase::trail_row` | **CONFIRMED**: the group is `projectiles` column 32, the trail row is column 6 (§7) |
| the particle world | `crates/napoleon/src/battle/fx.rs` | deterministic (`ntw_sim::rng::CaRng` off the battle seed, advanced only by the 0.1 s model tick); gravity, wind, dampening, fade, size ramp, colour ramp, sprite frame, `advance` |
| which group a muzzle plays | `fx::fire_group(db, projectile)` | **CONFIRMED**: `projectiles` column 31. `fx::fire_group_by_name` is the PROVISIONAL fallback |
| which group a boot plays | `fx::dust_puff`, `fx::dust_entity_name` | the frequency is CONFIRMED, the group PROVISIONAL |
| which groups a shot plays on impact | `fx::impact::air_burst`, `fx::impact::ground_scorch`, `fx::BLOOD_GROUP` | the first two **CONFIRMED** from `projectiles_explosions`; the blood group is INFERRED |
| the draw layer | `crates/napoleon/src/battle/fx_draw.rs` + `particle.wgsl` | one mesh entity per (effect texture, blend) pair; quads rebuilt each frame; each quad is turned by its emitter's shipped `sprite_facing_mode` (§8) |
| impact effects | `fx_draw::fire_muzzle` → `impact_at_target` | blood on a man, air burst, ground scorch, all from the data |

Harness (battle mode only):
- `NAPOLEON_FX_LOG=<n>` — log the live/released/dropped particle counts every second of battle
  time and quit once `n` particles have been released.
- `NAPOLEON_FX_SHOT=<file.png>` — save a screenshot at the first volley in the air (≥ 40 live
  particles, some ≥ 0.25 s old) and quit 1.5 s later.

---

## 5. Open / unresolved

Rows 1, 2, 13 and 16 were closed in round 2 and are kept below with their evidence, because a
negative result is worth as much as a positive one.

| # | Item | State |
|---|---|---|
| 1 | How `gun_type_to_projectiles.muzzle_flash` (`cannon_12_pounder_muzzle_flash`, 22 distinct names over 153 rows) reaches a group | **CLOSED, negative.** Two whole-pack scans say the name is exe-side, not data: none of the 22 is a group, group entry or emitter name in any effect file, and a scan of **all 86,977 pack entries** finds `muzzle_flash` in exactly two files — `db\gun_type_to_projectiles` itself and one unrelated `warscape_rigid` node of that name. It is in no gun model and no `.anim` file either. Evidence: `ntw_formats/tests/effects_install.rs::projectile_effect_names_are_land_battle_groups` (the scan is the last half of the test). **What the column is** is still INFERRED — the name is a per-gun *attachment point* (`cannon_8_pounder` reuses `cannon_9_pounder_muzzle_flash`, all howitzers share `howitzer_12_pounder_muzzle_flash`), which is why the flash *effect* lives in `projectiles` column 31 instead (§6.1) |
| 2 | `projectiles_explosions` column layout | **CLOSED for the string columns**, and that is all the effects need. Every row's strings are `key`, fuse class, shockwave class, fragment **projectile**, air-burst group, and on 10 of 35 rows a ground-scorch group; the 0 / 0 / 17 byte-gap run is unique to a row, so `ntw_formats::projectile_fx::ExplosionTable` cuts the table with no schema at all and checks the count against the header. The **numeric** columns are still unnamed (below) |
| 3 | `projectile_impacts` column layout, and which group column is the man / hard ground / soft ground / water | **UNKNOWN.** The rows are now read (each ends with a size class word, `small`/`medium`/`large`, which cuts all 8), but a row holds 15-18 group columns and their order is not decoded. `ImpactTable` returns them in file order |
| 4 | Which projectile row reaches which `projectile_impacts` row | **CLOSED.** `projectiles` column 33 is the impact **ball class** and every set value is one of that table's 8 keys (`default_ball` 85 rows, `musket_ball` 48, `naval_grape` 9). Renamed from the guessed `impact_effect` |
| 5 | `projectile_trails` numeric meaning | **PARTLY CLOSED, round 3 (§7).** The table reads whole with no schema guess (`ntw_formats::projectile_fx::TrailTable`, 5 rows, `ssffffffffff`). The second string is the **blend mode** (CONFIRMED: `alpha` / `add` / `none`) and floats **4-7 are an 8-bit RGBA quadruple** (CONFIRMED: the only four values in the table sharing an 8-bit range, the only run of three equal values — white at alpha 128 on three rows, grey at alpha 100 on the rocket, all zero on `none`; the order within the quadruple is INFERRED). **The other six floats stay UNKNOWN**, on a negative: one table row serves projectiles spanning a 62× muzzle-velocity range (`alpha`, 89 rows, 4..250 m/s) and a 15× effective-range range (50..750 m), so no float in a row can be a per-shot duration or length, and float 10 is the constant 50 on all four live rows. Evidence: `ntw_formats/tests/effects_install.rs::the_projectile_trails_table_reads_whole`, `ntw_data/tests/effects_data.rs::the_trail_group_and_the_trail_table_are_joined_by_column_six` |
| 5a | How a `projectiles.trail` value reaches the `projectile_trails` table — **the one open naming thread from rounds 1-2** | **CLOSED, round 3 (§7).** It was never meant to: column 32 names the trail's **effect group** (all 7 values are `landbattle.xml` groups) and **column 6** (`trail_texture`) is the foreign key — all 5 distinct values over all 144 `projectiles` rows are that table's 5 keys, a bijection |
| 6 | `EMISSION_MODIFIER_TYPE_*` values other than `NONE` (a clean negative over the 283 shipped rows) | UNKNOWN |
| 7 | How the engine seeds its particle RNG (replays care) | UNKNOWN |
| 8 | Scene lighting applied to particles (`lighting` attribute × the `.environment` light). We multiply one sun colour into one uniform for the whole battle | OPEN |
| 9 | `conic_range_fan` / `conic_range_depth` as a cone on the emitter's own `dir_range_XYZ` axis | PROVISIONAL |
| 10 | The sprite-sheet grid (`total_frames` → squarest grid, left to right) | INFERRED |
| 11 | `RENDER_METHOD_DISTORTION` (`particle_distortion.fx` bends the scene through a normal map) — drawn flat until the distortion pass exists | PROVISIONAL |
| 12 | Where a unit's muzzle is (we use half a unit block in front at 1.35 m; the original reads a node out of the firing animation model) | **PROVISIONAL, and round 3 showed it has to stay that way from data (§5.1).** Not a missing reader: the attachment point is not in any shipped file. Three exhaustive negatives, all kept in `ntw_formats/tests/effects_install.rs::no_shipped_gun_model_carries_a_muzzle_attachment_point` |
| 13 | `MUZZLE_HEIGHT` 1.35 m; the gun-size split at 9 pounds | **CLOSED for the split, kept for the height.** There is no calibre rule at all: `projectiles` column 31 names the group per row (`cannon_9_pounder_shot` → `LandGunFire_small`, `cannon_12_pounder_shot` → `LandGunFire`, `fort_24_pounder_shot` → `LandGunFire_large`), and `fx::fire_group` reads it. The 9-pound rule survives only as the fallback for the 24 rows with no group, which are arrows, grenades and fragments |
| 14 | Shot flight time (our model resolves a volley at once, so no visible flight) | **UNKNOWN, and a 0-A question.** Whether the original inserts a delay between fire and impact is a *battle-rule* question, so it belongs to 0-A, not to this slice. Reported, not implemented |
| 15 | Which of the `projectile_impacts` group columns a musket-on-a-man uses (we play `blood_gen`) | INFERRED — unchanged, see row 3 |
| 16 | Air-burst size by calibre | **CLOSED.** The size is `projectiles.explosion`'s own row in `projectiles_explosions`, and it is **authored per row**, not computed: `shell_12lb` → `AirExplosion_med` but `shrapnel_12lb` → `AirExplosion_sml`, and `shell_percussive_12lb` (a 12-pounder) → `AirExplosion_sml`. No calibre rule reproduces all three, so `fx::impact::air_burst` reads the row and the 6/12-pound thresholds are only the fallback. Evidence: `ntw_data/tests/effects_data.rs::explosion_rows_name_the_effect_groups_they_play` |
| 17 | The names of `projectiles_explosions`' numeric columns | UNKNOWN. The fourth value of each row's first block rises with the pound count over the shrapnel series (2, 3, 4, 6, 8, 10, 12, 14 m for 3-64 lb), so it is **INFERRED** to be a burst radius in metres — but it does **not** pick the `_sml`/`_med`/`_lrg` group (`shell_percussive_12lb` has 20 and plays `_sml`), so the group choice stays "authored per row" |
| 18 | Ground craters and decals | **CLOSED, negative.** `fx\decal.fx` ships and the decal textures do (`impact_decal`, `impact_decal2`, `impact_decal_cannonball_finish`, `impact_bounce_decal`, each with a `_normal` map), but **no shipped effect file names `decal.fx`**: `landbattle.xml` uses exactly three shaders (`particle.fx` 254, `particle_distortion.fx` 19, `ribbon.fx` 10) and three render methods (`ALPHA` 203, `ADDITIVE` 61, `DISTORTION` 19) — there is no `RENDER_METHOD_DECAL` — and the string `decal` appears in no `db\` table at all. So the decals are not driven by the particle system; they are exe-side or a different subsystem. Evidence: `cargo run -p ntw_data --example fx_table_probe -- attrs effects\landbattle.xml render_method` and the same for `fx` |
| 19 | A per-emitter particle cap, from `max_effects` / `max_num_quads` | **CLOSED, negative (nothing to add).** `max_effects` is the **constant 50 on all 283** land-battle emitters, so it is not tuned per effect and discriminates nothing. `max_num_quads` has nine values (250..20000, 157 emitters at 1000) but **0 of the 152 groups release more particles in one go than the smallest budget among their own emitters** — the firing groups release 14 (`pistolFire`) to 162 (`LandGunFire_large`) against a budget of 1000. So there is no data-supported per-emitter cap to enforce, and the one global 24,000 cap stays the only one. Evidence: `cargo run -p ntw_data --example fx_table_probe -- budgets` |
| 20 | `VELOCITY_FACING` for the firing and dust emitters | **CLOSED, negative, and wider than the firing groups.** The string is a **clean negative over all 435 emitters in the three effect files**: the shipped values are `CAMERA_FACING` 401, `BILLBOARD` 18, `LOCAL_Y_AXIS` 14, `WORLD_Y_AXIS` 2, and `align_to_velocity` is `false` on every one. Nothing should draw with it. Evidence: `ntw_formats/tests/effects_install.rs::every_sprite_facing_mode_is_a_named_one` |

### 5.1 The muzzle attachment point: three negatives (round 3)

The brief was to find the cannon models and read the muzzle attachment off them, copying 0-D's
method on `rigid_equip_euro_flagpole01` ("attachment 1 of 134" in
`unitmodels\euro_equipment.variant_weighted_mesh`). **The method does not transfer, and the reason is
worth keeping** because it will stop anyone re-running it:

0-D's attachments are **swappable equipment meshes carried by a soldier model** — a flagpole, a
musket, a sword, each a rigid mesh the soldier can be given. A **gun has none**: it is one
`.animatable_rigid_model` per LOD with no soldier and no equipment list. So there is no attachment
array to read an index out of.

Three scans, all kept in `no_shipped_gun_model_carries_a_muzzle_attachment_point`:

1. **Whole pack.** `muzzle` is a substring of only **5 of the 86,977** entries, and none is a gun
   model: `db\gun_type_to_projectiles_tables\gun_type_to_projectiles`,
   `db\warscape_rigid_lod_tables\warscape_rigid_lod`, `db\warscape_rigid_tables\warscape_rigid`,
   `rigidmodels\projectile\cannon_muzzle01.rigid_model` (the **shot's own** impact model) and
   `text\localisation.loc`.
2. **The gun models.** All **158** files under `enginemodels\` matching `cannon` / `howitzer` /
   `mortar` / `carronade` / `rocket` hold **8,415 strings** between them. Classifying every one, they
   are exactly four kinds: a texture name, a texture path, a `building_NNN` destruction clip, or one
   of the 15 named shader constants (`light_scale`, `bumpfactor`, `specpower`, ...). **Not one node
   name.** A model with a named attachment point would hold that name as a string.
3. **The gun table.** `db\models_artilleries_tables\models_artillery` has 34 rows, one per gun model,
   each followed by a 683-byte numeric block — and the block is **byte-identical across all 34
   rows** (`817c5603ae292a23`), so it is the shared field-cannon rig, not a per-gun muzzle. (The 34
   rows all point at the same `field_cannon_model.cs2.parsed`, which is why.)

So the original **computes** the muzzle; finding how is an exe-side question, not a data one.

---

## 6. Round 2: what the shipped tables settled

### 6.1 `projectiles` columns 31-33 (CONFIRMED)

`db\projectiles_tables\projectiles` was carrying three mislabelled columns. Reading what is actually
in them:

| col | was called | is | evidence |
|---|---|---|---|
| 31 | `fire_sound` | **`fire_effect`**: the `SCRIPTED_EFFECT_GROUP` the shot plays when fired | set on 120 of the 144 rows, every value a `landbattle.xml` group |
| 32 | `unknown_104` | **`trail`**: the trail the shot leaves (`shrapnel_trail`, `carcass_trail`, `congreve_rocket`, ...) | 7 distinct values over 9 rows |
| 33 | `impact_effect` | **`impact_ball`**: the `projectile_impacts` ball class | every value one of that table's 8 keys |

The fire group tally over the 144 rows (this is the **complete** set of groups a land battle can
play at a muzzle, so `fx_draw::FIRE_GROUPS` is now exactly these eleven):

```text
CannonFire 56   LandGunFire_large 9     LandGunFire_howitzer 7   fougasse_default 2
LandGunFire_canister 13                 LandGunFire_mortar 8    rifleFire 2
MusketFire 8                            LandGunFire_small 7      pistolFire 1
LandGunFire 7   (ship_explosion 1, naval)
```

`LandGunFire_congreve` was in round 1's list on the strength of its name. **No `projectiles` row
uses it**, so it is out; `fougasse_default`, which no row but two used, is in.

**CONFIRMED negative on the trail:** the `trail` values are *not* the `projectile_trails` table's
keys (`alpha`, `alpha_bullet`, `alpha_shrapnel`, `e3_rocket`, `none`). They are a naming convention
over the same idea (`*_trail`, plus `congreve_rocket` for a rocket class), and none of the seven
appears in that table — so **how** a `trail` value reaches `projectile_trails` was UNKNOWN after round
2. **Round 3 closed it; see §7.** The short version: it does not, because it was never supposed to.

> Superseded by §7. The paragraph above is kept because the negative was real and it is what pointed
> at the answer: the values were a naming convention over the *same idea*, which is what a different
> column's foreign key looks like.

### 6.2 `projectiles_explosions`, read without a schema

`ntw_formats::projectile_fx::ExplosionTable` cuts all 35 rows out of the raw bytes. A DB table has no
per-row framing, so the cut is structural: within a row the key, the fuse class and the shockwave
class are **adjacent** (0 bytes between them) and a **17-byte** numeric block follows them. That
0 / 0 / 17 run occurs exactly 35 times, which is what the header says, so the segmentation is
checked rather than assumed.

What each row turned out to hold, and the two corrections this forced:

- the **third** string column is a fragment **projectile**, not a group. All seven distinct values
  (`shell_fragment`, `carcass_fragment`, `shrapnel`, `quicklime_fragment`, `grenade_fragment`,
  `ship_explosion_fragment`, `ship_explosion_fragment_visual`) are `projectiles` row keys and **none**
  is a `landbattle.xml` group. Round 1 assumed they were groups;
- the **fifth** row group is the same on all ten rows that have one
  (`Cannon_Groundimpact_explosive`), and 25 rows have none — the carcass, grenade and quicklime
  families, which is why `fx::impact::ground_scorch` returns `Option`.

The air bursts, verbatim (this is the table, not a rule):

```text
shell_percussive_12lb -> AirExplosion_sml     shrapnel_3lb  -> AirExplosion_sml
shell_12lb            -> AirExplosion_med     shrapnel_6lb  -> AirExplosion_sml
shell_percussive_24lb -> AirExplosion_med     shrapnel_12lb -> AirExplosion_sml
shell_18lb            -> AirExplosion_med     shrapnel_9lb  -> AirExplosion_med
rocket                -> AirExplosion_med     shrapnel_18lb -> AirExplosion_med
mortar_4_shell        -> AirExplosion_med     shrapnel_24lb -> AirExplosion_med
shell_24lb            -> AirExplosion_lrg     shrapnel_32lb -> AirExplosion_lrg
shell_32lb            -> AirExplosion_lrg     shrapnel_64lb -> AirExplosion_lrg
shell_64lb            -> AirExplosion_lrg
shell_percussive_32lb -> AirExplosion_lrg     carcass_*lb   -> explode_cone
mortar_8_shell        -> AirExplosion_lrg     grenade_*     -> explode_grenade
naval_mortar_shell    -> AirExplosion_lrg     quicklime_*   -> explode_quicklime_med
rocket_naval          -> AirExplosion_lrg     ship_magazine -> ShipExplosion
```

---

## 7. Round 3: the trail bridge, closed

**A shot's trail is a pair, and the two halves live in two different columns.** Round 2 called the
bridge UNKNOWN after proving that `projectiles` column 32 held none of `projectile_trails`' five keys.
That was true and it was the wrong question: column 32 was never the foreign key.

| what | column | values | is |
|---|---|---|---|
| the trail's **effect group** — what plays | 32, `trail` | 7 over 9 rows | all 7 are `landbattle.xml` `SCRIPTED_EFFECT_GROUP`s |
| the trail's **colour and geometry** | **6, `trail_texture`** | 5 over **all 144** rows | all 5 are `projectile_trails` keys |

Column 6 was already decoded and already named — `analysis/worker2/schemas_battle.md` has had
"| 6 | o | trail texture | L | alpha_bullet / alpha |" on its column list all along. Round 2 read the
value, saw it matched `projectile_trails`, and wrote the column comment as "trail texture" without
following the link. The lesson for the next thread: **when a decoded column's values match a table
exactly, follow the foreign key before concluding the thread is dead.**

The two column sets are **disjoint**, which is why the original negative looked so clean. The nine
rows that name a trail group:

```text
carcass_fragment                   group carcass_fragment_trail          trail row alpha          blend alpha  rgba [255,255,255,128]
howitzer_experimental_carcass      group carcass_trail                   trail row alpha          blend alpha  rgba [255,255,255,128]
mortar_4_quicklime                 group quicklime_trail                 trail row alpha          blend alpha  rgba [255,255,255,128]
mortar_8_quicklime                 group quicklime_trail                 trail row alpha          blend alpha  rgba [255,255,255,128]
quicklime_fragment                 group quicklime_fragment_trail        trail row alpha          blend alpha  rgba [255,255,255,128]
rockets                            group congreve_rocket                 trail row alpha          blend alpha  rgba [255,255,255,128]
rockets_naval                      group congreve_rocket                 trail row e3_rocket      blend alpha  rgba [125,125,125,100]
shell_fragment                     group shrapnel_trail                  trail row alpha_shrapnel blend add    rgba [255,255,255,128]
ship_explosion_fragment_visual     group ship_explosion_fragment_trail   trail row e3_rocket      blend alpha  rgba [125,125,125,100]
```

Note `rockets` and `rockets_naval` share the group but not the trail row — so the two columns really
are independent, and neither one could have stood in for the other.

### 7.1 `projectile_trails` reads whole

The table is exactly regular — key, blend mode, ten floats, next key — so
`ntw_formats::projectile_fx::TrailTable` cuts it with no schema guess and checks the row count
against the header. This is the whole table:

```text
alpha          blend alpha   0.20  0.10   50   255 255 255 128   300   30   50
alpha_bullet   blend alpha   0.10  0.05   50   255 255 255 128   300   25   50
alpha_shrapnel blend add     0.05  0.05   25   255 255 255 128   300   25   50
e3_rocket      blend alpha   2.00  2.00  500   125 125 125 100  1000  100   50
none           blend none    all zero: this trail draws nothing
```

Named, with the evidence in §5 row 5:

- the second string is the **blend mode** (CONFIRMED — `alpha` / `add` / `none`, and `alpha_shrapnel`
  is the only row that is not `alpha`);
- floats **4-7 are an 8-bit RGBA quadruple** (CONFIRMED — white at alpha 128 on three rows, grey at
  alpha 100 on the rocket, all zero on `none`; they are the only four values in the table that share
  an 8-bit range and the only run of three equal values);
- the other **six floats are UNKNOWN**, on a negative rather than a shrug: one row serves many
  projectiles, and the `alpha` row alone spans **4 to 250 m/s** of muzzle velocity and **50 to 750 m**
  of effective range over its 89 rows, so no float in a row can be a per-shot duration or length.

`GameDatabase` gains `projectile_trails` plus `trail_row(projectile)` (column 6) and
`trail_group(projectile)` (column 32), so the pair is one call each.

---

## 8. Round 3: sprite facing, which was being drawn wrong

`effects.rs` mapped `CAMERA_FACING` and `VELOCITY_FACING` and put **everything else** into
`FacingMode::Other` — and `quad_mesh` never read the field at all, so **every** quad was
camera-facing. The shipped files say otherwise for 34 emitters:

| mode | land | naval | campaign | what they are |
|---|---|---|---|---|
| `CAMERA_FACING` | 261 | 121 | 19 | smoke, flash, dust |
| `BILLBOARD` | 13 | 5 | 0 | earth, debris, water sprites |
| `LOCAL_Y_AXIS` | 8 | 6 | 0 | the ground-impact distortion, the water ripples |
| `WORLD_Y_AXIS` | 1 | 1 | 0 | `shockwave_large` |

**`VELOCITY_FACING` is a clean negative**: no shipped effect file writes it, and `align_to_velocity`
is `false` on all 435 emitters. Confirmed over the whole set, not just the firing groups.

This matters because the non-camera emitters are exactly the ones the slice already plays. Every
ground-impact group carries 3-4 of them:

```text
Cannon_Groundimpact_explosive   4 of 18 entries are not camera facing
Cannon_Groundimpact_gen_sml     3 of  9
Cannon_Groundimpact_gen_med     3 of 11
Cannon_Groundimpact_gen_lrg     3 of 11
```

`Cannon_Groundimpact_explosive` — the ground scorch of an explosive shell — has `cannon_hit_smoke`
and `explosion` and `impact_cannon_earth_lrg1` as `BILLBOARD` and `ground_impact_distortion` as
`LOCAL_Y_AXIS`. All four were being tipped at the camera.

`quad_mesh` now reads the mode: `LOCAL_Y_AXIS` and `WORLD_Y_AXIS` build the quad **upright** and turn
it about the vertical only (a cylindrical billboard), which is what the name says and what keeps a
ground card visible from a low camera instead of edge-on.

Two bugs found on the way, both in code this round touched:

- the quad normal was `-normalize(eye)`, a single direction for the whole battlefield, so a quad far
  from the origin was shaded as if it were at it. It is now `normalize(eye - particle_position)`,
  with the upright modes taking the horizontal part;
- `the_quad_is_camera_facing_and_the_right_size` asserted against `positions(&mesh)` in a variable
  named `normals`, so the normal check had never actually run. It reads the normal attribute now.

---

## 9. Round 4: `BILLBOARD` is closed as a negative, from the shader's own source

Round 3 drew `BILLBOARD` like `CAMERA_FACING` and called the reading INFERRED. **It is not a guess
and not a name-reading: the shipped files leave no channel through which a difference could arrive.**

The evidence is that `fx\particle.fx` is not compiled bytecode. It is **4,246 bytes of readable
HLSL**, and it ships as such (`fx\particle2.fx` 15,592, `fx\particle_distortion.fx` 4,815,
`fx\particle_volumetric.fx_fragment` 30,907). `particle.fx` is only a list of techniques; the vertex
shader it calls, `particle_vertex_30`, is defined in the file it `#include`s:

```text
// fx\particle_volumetric.fx_fragment, lines 10-13
// The following 3 vectors describe the axis of a billboard aligned with the camera
const float3		g_camera_aligned_x_axis;
const float3		g_camera_aligned_y_axis;
const float3		g_camera_aligned_z_axis;
```

and the quad is built from two of them and nothing else (line 419):

```text
float2 xy = mul(xy_uv_offset.xy * scale, rot_mat);
/*if (g_align_to_velocity)
{
	float3 velocity_vector = normalize(velocity);
	current_pos	+= cross(g_camera_aligned_x_axis,velocity_vector) * xy.x + cross(g_camera_aligned_y_axis,velocity_vector) * xy.y;
}else*/
	current_pos	+= g_camera_aligned_x_axis * xy.x + g_camera_aligned_y_axis * xy.y;
```

Four consequences, each one a separate check in
`ntw_formats/tests/effects_install.rs::billboard_is_indistinguishable_from_camera_facing_in_the_shipped_shader`:

1. **The shader is never told the mode.** `sprite_facing`, `BILLBOARD` and `facing_mode` appear in
   **none** of the four particle shaders. So the basis is chosen **engine-side**, from the vertex
   stream, and `sprite_facing_mode` must be interpreted by the exe.
2. **There is only ever one basis.** A `LOCAL_Y_AXIS` sprite, which stands upright, needs a second
   basis the shader is never given — so the exe rewrites the vertex data, and it cannot do that
   differently for two modes that both arrive as camera-aligned axes.
3. **`align_to_velocity` is commented out in the shader** (lines 414-418). This is the second,
   independent confirmation of round 3's negative; the first was `align_to_velocity='false'` on all
   435 emitters. The branch is present and disabled.
4. **`particle_distortion.fx` — the shader every one of the 14 `*_Y_AXIS` emitters uses — compiles
   the very same `particle_vertex_30()`.** So `LOCAL_Y_AXIS` and `BILLBOARD` run through identical
   shader code, and any difference between them is entirely in what the exe puts in the buffer.

And the data side agrees. Cross-tabulating all 13 `landbattle` `BILLBOARD` emitters against the 261
`CAMERA_FACING` ones over **7 basis-side attributes** (`fx`, `render_method`,
`adjust_direction_by_offset`, `align_to_velocity`, `release_type`, `quality_level`,
`start_channels_linked`): **not one value is unique to `BILLBOARD`**. A renderer given only the
emitter record cannot tell the two apart.

The one honest exception, and it is worth recording because a strict version of that cross-tabulation
caught it: four shading terms are unique to a `BILLBOARD` emitter — `cannon_hit_smoke` `lighting=1.5`,
`cannon_hit_smoke_small` `thickness=1.2`, `impact_water_lrg2` `lighting=-1.088359`,
`impact_water_sml2` `lighting=-0.965659`. These are **shading** terms, not basis terms: the shader
reads them out of `fade_info_thickness_and_lighting` and passes them to the pixel shader, so they
cannot change a vertex's position. The test asserts on the basis-side attributes and prints the
shading ones, so a future real discriminator shows up rather than being assumed away.

**Verdict.** `BILLBOARD` is drawn like `CAMERA_FACING` because it is **indistinguishable from it in
everything the renderer is given**. What the original does differently stays **UNKNOWN**, and the
target is now a specific one rather than a vague exe question: **the basis the exe chooses for
`sprite_facing_mode`**. Unlike `MUZZLE_HEIGHT`, this one is not worth a Ghidra step on its own — it
only matters for 18 of 435 emitters, 13 of them in `landbattle.xml`, and it is a visual difference
inside an effect that is already PROVISIONAL in size and position.

Re-checked against the install in the round-4 recovery pass: that test and
`every_sprite_facing_mode_is_a_named_one` both pass on a real install, so §9 stands as written and
§9's numbers (13 `BILLBOARD` / 261 `CAMERA_FACING` in `landbattle.xml`) are the live ones.

## 10. Round 4 recovery: two mutation probes were left switched on

The round-4 session died with its weak-test audit half-done, and **two of the probes it had planted
to prove its tests could fail were never reverted**. Both were correct about themselves, which is the
point of the exercise: each one was caught by the test written for it.

### 10.1 `fire_group` was throwing the data column away — a regression this round

`battle::fx::fire_group` read `db.fire_effect(projectile)`, bound it to `_probe`, and returned the
weapon-family guess anyway. So every muzzle flash in the game was drawn with the round-1
**PROVISIONAL** name-matching table, which is documented in §6.1 as disagreeing with the shipped
column in several places: `cannon_18_pounder` is plain `LandGunFire`, not `_large`, and
`LandGunFire_canister` — 13 of the 144 rows — is a group **no calibre rule reaches at all**.

The data column wins again. `fire_group_by_name` survives as the fallback for the 24 rows with no
`fire_effect` (arrows, grenades, fragments) and for a modded table that leaves it empty. Caught by
`fx_draw.rs::fire_group_reads_the_projectiles_column_then_falls_back`, whose `assert_ne!` compares
the returned group against what the family *would* have produced.

The tally that column has to produce is now **pinned exactly** rather than as a floor. Round 4 had
`assert!(with_group >= 60)`, which passes if half the column stops being read — the exact failure
that was live. `ntw_data/tests/effects_data.rs::projectiles_name_their_own_fire_group_and_trail` now
asserts all of: 120 of 144 rows set the column, 24 do not, and the per-group tally verbatim
(`CannonFire` 56, `LandGunFire_canister` 13, `LandGunFire_large` 9, `LandGunFire_mortar` 8,
`MusketFire` 8, `LandGunFire` 7, `LandGunFire_howitzer` 7, `LandGunFire_small` 7,
`fougasse_default` 2, `rifleFire` 2, `pistolFire` 1). §6.1 quotes the same numbers, so the comment
and the data cannot drift apart silently. Re-measured on the install: matches.

### 10.2 `quad_mesh` computed the upright basis and discarded it — a regression this round

The `*_Y_AXIS` branch computed `y_right` / `y_up`, then the per-particle line read
`let _m = p.facing.is_y_axis();` and used the camera basis regardless. So the 16
`LOCAL_Y_AXIS` / `WORLD_Y_AXIS` emitters — the ground-impact distortion and the water ripples,
§8 — were being drawn as camera-facing cards after all, and tipped out of sight from a low camera,
which is the whole reason §8 changed them. The upright basis is now the one actually used: vertical
up, turning about the vertical only. Caught by
`fx_draw.rs::a_y_axis_sprite_stands_upright_while_a_camera_one_tips_to_the_eye`.

### 10.3 `crates/napoleon` did not compile

`fx_draw.rs:1292` assigned a `String` to `Projectile::weapon_family`, which is `Option<String>` since
round 3. One line; it meant nothing in the crate was being tested at all until it was fixed.

### Needs an in-game check

These are the three items the fixes change **on screen**. None can be judged from a unit test, because
each is a question about a picture.

- **Needs an in-game check:** a canister shell's muzzle flash. It is one of the 13
  `LandGunFire_canister` rows, and it is the group the PROVISIONAL table could never produce, so its
  flash should now look different from every other gun's — bigger and dirtier, per the emitter list
  in that group. Before the fix it drew `LandGunFire_large`.
- **Needs an in-game check:** a 18-pounder's muzzle flash, which §6.1 says is plain `LandGunFire` in
  the data and not `_large`. If a 18-pounder now flashes *smaller* than a 12-pounder, that is the
  data being right and round 1 being wrong, not a new fault.
- **Needs an in-game check:** a shell's ground impact from a **low camera**. The earth and debris
  cards (`ground_impact_distortion`, `_small`) should now lie on the ground and stay visible instead
  of tipping to face the eye and vanishing edge-on. Watch a round shot land while the camera is low
  and behind cover — that is the case §8 exists for and the one the broken basis threw away.
  **Review correction:** those two are distortion sheets, not earth or debris, and they stand
  *upright* (a vertical card turning about world up), they do not lie on the ground. The earth and
  debris are `BILLBOARD` and are drawn camera-facing (§9).
- **Needs an in-game check:** water ripples (`ripple_distortion*`, also `LOCAL_Y_AXIS`) seen from a
  low camera on a shoreline, same question as the ground cards.

## 11. Round 5: the three air-burst groups are ONE group, and nothing scaled size

This is the round's substantive finding, and it was found by *measuring* rather than by reading.

### 11.1 The finding

`AirExplosion_sml`, `AirExplosion_med` and `AirExplosion_lrg` are **byte-identical** in
`effects\landbattle.xml`:

| | entries | `spawn_interval` | `cell_size` | `cell_count` |
|---|---|---|---|---|
| `AirExplosion_sml` | 8 | 100 | 10 | 1 |
| `AirExplosion_med` | 8 | 100 | 10 | 1 |
| `AirExplosion_lrg` | 8 | 100 | 10 | 1 |

The eight, in order: `black_burn`, `sparks_airburst`, `sparks_airburst`,
`shock_distortion_airburst_med`, `explode_flare_airburst`, `explode_smoke_airburst`, `ribbon_360`,
`ribbon_additive_360`. Kept as `crates/napoleon`'s `fx_probe::tests::the_three_air_burst_groups_are_one_group`
(`#[ignore]`d, needs the install) and it also asserts the *negative* that this is not a parser that
stopped reading: the two generic ground impacts **do** differ, 9 entries against 11.

Round 2 recorded the `projectiles_explosions` row's fourth number as "rises with the pound count over
the shrapnel series, 2..14 m, so INFERRED to be a burst radius, **but it does not pick the group**".
The reason it does not pick the group is that **there is nothing for it to pick between**.

### 11.2 The bug that followed from it

`FxWorld::spawn`'s `strength` argument scaled **velocity only** — it reached `cone_direction` and
nowhere else. So with the three groups identical, **every shell's air burst was drawn at exactly the
same sprite sizes**: a 3 lb shell and a 64 lb shell produced the same picture. Nothing in the code
scaled `SCALE_INFO` at all. `air_explosion(pounder)`'s 6/12/24-pound thresholds chose between three
names that resolve to the same eight emitters, so the PROVISIONAL split could not have worked even in
principle.

### 11.3 The fix

`FxWorld` gained `size_scale`, applied to all three `SCALE_INFO` waypoints in `spawn_one`, and
`fx_draw::burst_scale` sets it from the shot's own row. **The scaling is PROVISIONAL** (review,
§14): the radii are shipped values (CONFIRMED by
`fx_probe::tests::the_burst_rises_with_the_pound_count_over_the_shipped_rows`, install), but the
scale column, the 10 m reference and the idea that the original sizes its sprites by this number at
all are ours:

| row | radius | scale | | row | radius | scale |
|---|---|---|---|---|---|---|
| `shrapnel_3lb` | 2 | 0.20 | | `shell_12lb` | 10 | 1.00 |
| `shrapnel_12lb` | 6 | 0.60 | | `shell_24lb` | 15 | 1.50 |
| `shrapnel_32lb` | 12 | 1.20 | | `shell_64lb` | 25 | 2.50 |
| `mortar_4_shell` | 15 | 1.50 | | `mortar_8_shell` | 25 | 2.50 |

`REFERENCE_RADIUS` is 10 m — a 12-pounder round shell's value — so the most common shot plays at the
shipped sprite sizes and everything else scales about it.

**INFERRED:** that the unit is metres and the number is a radius. **CONFIRMED:** that it is monotone
over the shell, shrapnel and mortar families and that those two families break it, which is why the
read is guarded:

- **by family** — only rows whose `air` group is one of the three `AirExplosion_*` groups. A `grenade`
  row's fourth number is 0, a `quicklime` row's 0.4, a `carcass` row's 0.99: not blast radii in metres
  for those shots.
- **by floor** — `MIN_BURST_RADIUS` 2.0. The **rocket** rows pass the family test (their air group *is*
  `AirExplosion_med` / `_lrg`) but carry 0.99, which over the reference is a 0.099 scale — a Congreve
  rocket's burst at a tenth of the sprite sizes, smaller than anything else in the file. Left
  **UNKNOWN** with the target named: the exe's own read of that numeric block. Both guards are pinned
  by `fx_probe::tests::burst_scale_falls_back_on_calibre`, which also asserts that the rockets are
  excluded by the *floor* specifically, since the family test cannot exclude them.

### 11.4 What this limits — a kept negative worth reading before any check

**Only 25 of the 144 `projectiles` rows carry an `explosion` key, and a field gun's own
`*_pounder_shot` row is not one of them.** `fort_12_pounder_shot`, `cannon_6_pounder_shot`,
`cannon_12_pounder_shot`, `fort_18_pounder_shot`, `fort_24_pounder_shot`, `fort_32_pounder_shot`,
`cannon_18_pounder_shot` and `siege_cannon_64_pounder_shot` all name **no** explosion row. So their
air burst is the PROVISIONAL calibre fallback and they have **no burst radius behind them at all**.

What does resolve: the **shrapnel** variant of a field gun (`fort_12_pounder_shrapnel` →
`shrapnel_12lb`), the howitzer / mortar / unicorn shells, and the experimental carcass / quicklime /
grenade families. Kept as
`fx_probe::tests::the_burst_rises_with_the_pound_count_over_the_shipped_rows`, which pins the 25 and
the ten that are field-gun round shots.

**This is why the round's shot-relative claims name a howitzer or a mortar and not a field gun**, and
it is why the Austerlitz check line asks about a *howitzer* burst: most of what the user sees on that
battlefield is still the fallback path.

## 12. Round 5: `fx_probe`, and answering a check with a number

Every battle-effects check in `docs/HANDOFF.md` that is not a clean negative is a **comparative**
sentence, and asking a person to judge one from pixels fails in both directions: a difference too
small to see reads as "no difference", and a difference that is merely *different* reads as "matches".

`crates/napoleon/src/battle/fx_probe.rs` measures instead. [`probe`] releases a group into a fresh
`FxWorld` at a fixed seed, steps it at the model's own 0.1 s tick and reports particle count, peak and
covered area, mean and max life, drift, blend share and facing share; [`probe_shot`] does the same for
a **shot**, with its own row's size scale, which is the only way to ask a size question at all (§11.1).
[`run`] then evaluates 12 group claims (13 before the review removed a vacuous one, §14) and
[`run_shots`] 4 shot claims, each a relation between two named groups, and prints a verdict with the
numbers behind it. [`summary`] counts them, and counts a
claim that **could not be answered** as a failure rather than a pass — a stale group name must not read
as a clean bill of health.

Round 5's output (before the review: the shot line printed the *scale* as "radius 1 m", and the
scorch claim counted `BILLBOARD` emitters we draw camera-facing — both fixed, §14; the review could
not re-run it, there is no install in its sandbox):

```
FX check shrapnel_12lb_bursts-smaller-than-a_12lb_shell PASS    76.9586 m^2 covered at radius 1 m (AirExplosion_sml vs AirExplosion_med) <  213.7740 m^2 covered at radius 1 m
FX check ground-scorch-sprites-stand-upright          PASS     0.2222 upright emitter share >= 0.2
FX check musket-smoke-lingers                         PASS     5.3499 s mean life >= 1
FX check: 0 of 17 claims FAIL, 0 could not be answered, everything held overall
```

**What this is and is not.** It measures *our* particle world driven by the shipped
`landbattle.xml`, so a PASS is a statement about what we draw from the original's data — **not** a
comparison against the original's picture, which is what the in-game check is still for. What it
removes is the need to eyeball.

Three measurement traps it had to close, each with its own test:

1. **The end of a size ramp is never sampled.** `advance` drops a particle the moment `age == life`,
   which is exactly where `secondary_scale_life` (1.0 in the shipped file) puts the ramp's maximum, so
   frame-by-frame sampling systematically under-reports. The three waypoints are folded in.
   `the_end_of_a_size_ramp_is_measured_not_sampled_past` pins the size of the bias rather than only
   asserting the fixed behaviour.
2. **The watch has to outlast the longest-lived emitter**, which it did not: round 5 read
   `Uber_smoke_brown` as `life_range variance(60, 5)` and `explode_smoke_airburst` as
   `variance(15, 5)`, so a 15 s watch was truncating them mid-growth. `PROBE_TICKS` is now 750 ticks
   (75 s) and `every_probe_watches_its_group_outlive_the_clock` holds that against the real file.
   **INFERRED, not CONFIRMED** (review): the round-5 run that read the two lifetimes kept no record
   and no test asserted them; that install test now asserts both, so the next install run settles it.
   Which groups play `Uber_smoke_brown` was not recorded either, so "a gun's ground dust lives a
   minute" is unverified.
3. **"Upright" is a claim about emitters, not particles.** The scorch's upright *particle* share is
   0.2 %. Round 5's claim used the share of emitters the file marks non-camera-facing (4/18 = 0.222),
   but three of the four are `BILLBOARD`, which we draw camera-facing; the review changed the claim to
   count only what we draw upright (exactly one, §14). The survey test
   `every_non_camera_facing_emitter_of_the_shipped_file` prints the whole survey: **46 non-camera-facing
   group entries** in the file, `BILLBOARD` 23 / `LOCAL_Y_AXIS` 22 / `WORLD_Y_AXIS` 1, no
   `VELOCITY_FACING`. Of the scorch's four, only `ground_impact_distortion` actually stands up in our
   draw — a single 0.7 s sheet scaling `0.0 -> 20.0 -> 30.0` m — because round 4 closed `BILLBOARD` as
   indistinguishable from `CAMERA_FACING`.

### 12.1 The environment switches

- **`NAPOLEON_FX_CHECK=<substring>`** (or `all`) prints every claim verdict, a per-group measurement
  table and the run's [`Summary`], then quits. No camera and no battle needed in principle; it runs
  inside the battle so it works with the loaded library either way. `all` prints exactly the text the
  install test prints, through `report_everything`, so a run and a test cannot disagree about a
  verdict; a substring prints just those claims with the check line each stands for. A group still alive
  at the end of the watch is marked `(STILL ALIVE AT THE END OF THE WATCH)`, because its peak size was
  then measured mid-growth.
- **`NAPOLEON_FX_SHOT_AT=<battle seconds>`** gates `NAPOLEON_FX_SHOT` on the **battle clock**. Ported
  from the stale round-3 tree, where it had been written and never committed. The reason it matters is
  in the comment it carries: at Austerlitz's deployment distance a volley is a few pixels wide, so a
  shot at the first volley shows almost nothing.

Still outstanding from earlier rounds and unchanged by this pass:

- **Needs an in-game check:** the `RENDER_METHOD_DISTORTION` sprites (`shock_distortion`,
  `ground_impact_distortion`) are drawn flat — PROVISIONAL until the distortion pass exists (§4).
  This is now the *only* upright sprite of the ground scorch (§12), so it is worth a close look.

## 13. Round 5 item 2: `MUZZLE_HEIGHT` — parked, for the debugger

Round 3 closed the data route with three exhaustive negatives (§5 row 12) and named the target as
"the exe's own muzzle computation". Round 5 was given one optional, time-boxed Ghidra step for it
and **stopped without taking it**, because the honest position is that this is a debugger question and
a blind search is not a step.

What would settle it, in order of cost:

1. **A breakpoint on the effect-system call that plays a gun's fire group**, with the muzzle transform
   it is handed read out. That is one debugging sitting, and it answers the question exactly — it is
   the same shape as the `promotion_probe.cdb.txt` 0-G already wrote.
2. Failing that, a Ghidra trace from the `gun_type_to_projectiles` reader, which is where the exe's
   per-gun muzzle naming would have to come from. Round 3's negative is that the 22
   `gun_type_to_projectiles.muzzle_flash` names reach **no** group, group entry or emitter in any of
   the three effect files, so this trace is looking for a name the file does not resolve, which is a
   weaker lead than it looks.

`MUZZLE_HEIGHT` therefore stays **PROVISIONAL at 1.35 m**, unchanged since round 1, and no Ghidra
copy was opened this round. Nothing else in §2 depends on it: the burst size (§11) and the fire group
(§6) both come from data.

## 14. Review (S2 reviewer, on `work/review-next-s2`)

A review of rounds 1-5 against the bar "every CONFIRMED has kept evidence; stand-ins tagged". No
install in the review sandbox, so no install test was re-run; everything below is from reading the
code and the kept tests.

**Evidence checked and accepted as tagged:** the fire group is `projectiles` column 31 (exact tally
pinned in `effects_data.rs::projectiles_name_their_own_fire_group_and_trail`); the trail join on
column 6; the per-row air-burst group choice (`explosion_rows_name_the_effect_groups_they_play`, ten
rows verbatim); 22 of 283 land emitters not camera-facing (`every_sprite_facing_mode_is_a_named_one`);
the three air-burst groups being identical (`fx_probe::tests::the_three_air_burst_groups_are_one_group`).

**Downgraded:**
- the air-burst **sprite scale** (§11): was called CONFIRMED in `fx.rs` / `fx_draw.rs` ("the fact that
  it is *the* size knob is CONFIRMED by the groups being identical"). Identical groups prove only that
  the name cannot size a burst, not that the original sizes it by the fourth number. Now PROVISIONAL.
- the dust **interval** reading was called CONFIRMED in a test comment; INFERRED, as `fx::dust` says.
- `Uber_smoke_brown` 60 s / `explode_smoke_airburst` 15 s: INFERRED until the install test that now
  asserts them has run.
- `LOCAL_Y_AXIS` drawn upright: INFERRED from the name (the shader is given one camera basis only).

**Bugs fixed:**
- `fx_draw::quad_mesh` derived the billboard basis from `eye.normalize()` — the direction from the
  world origin to the camera — so every quad was skewed off the screen plane unless the camera looked
  at (0, 0, 0). It now uses the camera's own right/up, which is what the shipped `particle_vertex_30`
  does (`g_camera_aligned_x/y_axis`).
- `particle.wgsl`: Bevy's `AlphaMode::Add` is a premultiplied blend state; the shader returned straight
  alpha, so a flash darkened what was behind it by `1 - a` and ignored its own fade.
- `FxWorld::advance` took the dampening share off per 0.1 s tick, ten times its documented per-second
  rate; now `(1 - d)^dt`.
- `FxWorld::dust` released one puff per step for a 0.05 s interval at the 0.1 s tick (half the
  shipped rate) and its timer grew without bound; every owed puff is now released, capped at 4.
- `tick_fx`'s volley cursor (a system `Local`) survived into the next battle.
- `projectile_fx::find_strings` could report a string starting inside another, which could underflow
  and panic on a modded table; the scan is now non-overlapping.
- `fx_probe`: `shrapnel-burst-smaller-than-shell-burst` was `Relation::Same` under a "smaller" name
  (could never fail) and is removed; the scorch claim counted `BILLBOARD` emitters as upright; shot
  verdicts printed the scale as "radius N m" and always `<`.
- `every_claim_names_a_shipped_group` opened the install without `#[ignore]`, so `cargo test` failed
  without one.
- `ntw_data` example `table_list` printed nothing (wrong path parsing) and misread versioned headers.

**Still to check in game:** that quads now face the screen everywhere on the map (the skew was worst
far from the origin); that a muzzle flash brightens and fades rather than darkening; that dust under a
walking unit is denser than before (twice the puffs at 0.05 s); the open items in §5 and §10.

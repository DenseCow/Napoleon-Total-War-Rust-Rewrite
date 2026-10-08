# Horses, cavalry, command figures and clip selection (cavalry worker)

Tags: **CONFIRMED** = checked on every shipped file/row by a test; **INFERRED** = strong reading; **UNKNOWN**.
Stand-ins in code are tagged PROVISIONAL / PLACEHOLDER. No game data is copied here, only structure and counts.

Code: `ntw_formats::{weighted_mesh, mount, battle_animation, unit_animation}`, `ntw_formats::unit_model::BattleTables`,
`napoleon::soldiers` (`build_figure`, `FigureKit`, `MountKit`), `napoleon::model_viewer::soldier`, `napoleon::battle::view`.
Tests (`--ignored`, real install):
- `ntw_data/tests/cavalry_install.rs`: `every_unit_figure_resolves_clips`, `every_mounted_unit_resolves_to_horse_and_rider`,
  `command_figures_resolve_equipment`;
- `ntw_formats/tests/real_install.rs`: `every_weighted_mesh_parses`.
Probes: `cargo run -p ntw_formats --example mount_probe -- slots|speed|text|words|wsurvey|uvs ...`,
`cargo run -p ntw_data --example cavalry_probe -- <unit filter>`.

## 1. Which units ride
- `unit_stats_land` #2 `num_mounts`, #14 `mount` (FK `mounts`), #15 `mount_entity` (FK `battle_entities`),
  #16 `mount_type`.
- 143 units have `num_mounts > 0` and a `mount`. These are cavalry and generals' bodyguards (24 or 32 mounts).
  We treat them as **mounted** (INFERRED).
- 47 artillery units also have a `mount` (`horse_artillery_*`) but `num_mounts = 0`. Those horses are the gun and limber
  teams; the crews stand. We do not mount them (INFERRED).
- `Cav_Light_Brunswick_Hussars` has no `uniforms` row in the shipped data, so no route can draw it (CONFIRMED).

## 2. Mount → horse model (CONFIRMED: `every_mounted_unit_resolves_to_horse_and_rider`, `every_weighted_mesh_parses`)
```
unit_stats_land #14 mount ("horse_hussar_mixed")
  -> mount_variants (mount, model key, weight)       45 rows, 28 mounts, all weights 1.0
  -> warscape_animated (model key, texture stem, kind)   30 horse models, all with stem UnitModels/horse/textures/horse
  -> warscape_animated_lod (id, mesh path, distance, model key)   4 LODs per horse (distances 10, 20, 50, 0)
       -> UnitModels\horse\horse_<A..F>_<saddle>_lod<N>.variant_weighted_mesh
```
- 29 models and 113 LOD meshes are reachable from `mount_variants`. All of them parse with the strict reader.
- **Coat colour** is the letter A..F: light brown, grey, black, dark brown, white, brown. Each letter is a separate mesh with
  the same geometry and different UVs into **one shared texture** (`horse_diffuse/normal/gloss_map.dds`). This is
  CONFIRMED for A vs C by a UV survey. A "mixed" mount lists several letters, and we pick one per horse by its weight with
  the caller's seed. How the game picks is UNKNOWN; our pick is PROVISIONAL.
- **Saddle** (basic, covered, hussar, iberian, artillery) is part of the file name, so the mount key fixes it.
- **Camels:** the mount is `camel_base`, with its own skeleton and the `mount_camel` table. The same route resolves for
  the 6 camel units.
- A mesh holds optional pieces: two manes (`hat01/02`), three head markings (`head01..03`), the body, a saddle,
  stirrups and the artillery harness. We draw one piece per name group, picked by seed. This is PROVISIONAL: the game's
  rule is UNKNOWN. The three heads share their geometry and differ only in UVs, which suggests one per horse.
- There is no colour mask; the horse texture is drawn as is.

### `.variant_weighted_mesh` layout
See the module docs in `weighted_mesh.rs`. In short:
- the file has magic `0x12345678`, version 1, named scalar and vector material parameters, and a table of pieces;
- each vertex has a uv, a normal, a tangent, a list of `{bone, bone-local position, bone-local normal, weight}`
  influences, and 4 unknown floats;
- indices are u32.

A posed vertex is `Σ w·(M_bone · p_bone)`, so no bind pose is needed. This is the same idea as the soldier part meshes.
Bones index the mount clip skeleton (40 bones for the horse, including `Lstirrup1/2` and `Rstirrup1/2`).

**Update (campaign-data worker): all 286 now parse** (attachments after the pieces, two headerless
layouts; see `weighted_mesh.rs` and `analysis/campaign/CAMPAIGN_DATA.md` §7). Previously 23 did not:
- testdata;
- campaign agents;
- `napoleon_battleoutfit_lod*`, `campaign_soldier_base`, `euro_equipment`.

They have a non-zero trailer (probably attachments) or another magic. None of them is a mount; decoding them is left to do.

## 3. Rider ↔ horse (INFERRED from the files; pairing CONFIRMED by `every_unit_figure_resolves_clips`)
- The rider clips are authored **in the horse's space**. In `H_Rider_Stand` frame 0 the rider's hips are at y = 1.91,
  z = 0.72, over the saddle (the horse's hips are at y = 1.66, z = 0.07; its Spine1 is at z = 0.90). His feet are at the
  stirrup bones. So the rider shares the horse's origin and transform. There is **no attachment bone**.
- Paired clips have the same frame count, duration and root motion. For example:
  - `Horse_Walk` and `H_Rider_Walk`: 23 frames, 1.10 s, 1.471 m/s;
  - trot: 3.533 m/s;
  - canter: 5.252 m/s;
  - gallop: 10.19 m/s.

  This holds for every mounted figure the DB route picks (429 figures × stand/walk/run). The rider and the horse
  therefore play the same frame index.
- The rider is a normal soldier (41-bone man skeleton, `uniforms` → `.unit_variant` soldier file). Officers and
  musicians ride too, through their personality tables (§5).

## 4. Animation tables → clips
`animations\animation_tables\animation_tables.txt` holds the tables. `animations\battleconfiguration\<fragment>.txt` holds
the fragments. The parsers are in `battle_animation.rs`; the format is in its module docs. Every table and fragment parses
(CONFIRMED).

Facts (CONFIRMED by the install tests unless tagged):
- `unit_stats_land` #9 `man_animation_type` names a table (`man_musket`, `rider_sabre`, `rider_lance`, `rider_camel` ...).
  So does `battle_personalities` col 2 (`personality_drummer` ...).
- **One table serves both foot and mounted use.** `rider_sabre` lists foot fragments (`swordsman_movement_fragment` ...)
  and rider fragments (`horse_rider_base_fragment`, `sabre_horse_rider_fragment`). The rider slots carry a `RIDER_` prefix,
  so they never collide with foot slots.
- `mount_table` in a table names the mount's table (`mount_horse`, `mount_camel`). For every mounted unit it equals
  `unit_stats_land` #16 `mount_type`.
- A later fragment replaces a slot that an earlier fragment names, and `cancel` removes the slot (INFERRED).
- A slot that repeats in a fragment lists alternatives (`STAND_TRAINED` ×5).
- `battle_entities` (s, s, s, f×10, s, f×6, i): col 1 is the class, col 2 the skeleton (`man`, `horse`, `camel`),
  col 3 the walk speed, col 4 the run speed, col 7 the charge speed (INFERRED from the values):
  - `infantry_euro_medium`: walk 1.4, run 3.6;
  - `horse_heavy`: 2.6 / 10 / 11.5;
  - `horse_light`: 2.8 / 12 / 13.

**Slots used for idle/walk/run.** These are engine vocabulary, the same in every fragment:

| figure | stand | walk | run |
|---|---|---|---|
| man on foot | `STAND_TRAINED` → `STAND` | `WALK_TRAINED_n` → `WALK_n` | `RUN_TRAINED_n` → `RUN_n` |
| mount | `STAND` | `WALK_n` | `TROT`, `CANTER`, `GALLOP`, `RUN_n` |
| rider | `RIDER_<mount slot>`, else without the `_n` (`WALK_1` → `RIDER_WALK`) | same | same |

`n` is a speed level. The clips move at the speeds their file names give (`MUS_T_Walk_100/127/173`, `Jog_219/313`). Measured
from the root bone (`Anim::root_speed`), `mus_t_walk_127` gives 1.269 m/s.

**Our choice** (`unit_animation::choose_clips`):
- prefer `_TRAINED`;
- pick the level whose clip root speed is closest to the entity's walk or run speed. The mount's speeds apply when
  mounted.
- `alt` (per-man seed) picks among a slot's alternatives.

This is PROVISIONAL. The exe's rules are UNKNOWN: training level → trained/irregular, speed-level choice, blending,
`*_TO_*` transitions and idles (`STAND_TRAINED_IDLE_n`).

Results:
- Line infantry gets `STAND_TRAINED` / `WALK_TRAINED_2` (1.27) / `RUN_TRAINED_2`.
- Cuirassiers get `STAND` + `RIDER_STAND`, `WALK_1` + `RIDER_WALK`, and `GALLOP` + `RIDER_GALLOP`.
- Root motion is removed so the clips loop in place, and the unit's own movement carries the men. The drift is the same
  for rider and horse. The engine's handling of root motion is UNKNOWN.

## 5. Command figures and their equipment (CONFIRMED: `command_figures_resolve_equipment`)
```
unit_stats_land #4 officer / #5 musician / #6 standard_bearer  (battle_personalities keys)
  -> battle_personalities (key, model set, animation table, equipment theme, role)
       euro_officer         euroline personality_officer          generic_officer   officer
       french_drummer       euroline personality_drummer          france_drummer    drummer
       euro_standard_bearer euroline personality_standard_bearer  generic_standard  standard_bearer
  -> warscape_equipment_themes (key, primary, secondary, flag?, ambient, instrument)
       france_drummer   -> euro_straightsabre, -, false, french_infantry_equipment, euro_drum_kit
       generic_standard -> euro_straightsabre, -, false, -, euro_flagpole
  -> warscape_equipment_items (item, set): euro_drum_kit = euro_drum + euro_ldrumstick + euro_rdrumstick
  -> equipment pieces rigid_equip_<item>[NN]_lodK
     euro_drum on bone 0 (Hips), drumsticks on 29/30 (hands), bugle on 29, flagpole on 3 (Weapon3)
```
Which sets are shown comes from the fragment that supplies the figure's current clip: its
`default_equipment_display = primary_weapon, secondary_weapon, ambient, personal, defensive`.
- The theme's **instrument column is the `personal` set** (INFERRED). Drummer, bugler and standard-bearer fragments
  display only `personal`, and their themes put the drum kit, bugle or flagpole in that column.
- Mounted figures use the rider fragment's list. A mounted bugler (`horse_rider_base_fragment`:
  `primary_weapon, secondary_weapon`) therefore shows his sabre, not the bugle, while riding.
- Per-clip overrides such as `RIDER_MOUNT_LEFT ... primary_weapon = off, ambient = on` are parsed (`FragmentClip::attributes`)
  but not used yet.
- The **flag cloth** of a standard bearer is not in the equipment data. The flagpole piece is only the pole and finial
  (140 vertices). No unit-flag mesh or texture is referenced from the DB route. Candidates are the faction folders
  `ui\flags\<faction>\` (`flag_animation0000..0011.tga`, `large.tga`) and `rigidmodels\flags\` (the floating unit-ID
  banners). The source is UNKNOWN and needs the exe (§7). We draw the pole only.
- The theme column #3 (bool, named `flag`) is false in every row checked. Its meaning is UNKNOWN.

## 6. What the game shows that we don't yet
- The flag cloth (§5) and cloth/physics of plumes and manes.
- Horse LOD switching: `warscape_animated_lod` distances are there, but we always draw lod1.
- Transitions, idles, turns, charge and dismount clips (fire, reload, melee, death and knock-down clips are played now:
  `analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` §1.7). They resolve from the same tables but are not
  played.
- Per-man clip alternatives: the battle still shares 3 kits per unit. (Per-man animation phase: done, GPU skinning.)
- Horse normal and gloss maps, and the material scalars in the mesh header (`specpower`, `colourmapfactor` ...).

## 7. Questions for the exe (Ghidra)
Fidelity 0-D answers (`analysis/fidelity/UNITS_TERRAIN_FIDELITY.md`): 1 partly (alternatives §1.2–1.3, speed level and
time-scaling §1.4; trained vs irregular and idles still open); 2 partly (the slot table pairs every `RIDER_x` with the
mount slot `x`, and `RIDER_WALK`/`RIDER_RUN` with `WALK_1`/`RUN_1`, §1.1; the runtime binding is still open); 4 partly
(flag cloth = verlet `.logic` + `flags.tai` atlas, §3; attachment frame open); 6 (root motion used by the model, §1.6,
INFERRED). Horse LOD: soldier/variant LOD tweaks in §2; `warscape_animated_lod` distances are the horse ones.
1. How a figure's clip is picked from a slot family:
   - the trained vs irregular choice (training level? `entity_training_levels` only holds a float);
   - the speed-level choice;
   - whether playback is time-scaled to the unit's speed;
   - how alternatives and `STAND_*_IDLE_n` idles are chosen.
2. How the rider is bound to the mount at runtime: is it a shared origin, as the clips suggest, or a bone/attachment
   transform? And are paired clips always started on the same frame?
3. How one colour variant per horse is picked from `mount_variants` (RNG and seed), and which optional pieces are shown:
   mane `hat01/02`, head marking `head01..03`, stirrups.
4. The standard bearer's flag cloth: where the mesh and texture come from (per faction / per unit?), and the meaning of
   `warscape_equipment_themes` col 3.
5. The trailer of the non-mount `.variant_weighted_mesh` files (Napoleon's battle outfit, campaign agents).
6. Root motion: is the clip drift used to move men, or stripped as we do?

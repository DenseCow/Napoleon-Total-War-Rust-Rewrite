# `.anim` skeletal animations and soldier skinning (unit-models worker)

Tags: **CONFIRMED** = checked on every shipped file by a test; **INFERRED** = strong reading; **UNKNOWN**.
Reader: `crates/ntw_formats/src/anim.rs`. Tests: `crates/ntw_formats/tests/real_install.rs`
(`every_anim_parses`, `every_unit_resolves_to_parts`, `part_mesh_lod_order_survey`).
Probe: `cargo run -p ntw_formats --example variant_probe -- anim|bonepos <path>`.
No game data is copied here, only structure and counts.

## 1. Files
data.pack holds 3,814 `.anim` files (`animations\men\...`, `animations\animals\...`, `matched_combat`, `campaignpieces`,
`testdata`). 3,783 parse with the layout below (CONFIRMED, to EOF). **All 3,814 parse now** (campaign-data worker): the
31 under `testdata\animations` use 28-byte keys (translation + rotation, no unknown floats), and the 4 oldest
(`testdata\test.anim`, `ranger_test_lod1`, `ranger\ranger_test_lod1`, `musketman_lod1_anim`) store the bones as parent
indices only (no names) and have no event list; their duration field is 3.333 s for 66 frames (convention UNKNOWN).
Not used by the game (INFERRED).

## 2. Layout (all little-endian): CONFIRMED on 3,783/3,783
```
f32  frame_rate      20.0 in every file
f32  duration        == (frame_count - 1) / frame_rate in every file
u32  bone_count      41 in 3,452 files (the soldier skeleton), 40 in 171, others 0..38 (animals, props)
bone_count x { u16 n; [u16; n] UTF-16 name; i32 parent }   parent = -1 or an earlier bone
u32  frame_count
frame_count x bone_count x KEY (40 bytes)
     f32 x3  translation, relative to the parent (roots: model space)
     f32 x4  rotation quaternion x, y, z, w, relative to the parent
     f32 x3  UNKNOWN (0.001 for every bone in frame 0; small varying values later)
u32  event_count
event_count x { u32 n; n x (u16 len + UTF-16 string) }
```
Events seen (name: files): IMPACT_TIME 472, IMPACT_POSITION 472, FIRE_TIME 223, FACE_UP 177, FACE_DOWN 177,
FIRE_POSITION 114, FIRE_POSITION_RIGHT 60, DISTANCE 52, FIRE_POSITION_LEFT 24, ON_BONE3 20, OFF_BONE3 16, ON_BONE2 13,
OFF_BONE2 13, FUSE_POSITION 6. Arguments are decimal text (`"0.25"`); times INFERRED to be seconds, positions metres.

## 3. Space and skeleton
- Y up, a man faces **+Z**, his left is -X (left-handed D3D space like the rigid models). The Hips root sits ~1.05 above
  the origin (CONFIRMED by evaluating `mus_standt.anim` frame 0: toes in front of the feet at +Z).
- `world(bone) = world(parent) * T(translation) * R(rotation)`. The posed French fusilier spans y = 0.003 .. 2.06 with the
  head at 1.65 .. 1.92 and the hat above it (test `every_unit_resolves_to_parts`).
- Soldier skeleton (41 bones): 0 Hips, 1-3 Weapon1-3 (extra roots that carry weapons), 4/5 Left/RightUpLeg, 6 Spine,
  7 Tail, 8/9 Left/RightLeg, 10 Spine1, 11/12 Left/RightFoot, 13 Spine2, 14 LeftShoulder, 15 LeftToeBase, 16 Neck,
  17 RightShoulder, 18 RightToeBase, 19 Head, 20/21 Left/RightArm, 22 Brow, 23 Eyes, 24 Jaw, then forearms, forearm rolls,
  hands and fingers.
- Root motion: locomotion clips move the roots forward (`mus_t_walk_127`: Hips +1.65 m in 1.3 s = 1.27 m/s, matching the
  file name). Our renderer removes that drift to loop in place (PROVISIONAL; the engine's handling is UNKNOWN).
- Clips are picked through the DB animation tables (`unit_stats_land` #9 -> `animation_tables.txt` -> fragments, with
  speeds from `battle_entities`); see `CAVALRY.md` §4. The hard-coded `mus_t_idle_1` / `mus_t_walk_127` are gone.

## 4. Soldier vertices (`.variant_part_mesh` format 1, 40 bytes): field map INFERRED
From a byte survey of 322,982 vertices (`variant_probe bytestats 40`) and hand checks:
```
 0 f16 x3 position in bone A's frame     6 f16 u
 8 f16 x3 position in bone B's frame    14 f16 v
16 4 bytes always 0                    20 f16 x2 always (1.0, 1.0)   (UNKNOWN)
24 u8 x3 normal (bone A frame)  27 u8 bone A
28 u8 x3 normal (bone B frame)  31 u8 bone B      (28..31 all zero when unused)
32 u8 x3 tangent                35 u8 weight of bone A, 0x80..0xFF (/255); bone B gets 1 - w
36 u8 x3 binormal               39 always 0
```
- Unit vectors are `(b - 127.5) / 127.5` in **z, y, x** byte order (D3DCOLOR b, g, r). CONFIRMED statistically: the stored
  normal agrees in sign with the geometric face normal for 193,183 of 196,145 single-bone triangles in that order
  (90,959 net for x, y, z order). `(p1 - p0) x (p2 - p0)` points outward in file space.
- Posed position = `w * (M_A * p_A) + (1 - w) * (M_B * p_B)` with the model-space bone matrices of any animation frame. No
  bind pose is needed. The two bone-local positions of a vertex agree to ~1.5 cm on average under `mus_standt` frame 0,
  so the authoring bind pose is close to, but not exactly, a shipped frame. The exe's GPU skinning (vertex declaration and
  shader) is UNKNOWN. The battle skins on the GPU (`napoleon::battle::skin`, `soldier_skin.wgsl`): each vertex keeps up
  to 4 bone-local positions/normals, the bone matrices of every clip frame sit in one storage buffer, and each man has
  his own phase (PROVISIONAL hash) and frame blend. FPS (RTX 5070, 1280x720, no vsync, `NAPOLEON_BATTLE_REPEAT`/
  `NAPOLEON_FPS_LOG`): 2,000 men 87-95 FPS with CPU skinning in lockstep -> ~175 FPS on the GPU with per-man phases;
  4,000 men 15.5 -> 110-127 FPS. The model viewer still skins on the CPU.
- Format 0 (64 bytes, plumes and equipment pieces): f32 x4 position (w = 1), u8x4 normal, tangent, binormal (same byte
  order), f32 u, v, then f32 u2, v2 (equipment only), u32 colour (FFFFFFFF), 8 zero bytes, f32 1.0, 1.0 (tail UNKNOWN).
- LOD order: the first LOD block is the most detailed in 287 of 297 part files (CONFIRMED survey; 5 single-LOD, 5 mixed).
  This corrects worker2's §4.1 note ("first block is the smallest").

## 5. Attachments and equipment
- VMPF attachment record (100 bytes, CONFIRMED size on all 89 files): `[u16; 16]` name, 16 f32, u32 bone. The floats are a
  row-major 4x4 with the translation in elements 3, 7, 11 (INFERRED: orthonormal 3x3, last row 0 0 0 1). Hats publish
  `plumes` on bone 19 (Head); a plume mesh (format 0) is placed at `M_bone * attachment`.
- Equipment containers (format 2; `equipment\mesh` 150 pieces, `mesh2` 34): per piece `[u16; 40]` name, i32 bone (-1 for
  `Reference`), u32 V, u32 I, V x 64-byte vertices, I x u16 indices; then the usual material parameters. CONFIRMED walk to
  EOF with matching header totals. Pieces sit on bones: muskets/rifles on 1 (Weapon1), sabres on 2, lances, stakes and
  ramrods on 3, flasks/bags on 0 (Hips), Napoleonic backpacks on 13 (Spine2). Names carry `_lod1`, `_lod2` ... suffixes
  (lod1 = most vertices). Texture pairing `mesh` -> `texture_*`, `mesh2` -> `texture2_*` is INFERRED from the names.
- 25 kind-1 names in `.unit_variant` files (e.g. `rigid_equip_euro_bag01`, `..._backpack01`) match no piece (Empire-era
  leftovers). The DB route resolves for every unit: `unit_stats_land` #10 (e.g. `france_musket`; ntw_data calls it
  `weapon_anim_group`) = a `warscape_equipment_themes` key -> primary / secondary / ambient / instrument sets ->
  `warscape_equipment_items` (item, set) -> pieces `rigid_equip_<item>[NN]_lodK`. Which route the exe uses is UNKNOWN; we use
  the DB route for the rank and file (PROVISIONAL). Officers, musicians and standard bearers get their themes through
  `battle_personalities`, now decoded and used (see `CAVALRY.md` §5).

## 6. Colour masks
Per-part `_colour_mask.dds` (DXT1). French line coat: red covers ~40% of the texels (coat body), green ~8% (facings),
blue ~0. Colours: `uniform_to_faction_colours` (uniform, faction, 3 x RGB), else `faction_uniform_colours` (faction,
3 x RGB); every uniforms row has colours (CONFIRMED). We tint `diffuse * lerp(1, colour_k, mask_k)` for k = R, G, B ->
primary, secondary, tertiary (PROVISIONAL: the shader is UNKNOWN). The result looks right for France (blue coat, red
facings) and Austria.

## 7. Open questions for the exe (Ghidra)
Fidelity 0-D answers (`analysis/fidelity/UNITS_TERRAIN_FIDELITY.md`): the per-man pick of clip alternatives (§1.2–1.3:
`selection % count`, one LCG number per man, CONFIRMED), speed levels and playback rate (§1.4), root motion (§1.6,
INFERRED: the model uses it), soldier LOD distances (§2). Still open: 1 (vertex declaration / shader, belongs with the
shader work §2), 2 (mesh and face picks per man), 3, 4, the trained/irregular choice in 5, 6, the phase in 7.
1. The vertex declaration and skinning shader for format-1 part meshes: confirm the field map, the meaning of bytes 16..23,
   and whether both bone-local normals are blended.
2. How one mesh per category and one face texture are chosen per man (RNG? seeded by what?).
3. Which equipment source is used (variant kind-1 entries or warscape_equipment_themes), and how secondary weapons show.
4. The colour-mask combine in the unit shader; whether the per-faction atlases (`units\atlas\*.atlas`) replace the loose
   textures at battle time.
5. Clip selection (man_animation_type / battle_entities / anim_reference_poses), blending, root motion, and the 3 unknown
   floats per key.
6. Unit formation rank counts and spacing at deployment. `unit_stats_land` #43 is INFERRED to be the default depth
   (ranks): line infantry 3, skirmishers 2, cavalry 3, mobs 4, revolutionary columns 8, artillery 1. The battle now uses
   it (and #44/#45 close spacing); confirm in the exe, and how a default deployment lays units out in an area.
7. Each man's animation phase (we hash unit id + index) and whether frames are blended.

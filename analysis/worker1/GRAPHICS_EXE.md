# GRAPHICS_EXE: rigid_model, vertex formats, textures and unit_variant, from the Napoleon.exe side (Worker 1)

Status: rigid_model + textures DONE (decoder in `crates/ntw_formats/src/rigid_model.rs` and `dds.rs`,
viewer in `crates/napoleon/src/model_viewer/`). unit_variant / variant meshes: first notes only (§7).

Pseudocode comes from Ghidra 12.1.4 decompiling Napoleon.exe. It is a reconstruction, not original
source, and none of it is pasted into the Rust crates. Raw dumps (gitignored) are in
`analysis/worker1/ghidra_out/gfx/` (`scalar_magic.c`, `magic_callers.c`, `mat_vert.c`,
`rigid_main.c`, `rigid_callers.c`). New Ghidra script: `ghidra_scripts/ScalarRefs.java`
(finds instructions that use given constants and decompiles their functions).

Confidence tags:
- **CONFIRMED**: seen in the exe code AND matches the bytes of the real files (the decoder parses all
  4,796 shipped `.rigid_model` files exactly to end of file).
- **INFERRED**: consistent with the data, but the meaning is our interpretation.
- **UNKNOWN**: not worked out yet.

Plain-language summary for beginners: a `.rigid_model` is a 3D object that does not bend
(a house, a fence, a cannon wreck). The file is a list of "meshes". Each mesh is one set of
triangles that share one look (a "material": which picture files to paint on them). Each
triangle corner (a "vertex") stores where it is, which way the surface faces, and which
point of the picture (the "texture") lands on it.

---

## 1. Where the files are

| pack | `.rigid_model` files | notes |
|---|---|---|
| buildings.pack | 3,266 | battle buildings, `rigidmodels\buildings\<name>\<name>_pieceNN_destructNN_lodNN.rigid_model` |
| rigidmodels.pack | 1,497 | props, campaign buildings, 3D indicators, naval parts, mountains |
| data.pack | 33 | `enginemodels\...` (artillery wrecks) and 5 old `testdata\` files |
| **total** | **4,796** | parse rate with our decoder: **4,796 / 4,796 (100%)** |

Side files: 2,887 `.rigid_model_header` (§4), 782 `.rigid_model_animation` (not decoded, §8),
95 `.animatable_rigid_model` (not decoded).

Note: the first u32 is the **mesh count**, not a version. The "version" (1..5) is the third u32,
and belongs to the first mesh (see 2.1).

## 2. `.rigid_model` layout (all little-endian)

```text
u32  mesh_count                                   CONFIRMED  (exe 0x011D9D80 reads it and loops)
mesh_count x MESH                                 CONFIRMED
f32 x3 bbox_min, f32 x3 bbox_max                  CONFIRMED  (0x011D9D80 reads 2 vec3 after the loop
                                                             and multiplies them by the model scale)
end of file                                       CONFIRMED  (all shipped files except 5 testdata, below)
```

### 2.1 MESH

```text
[u32 0x12345678, u32 version]   optional          CONFIRMED  0x01104810: reads a u32; if it is
                                                             0x12345678 it reads the version,
                                                             otherwise it seeks back 4 bytes and
                                                             the version stays 0
MATERIAL                        (2.2)             CONFIRMED  0x012234A0
u32  vertex_count                                 CONFIRMED
vertex_count x VERTEX           (2.3)             CONFIRMED  0x011B2AB0
u32  index_count                                  CONFIRMED
index_count x u32               triangle list     CONFIRMED  read as u32, stored as u16 by the game
                                                             (so indices are always < 65,536)
```

The version is **per mesh**, not per file. Shipped data:

| mesh version | meshes | vertices | vertex size |
|---|---|---|---|
| 0 (no marker) | 38 | 30,621 | 56 bytes (only the 5 `testdata\` files) |
| 1 | 172 | 29,144 | 72 |
| 2 | 3 | 485 | 72 |
| 3 | 49 | 13,172 | 80 |
| 4 | 1,967 | 1,435,456 | 80 |
| 5 | 12,921 | 11,840,723 | 80 |

The 5 marker-less `testdata\*.rigid_model` files end right after the last mesh, with **no
bounding box** (CONFIRMED from bytes). The exe would read past the end there; we believe the
game never loads them (INFERRED). The decoder accepts them and computes the box from the vertices.

### 2.2 MATERIAL (depends on mesh version): CONFIRMED from 0x012234A0 / 0x011B8520

A "string" is a `u16 length` followed by that many UTF-16LE code units (no terminator).

```text
version 0 or 1:
  string base_name                 e.g. "flagpole"
  -> the game builds <texture folder>/<base_name> + "_diffuse", "_normal", "_gloss_map",
     and "_dirty_map" (or "_snow_dirty_map" in snow)                         CONFIRMED (code)
version >= 2:
  3 x { u8 flag, string name }     diffuse, normal, gloss map                CONFIRMED
version >= 3:
  string extra                     4th texture, no flag byte; empty in nearly all files
                                   INFERRED: dirt map (same variable the v0/v1 path fills
                                   with "_dirty_map"; default "RigidModels/Buildings/default_dirtmap")
version >= 4:
  u32 n, n x { string name, f32 value }        named float shader constants  CONFIRMED
  u32 m, m x { string name, f32 x4 value }     named vec4 shader constants   CONFIRMED
```

- Texture names have **no `.dds`** and **no folder** (e.g. `eu_diffuse0`). CONFIRMED.
- Flag byte: 0 in 43,958 slots, 1 in 862. With flag 1 the name is a full path to a placeholder
  (e.g. `RigidModels\DummyTextures\dummy_normal`), and the code has a branch that substitutes
  `RigidModels/DummyTextures/dummy_diffuse|dummy_normal|dummy_gloss_map`. INFERRED meaning:
  "this slot uses a dummy texture".
- Float constants the exe looks for by name (CONFIRMED string compares): `bumpfactor`, `specpower`,
  `specfresnelpower`, `glossfactor`, `fresnelpower`, `reflect_factor`, `ambient_factor`, `rimoffset`,
  `rimpower`, `specbrightness`. Vec4: `rimcolor`, `specfactor`. Files also contain names the exe
  ignores (e.g. `light_scale`, `offsetu0`, `offsetv0`, `specularfresnelpower`).

### 2.3 VERTEX: CONFIRMED from 0x011B2AB0 (reads 0x38 bytes, then +16 if version != 0, then +8 if version > 2)

| offset | type | field | notes |
|---|---|---|---|
| 0 | f32 x3 | position | metres (INFERRED from building sizes), Y up, Direct3D left-handed |
| 12 | f32 x3 | normal | unit vector |
| 24 | f32 x2 | uv | texture coordinate; values outside 0..1 tile; V points down (D3D) |
| 32 | f32 x3 | tangent | |
| 44 | f32 x3 | binormal | |
| 56 | f32 x4 | "colour" | version >= 1 only. The game clamps each value to 0..1, multiplies by 255 and packs a D3DCOLOR (CONFIRMED). The version-0 default is (1, 1, 0, 1) (CONFIRMED). Meaning INFERRED (vertex colour / blend weights); 4.2 M of 13.4 M vertices have non-zero values. |
| 72 | f32 x2 | uv2 | version >= 3 only. Copied unchanged into the GPU vertex (CONFIRMED). INFERRED to be a second UV set (1.0 M vertices non-zero). |

Triangle winding: for every triangle checked, `(b - a) x (c - a)` points the same way as the stored
vertex normals (CONFIRMED on houses and props with `examples/rigid_info`). In D3D terms that means
clockwise front faces.

### 2.4 What the game builds on the GPU (runtime vertex, 44 bytes), from 0x012234A0

```text
+0  f32 x3  position * model scale
+12 D3DCOLOR normal    ((n + 1) * 127.5 per axis; x in byte 2, y in byte 1, z in byte 0)
+16 D3DCOLOR tangent   (same packing)
+20 D3DCOLOR binormal  (same packing)
+24 f32 x2  uv
+32 f32 x2  uv2
+40 D3DCOLOR colour    (clamped 0..1 * 255)
```
CONFIRMED (the stores to dword offsets 0..10). The matching `D3DVERTEXELEMENT9` array is built at
run time: the `re_tools vdecl` static scan finds **0** declarations in the exe's data sections, so
the D3D usage names (NORMAL, TANGENT, ...) per offset are INFERRED.

### 2.5 Render flags from the file name: CONFIRMED (0x011D9D80)

The group loader checks the model's **path**: `_alphatest` -> alpha test, `_alphablend` -> blended,
`_twosided` -> no back-face culling. A second parameter picks the mesh class: 0x13 standard rigid,
0x33 (a variant that also builds `_diffuse` lookup textures), 0x99 campaign settlement slots
(`_slot.rigid_model`, `_blend.rigid_model`), 0xA5 another class (UNKNOWN which).

## 3. Levels of detail (LOD) and texture folders: DB tables

Each LOD is a **separate file** (`..._lod01.rigid_model` is the most detailed, then `_lod03`, `_lod05`).
They are linked by three DB tables in data.pack (the layouts parse cleanly as all-string / string+float):

- `warscape_rigid_lod` (4,244 rows): `hash`, `model path`, `"N_i"` (= LOD i of N), `key`.
- `warscape_rigid_lod_range` (15 rows): `"N_i"` -> distance. With 3 LODs: 600, 1200, 0 (0 = everything
  beyond the last). With 5 LODs: 200, 400, 750, 1500, 0. Units INFERRED to be metres from the camera.
- `warscape_rigid` (2,238 rows): `key`, **texture folder**, category (`battle_building`,
  `campaign_building`, `campaign_template_extra`, `battle_misc`, `3Dindicators`, ...).

So: model path -> `key` -> texture folder -> `<folder>\<texture name>.dds`. The data relationship is
CONFIRMED (every model the viewer loaded resolved this way, e.g. buildings ->
`RigidModels\Buildings\AAA_TEXTURES`). That the exe uses exactly this route is INFERRED: the mesh
loader receives the folder as a parameter, and its callers are not fully traced. The viewer falls
back to `<model dir>\textures`, and then to a search by file name.

## 4. `.rigid_model_header`: CONFIRMED (0x011DA5A0)

```text
u32 0x000FEABC, u32 1, u8 animated, u32 vertex_count, u32 index_count,
f32 x3 bbox_min, f32 x3 bbox_max, [u32 extra, only if animated]
```
If the header file is missing, the exe estimates `vertex_count = file size / 84` and
`index_count = vertex_count / 3` (CONFIRMED). For 1,293 of 1,870 models the header totals equal the
model's own; the rest differ by about 10% (INFERRED: headers exported from a slightly older version
of the model). The meaning of the extra u32 in animated headers is UNKNOWN.

## 5. Textures (.dds): survey of all 8,239 in the packs

DXT5 4,850 (normal maps, diffuse with alpha), DXT1 2,406, DXT3 146 (one of them DXT2), uncompressed
32-bit 101, 16-bit 708 (R5G6B5 colour masks, L16 battle heightmaps), 24-bit 14, 8-bit 12. 8,237 decode
with `ntw_formats::dds`. The 2 failures are `testdata\gadgets.dds` (fourCC 36, A16B16G16R16) and
`ui\cinematicicons4.dds` (fourCC 63). Almost all model textures have full mip chains. Naming:
`<name>_diffuse`, `_normal`, `_gloss_map`; unit parts also have `_colour_mask` (faction colours). The
exe has format strings such as `%S_diffuse.dds` and `%S[%02d]_diffuse.dds`. No rigid_model material
we decoded references a `.tga` (TGA is used by UI and terrain).
UNKNOWN: whether normal maps are stored swizzled (DXT5nm) and which green-channel convention they
use. The viewer currently uses the diffuse textures plus vertex normals only.

## 6. Coordinate conversion used by the viewer

D3D is left-handed. To show models in Bevy (right-handed, Y up), the viewer negates Z of positions,
normals, tangents, binormals and the box, and swaps the 2nd and 3rd index of each triangle
(mirroring flips which side is in front). Verified visually: buildings render upright and not inside
out, with back-face culling on.

## 7. Units: `.unit_variant` (VRNT) and `.variant_part_mesh` (VMPF). First notes, not implemented

- `*.soldier|officer|musician|standard_bearer.unit_variant` (3,126 files, variantmodels.pack):
  `"VRNT"`, u32 0, u32 version (5: 223 files, 6: 3, 7: 7, 8: 1,497, 9: 12), u32 count (e.g. 20),
  u32 (e.g. 0xA64), then **fixed-size** UTF-16 slots (part names such as `hands`, padded with zeros
  to a fixed width). It looks like a table of body-part slots -> lists of part meshes and textures.
  INFERRED. Worker 2 has a started reader (`analysis/worker2/data_tools/src/variant.rs`).
- `mesh.variant_part_mesh` (299 files): `"VMPF"`, u32 0, u32 (0/1/2), u32 (0/1), then what look like
  per-LOD records `{u32 lod, u32 vertex_count, u32 index_count}` (e.g. `1, 0x66, 0x114 ... 2, 0x66, 0x114`)
  and 40-byte vertices using **half floats** (positions; UV `00 3c 00 3c` = 1.0, 1.0) plus packed
  bytes (`7f 80 ff 03`: normals or bone indices/weights). INFERRED; the exe's variant loader
  (`Warscape\Source\systems\variant`, `VariantNode`) should be traced next.
- `.variant_weighted_mesh` (data.pack, 286 files): skinned meshes (u32 count + UTF-16 name, no magic). UNKNOWN.

## 8. Not done yet

- `.rigid_model_animation` (destruction animations), `.animatable_rigid_model`, `.rigid_naval_model`.
- Normal and gloss maps, dirt maps and shader constants, for a material that matches `Textured_Rigid.fx`.
- Unit variant assembly (§7), skinning, `.anim`.
- Battle terrain (battleterrain.pack: heightmaps are L16 DDS) and the campaign map
  (`campaign_tiles`, `rigid_spline`; campaign mountains are ordinary rigid_models in rigidmodels.pack).

## 9. Exe functions referenced

| address | what | tag |
|---|---|---|
| 0x01104810 | optional `0x12345678` + version reader | CONFIRMED |
| 0x011D9D80 | rigid group loader: mesh count, per-mesh class/flags from the file name, bbox | CONFIRMED |
| 0x011DECE0 | constructor of one rigid mesh object; calls 0x012234A0 | CONFIRMED |
| 0x012234A0 | mesh reader: material, vertices -> 44-byte GPU vertices, u32 -> u16 indices | CONFIRMED |
| 0x011B2AB0 | stored-vertex reader (56/72/80 bytes by version) | CONFIRMED |
| 0x011B8520 | material reader used by other mesh classes (same layout) | CONFIRMED |
| 0x011DA5A0 | `.rigid_model_header` reader (magic 0x000FEABC) | CONFIRMED |
| 0x0121ADE0 | texture path helper (folder + name + suffix) | INFERRED |
| 0x012457D0 | a simpler single-mesh reader (28-byte vertices, same file layout) | INFERRED |

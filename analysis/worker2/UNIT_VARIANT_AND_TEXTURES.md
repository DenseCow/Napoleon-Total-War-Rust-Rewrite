# Unit variants, part meshes and textures (structure notes)

Tags: **CONFIRMED** = checked on every shipped file by a tool run; **INFERRED** = strong reading, not proven; **UNKNOWN**.
Tool: `analysis/worker2/data_tools` (Rust, read-only). Commands used: `variant`, `variant-survey`, `vmpf-survey`,
`atlas-survey`, `dds-survey`, `u32s`, `lattice`, `find-pair`, `db uniforms`. No game data is copied here; only structure
and counts. Run from a folder that has no `data` subfolder (or pass `data.pack` with the extension), because `pack::resolve`
treats an existing relative path as a file.

The `.rigid_model` format itself (buildings, props, ships, cannons) is owned by the graphics worker
(`crates/ntw_formats/src/rigid_model.rs`, `analysis/worker1/GRAPHICS_EXE.md`). Soldiers are **not** rigid models: they are
assembled at run time from `.unit_variant` part lists and `.variant_part_mesh` files, described here.

## 1. Where the files are (CONFIRMED, pack index)

| pack | contents |
|---|---|
| variantmodels.pack | 3,126 `*.unit_variant` (1,062 soldier, 1,060 officer, 730 musician, 274 standard_bearer), 299 `mesh.variant_part_mesh` (`unitparts\euro\...`, `unitparts\ottomans\...`, `unitparts\test_schema\...`, `equipment\mesh[2].variant_part_mesh`), 1,778 `.dds`, 304 `.atlas` (`units\atlas\`), plus a few leftovers (19 .tga, 14 .bmp icons, 6 thumbs.db, 3 .psd, 2 .max under `test_schema`) |
| variantmodels2.pack | 76 `units\atlas\<faction>_colour_mask.dds` (one per faction, uncompressed RGB565) |

## 2. The unit → model chain

```
units.key ("Art_Foot_Austrian_12_lber")
   └─ uniforms (DB, v0, 1,120 rows, schema str,str,str,str):
        col0 uniform key, col1 faction key, col2 variant name, col3 unit key (FK units)
          └─ variantmodels\units\<lower(variant name)>.<role>.unit_variant     role = soldier | officer | musician | standard_bearer
                └─ per body-part category: list of (mesh path, texture stem)
                      ├─ <mesh path>.variant_part_mesh               (VMPF, §4)
                      ├─ <texture stem>_diffuse.dds / _normal.dds / _gloss_map.dds / _colour_mask.dds   (§5)
                      └─ or a named equipment piece (flag = 1) inside variantmodels\equipment\mesh*.variant_part_mesh
   faction colours: uniform_to_faction_colours (uniform key, faction, 9 ints = 3 RGB colours), faction_uniform_colours (faction, 9 ints)
   equipment: warscape_equipment_themes (theme → primary/secondary weapon, equipment set, instrument), warscape_equipment_items
```
Evidence (CONFIRMED by a full cross-check):
- every `uniforms.col3` (1,120/1,120) is a `units` key;
- for 1,118 of 1,120 uniform rows both `<variant>.soldier.unit_variant` and `.officer.unit_variant` exist; 2 have only the soldier file;
- 63 rows have a uniform key different from the variant name (several factions share one variant);
- 4 of the 1,062 variant stems are not referenced by any uniform (e.g. `copy of portugal_inf_skirm_portuguese_tiradores`, an obvious leftover).
- The variant stem is usually `<faction>_<unit key>` in lower case (`austria_art_foot_austrian_12_lber`), but the code must go through
  `uniforms`, not build the name (INFERRED: that is what the table is for).

How the game picks which of several meshes in a category a soldier gets (random per man? seeded by the battle RNG?) is UNKNOWN
(exe side).

## 3. `.unit_variant` ("VRNT") — CONFIRMED on all 3,126 files

All integers little-endian. Parsed strictly (every file ends exactly where the layout says).
```
0x00 [4]  "VRNT"
0x04 u32  version            0 in all files
0x08 u32  category_count     5..11 (8 in 1,497 files, 10 in 1,306)
0x0C u32  header_size        20 in all files (= offset of the category table)
0x10 u32  mesh_table_offset  = 20 + category_count * 528 in all files
0x14      category_count x CATEGORY (528 bytes)
            [u16; 256] name     UTF-16LE, NUL-padded (bytes after the NUL are all zero)
            u32 index           == position in 27,170 of 27,184 categories (INFERRED: a stable slot id; the 14 exceptions are UNKNOWN)
            u32 unk             0 in all files
            u32 mesh_count      may be 0 (category present but empty)
            u32 first_mesh      categories cover the mesh table contiguously, in order (27,184/27,184)
mesh_table_offset: N x MESH (1,026 bytes), N = (file size - mesh_table_offset) / 1026 exactly
            [u16; 256] mesh     path, '/' separators, no extension, mixed case
            [u16; 256] texture  texture *stem*, no suffix
            u16 kind            0 = part mesh + texture (21,229 entries); 1 = named equipment piece (12,760 entries)
```
Category names (case varies: "Heads", "hats"): hands, heads, hats, legs, torsos (in every file), belts, plumes, shoulders,
equipment_primary_weapon, equipment_secondary_weapon, equipment_ambient, equipment_personal, and an empty name (115 times).

Mesh references:
- kind 0: `mesh + ".variant_part_mesh"` resolves in variantmodels.pack for **21,229/21,229** entries; the texture stem resolves
  (at least one file starting with it) for 21,229/21,229.
- kind 1: the mesh field is a bare piece name such as `rigid_equip_euro_straightsabre01` and the texture is empty (12,760/12,760).
  These names occur as piece names inside `variantmodels\equipment\mesh.variant_part_mesh` and `mesh2.variant_part_mesh` (§4.3).

## 4. `.variant_part_mesh` ("VMPF")

### 4.1 Header and LOD walk — CONFIRMED by size accounting on all 297 single-part files
```
0x00 [4]  "VMPF"
0x04 u32  0                       (all 299 files)
0x08 u32  vertex format           1 = 40-byte vertex (267 files), 0 = 64-byte vertex (30 files, the plumes), 2 = equipment container (2 files, §4.3)
0x0C u32  attachment_count        0 or 1 (1 in 89 files: hats, whose attachment is named "plumes")
0x10 u32  lod_count               4 in 294 files, 1 in 3
0x14 u32  total_vertices          sum over LODs
0x18 u32  total_indices           sum over LODs
0x1C u32  scalar_param_count      0, 9 or 13
0x20 u32  vector_param_count      0 or 2
0x24      lod_count x LOD { u32 V; u32 I; V x vertex (40 or 64 bytes); I x u16 index }
          attachment_count x ATTACHMENT (100 bytes): [u16;32] name ("plumes") + 36 bytes (INFERRED: a transform, e.g. 9 floats; UNKNOWN)
          scalar_param_count x { [u16;32] name; f32 value }                         (68 bytes each)
          vector_param_count x { [u16;32] name; u16 n; n x u16 name again; f32[4] } (e.g. "colourmapfactor", "specfactor")
```
Walking the LODs from 0x24 with these sizes, the per-LOD V/I sums equal the header totals **and** the remaining bytes are exactly
`attachments*100 + scalars*68 + vectors` (826 B for 9+2 params, 1,098 B for 13+2, 0 B for files without material) in every one of
the 297 single-part files. The scalar names seen: light_scale, bumpfactor, specpower, specbrightness, specfresnelpower, glossfactor,
fresnelpower, reflect_factor, ambient_factor (the 13-param files add four more; not listed here).

LOD order: the first LOD block is the *smallest* in the files checked (belt_1: 68 of 196 vertices; head_austrian: 395 of 1,173)
(INFERRED: LODs are stored low → high detail, or the first block is a lowest-detail/shadow mesh; UNKNOWN which).

### 4.2 Vertex layout — partly INFERRED
40-byte vertex (format 1), from hand-reading belt_1 and head_austrian: the data uses **half floats** (0x3C00 = 1.0 appears as
`00 3C 00 3C` at byte 20 of every vertex; GRAPHICS_EXE.md §7 reads it as a UV of (1, 1)), there are packed bytes that look like a normal/tangent (`xx xx xx FF`, D3DCOLOR style) and
probable bone indices/weights. The exact field map, and the 64-byte format 0, are UNKNOWN. The exe's vertex declarations
(`re_tools vdecl`, GRAPHICS_EXE.md, graphics worker) are the right way to pin this down.

### 4.3 Equipment containers (format 2) — INFERRED
`variantmodels\equipment\mesh.variant_part_mesh` (150 pieces, 14,311 vertices / 41,541 indices in the header) and `mesh2` (34 pieces).
The first piece's name ("Reference") appears at 0x24 as a fixed UTF-16 field, and piece names like `rigid_equip_east_rifle02_lod1`
follow each piece's index list. So a container is a list of named pieces with LOD suffixes; `.unit_variant` kind-1 entries name them
without the suffix. Exact piece record layout: not walked yet.

## 5. Textures

### 5.1 Texture stems and suffixes (CONFIRMED, variant-survey)
A `.unit_variant` texture field is a stem. The files that exist for a stem:
| suffix set | mesh entries using it |
|---|---|
| `_diffuse`, `_normal`, `_gloss_map`, `_colour_mask` (.dds) | 14,232 |
| `_diffuse`, `_normal`, `_gloss_map` | 1,866 |
| numbered variants `[01]_diffuse` … `[16]_diffuse`, `[NN]_normal`, plus one `_gloss_map` | 5,131 (heads/skin sets: one texture per face) |

`_colour_mask` is INFERRED to drive faction colouring (the three RGB colours of `uniform_to_faction_colours`).

### 5.2 Per-faction texture atlases (CONFIRMED structure)
`variantmodels\units\atlas\<faction>_{diffuse,normal,gloss,colour_mask}.atlas` (76 factions × 4 = 304 files) and matching big DDS
sheets: `<faction>_diffuse.dds` / `_normal.dds` / `_gloss.dds` in variantmodels.pack (DXT5, 4096×4096 diffuse/normal), and
`<faction>_colour_mask.dds` in variantmodels2.pack (RGB565 uncompressed, 4096×4096 or 2048×2048).
```
.atlas: u32 version (1), u32 0, u32 count, then count x 1,048-byte entries:
        [u16;256] key   (texture stem + channel, e.g. ".../tex/texture_colour")
        [u16;256] source file (".../tex/texture_colour_mask.dds"); identical to the key in 3,119 of 16,262 entries
        f32 u0, v0, u1, v1   normalised rectangle in the sheet (all inside [0,1])
        f32 width, height    source size in pixels (512, 384, 256, 128, 32 ...)
```
File size == 12 + count × 1,048 in all 304 files. INFERRED: at battle time the game draws soldiers from the per-faction sheet
(one texture bind per faction) rather than from the loose per-part DDS files, which are the source art.

### 5.3 DDS survey (CONFIRMED, every `.dds` in every pack: 8,239 files)
| format | files | MB | where |
|---|---|---|---|
| DXT5 | 4,850 | 4,562 | everywhere (buildings 2,362, variantmodels 1,036, rigidmodels 746, data 598) |
| DXT1 | 2,406 | 1,143 | everywhere |
| L16 (16-bit luminance) | 632 | 785 | battleterrain.pack 623 (heightfields, see worker5 BATTLE_TERRAIN.md), data.pack 9 |
| DXT3 | 145 | 40 | data (UI) 104, rigidmodels 35 |
| A8R8G8B8 | 96 | 206 | rigidmodels 54, data 22 |
| R5G6B5 | 76 | 2,192 | variantmodels2 (faction colour masks) |
| R8G8B8 | 14 | 50 | rigidmodels, boot |
| L8 | 11 | 56 | rigidmodels 10, data 1 |
| X8R8G8B8 | 5 | 4 | |
| A8, DXT2, D3DFMT 36 (A16B16G16R16), 63 | 1 each | | data.pack / rigidmodels |

- 8,235 are 2D, 4 are cube maps; no volume textures; no DX10 header.
- File size is exactly `128 + mip chain` for 8,214 files; 22 have 8–32 trailing bytes; 3 are formats the tool does not size.
- 1,392 are not power-of-two (e.g. 1025×1025 L16 heightfields, 384×384 part textures).
- Common sizes: 512² (2,397), 256² (1,660), 384² (700), 1024² (624). Mip counts 1..13 (720 files have no mips).

Bevy note: DXT1/3/5 map to BC1/2/3 and load directly; L16, RGB565 and the rest need a small conversion on load (INFERRED from the
formats; how the engine samples L16 heightfields is the terrain worker's question).

## 6. Open questions
- Vertex field map of the 40- and 64-byte formats; meaning of the 36-byte attachment payload; the LOD order.
- How a soldier's parts are chosen from a category (exe side), and how colour_mask + faction colours combine in the shader.
- `.unit_variant` category `index` values that differ from their position (14 cases).
- The full piece record layout of the equipment containers.

## 7. Relation to `analysis/worker1/GRAPHICS_EXE.md` §7 (graphics worker's first notes)
GRAPHICS_EXE.md §7 was written from a few hex dumps and is marked "first notes, INFERRED". The strict parsers here (every
file parsed to EOF) give a different reading of some header words; this file supersedes those parts:
| GRAPHICS_EXE.md §7 says | checked reading (this file) |
|---|---|
| VRNT 0x08 = "version" (5, 6, 7, 8, 9) | 0x08 = **category_count**; 0x04 is the version (0). The counts 5/8/9… match the category histogram in §3 |
| VRNT 0x0C = "count (e.g. 20)" | 0x0C = **header size / category table offset**, 20 in every file |
| VRNT 0x10 = "e.g. 0xA64" | 0x10 = **mesh table offset** = 20 + count·528 (0xA64 = 20 + 5·528) |
| VMPF u32 (0/1/2), u32 (0/1) | 0x08 = **vertex format** (0 = 64-byte, 1 = 40-byte, 2 = equipment container); 0x0C = **attachment count** |
| VMPF per-LOD `{u32 lod, u32 V, u32 I}` | header has lod_count + totals; each LOD block is `{u32 V, u32 I}` + vertices + u16 indices (sums == totals in all 297 files) |
The DDS survey numbers agree (8,239 files, same format counts; this file splits the 16-bit group into L16 and R5G6B5).

## 8. Corrections from the unit-models worker (2026-10-03)
See `analysis/units/ANIM_FORMAT.md` for details and tests:
- §4.1 LOD order: the **first** LOD block is the most detailed in 287 of 297 part files (CONFIRMED survey).
- §4.1 attachment: 100 bytes = `[u16; 16]` name + 4x4 f32 + u32 bone (not a 64-byte name + 36 bytes).
- §4.2 vertex layout: the 40-byte format is two-bone skinning with a bone-local position per bone; the 64-byte format is
  rigid (field maps in ANIM_FORMAT.md §4).
- §4.3 equipment containers: fully walked (piece = `[u16; 40]` name, i32 bone, V, I, 64-byte vertices, u16 indices).
- 25 kind-1 equipment names have no piece; the DB equipment themes resolve for every unit.

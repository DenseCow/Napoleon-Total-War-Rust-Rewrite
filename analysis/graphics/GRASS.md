# Battle-map grass

BACKLOG §2 "Grass". Code: `crates/ntw_formats/src/grass.rs` (placement, data), 
`crates/napoleon/src/terrain/grass.rs` + `grass.wgsl` (drawing), material description
`crates/ntw_formats/src/materials/vanilla/grass.toml`. Graphics are our own (CLAUDE.md); the
placement, the data read and the detail setting are the original's.

## Where I am

Placement, drawing, the detail setting, fade and wind are in. Left: the cost number below is for
the default setting only; the items tagged below.

## Data (CONFIRMED by files unless tagged)

- The grass map: `0x00ECAC80` asks the image loader `0x008D1340` for `<preset folder>grassmap` with
  no extension; the loader tries `""`, `.jpg`, `.tga`, `.dds`, `.png` in that order (the table at
  `0x015C1438`) and reads the first that exists (CONFIRMED; ours `ntw_formats::grass::find_grass_map`).
  Vanilla: a `grassmap.tga` on most presets; six (caribbean, empty_flat, hb_naval, hb_nile,
  indian_great_fortress, ottoman_great_fortress) ship only `grassmap.jpg` + `grassmap_alpha.jpg`
  (asserted by `every_grass_map_and_atlas_reads`; `every_jpeg_grass_map_loads` in
  `napoleon::terrain::grass` loads them).
- Decoding (CONFIRMED `0x008D1340`): a `.tga` goes to the exe's TGA reader `0x0086F5B0` →
  `0x004E2680`, which flips rows by the descriptor's bit 5 (clear = bottom-up) and columns by bit 4,
  and for a 32-bit, 8-alpha-bit image (`0x004CDCA0`: format 5) copies the stored B, G, R, A bytes
  as they are (`0x004D6210`). Every other name goes through D3DX (`0x008D1710` → `0x010A6890`,
  decoded to `A8R8G8B8`, not flipped). INFERRED: which formats D3DX recognises by content; ours
  decodes JPEG, PNG and DDS by signature and anything else as TGA. Every vanilla `grassmap.tga`
  is 32-bit, 8 alpha bits, columns left to right (asserted).
- `.jpg` alpha (CONFIRMED `0x008D1340`): for a `.jpg` the loader also reads the name less its last
  4 characters + `_alpha.jpg`, decoded to 8 bits by `0x008D1970` with `0x004D6D60`
  (`(76·R + 150·G + 28·B + 127) / 254`), and writes it into the alpha of texel (row `i`, column `j`).
  ORIGINAL BUG: it loops over the alpha image's own width × height and writes texel
  `j + i × colour width` with no bounds check, so a larger alpha image spills into other rows or past
  the pixels. Ours writes only the texels both images have (`GrassMap::apply_alpha`); every vanilla
  pair is the same size (`every_jpeg_grass_map_loads`), so vanilla is the same.
- Channels (CONFIRMED): `0x01248590` reads a texel's bytes +2, +1, +0, +3 as kinds 0..3 (the table
  `0x0141AE04`); the image is B, G, R, A in memory by both paths above, so R, G, B, A are the densities
  of g0..g3. Layout: read top-down, like the ground-type map (row 0 = the map's +y edge, column 0 its
  −x edge): CONFIRMED by data, its densities are most uniform within each ground type with no flip
  on 48 of 51 `.tga` maps (`tests/battle_terrain_install.rs`, `every_grass_map_and_atlas_reads`).
- Our open override: `grassmap.png` in the same folder is looked for first (in the exe `.png` comes
  last), so a mod's PNG wins over a shipped `.tga` or `.jpg` (same layout).
- `textures.xml grass_type` (`temperate`, `alpine`, `default_winter`...) names the atlas
  `battleterrain\grass\<type>.tai` (CONFIRMED `0x011D0A10` builds `BattleTerrain/Grass/` + type +
  `.tai`) and its DDS pages in the same folder.
- A picture's kind is its name's prefix `g3`/`g2`/`g1`, else 0 (CONFIRMED `0x0121E190`), matched
  ignoring ASCII case (CONFIRMED `0x004F4540`, which lowers `A`..`Z` on both sides); its size
  in metres is `_<width>_<height>` (mm) at the end of the stem, missing height 512, missing width
  1024 (CONFIRMED `0x011D0A10`, which stores the next-to-last token at the size pair's `+0` and
  the last at `+4`; `0x01248590` passes them in that order to `0x011EFBD0`, which writes them as
  the vertex's size, whose first value grass.fx spreads across the blade).
- `fx\grass.fx` (shipped HLSL text) gives the quad, fade, sway and colour rules; its numbers are in
  `grass.toml` (CONFIRMED by the file).
- `commontextures\jitter.dds`: the fade's dissolve pattern (CONFIRMED string in the exe; which
  coordinates sample it is grass.fx's: the corner weights).

## Placement (CONFIRMED `0x01248590`, `0x0124E070`, `0x01223060`, `0x011EFBD0`)

Described in `ntw_formats::grass::place_cell` and the pending write-ups
(`names\pending\grass.jsonl`). In short: per cell `blades_per_side²` tries from an LCG seeded by
`cx + cz·(level + 32)`; edge margin `5 × grass_blade_width / 2` (`grass_blade_width` 2.0,
`0x01839820`); a jitter of up to half a spacing; the kind by the texel's densities (`0x0124E070`)
and accepted when its density ≥ draw/257; the picture by `0x01223060`; the scale
`(draw·0.15/65535 + 1.05 or 0.75)·0.7`; only where the ground is above 0.25 m. Mirroring and the
phase come from the blade's own draw (`0x011EFBD0`).

- Float order: ours computes the spot, the margin test (in centred coordinates), the jitter, the
  texel, the accept test (`draw × 0.0038910506`) and the scale (`v − v·0.30000001`) in the exe's
  order and with its constants, so the f32 results match.
- INFERRED: the ground-gate height. The exe reads the height bilinearly from its level record
  (origin `+0x1C`/`+0x20`, scale `+0x2C`/`+0x30`, max indices `+0x0C`/`+0x10`, floats from `+0x34`)
  and keeps the blade when it is above 0.25; ours uses `BattleMap::height_at` (the level holding the
  point, bilinear). It is the same rule if that record's origin is half the map and its scale one
  over the spacing (the record's fill is not traced), and its interpolation order differs
  (`(h00(1−fx) + h01·fx)(1−fz) + …` against ours `h00 + (h10 − h00)·t`), which can change a
  blade's keep only within an f32 rounding of 0.25.

- The cell seed `cx + cz·(level+32)` repeats for cells (cx+32, cz−1) on level 0: kept 1:1 (it is
  the original's pattern, not a fault that contradicts its intent).
- ORIGINAL BUG (`0x01248590`): the grass-map row is read at `z / map width` instead of
  `z / map height`. The same function keeps the spot on the map with x against the level's width
  (its settings `+0x14`) and z against its height (`+0x18`), but both image reads (the grass map and
  the tint image) scale z by `1 / width` as well and never clamp the row: on a map taller than wide
  the row runs past the image, on a wider one the image's lower part is never read. That contradicts
  the function's own pairing of z with the height, so it meets the bar. Fixed (divide by the
  height). Every vanilla preset's level 0 is square (asserted over all presets by
  `every_grass_map_and_atlas_reads`), so vanilla is the same; a modded oblong map gets its grass
  where its grass map puts it.

## Detail setting (CONFIRMED)

`gfx_grass_quality` (registered default 2, `0x00403D20`) selects a row of the table at
`0x01419AB4`: radius 5 cells, cell 16 m, 16/24/28/32 blades per side for 1..4; 0 is off. The exe
lowers the setting at start-up while its grass would not fit a fixed memory budget (`0x0117B330`):
`5 · radius²` cell records (`0x011D19C0`, 48 bytes each, each with a vertex buffer for
`(blades_per_side + 1)²` blades) times `108 · (blades_per_side + 1)²` bytes (`0x011F9B70`: 4
vertices of 24 bytes and 6 16-bit indices per blade). Lifted in ours (CLAUDE.md, no engine limits). `NAPOLEON_GRASS_QUALITY=<0..4>` overrides it.

## Per frame (`0x01247E90`)

The camera's cell ± the radius, kept on the map, sorted by distance, built when missing. Ours
builds at most 24 cells per frame, nearest first, and drops cells that leave the window; a cell is
the same every time it is built. The original loops `num_grass_levels` levels (cvar `0x018396A0`)
with cells doubling per level: its default is UNKNOWN, so ours draws level 0 only (PROVISIONAL).

Wind: `0x01248580` sets grass.fx's `g_wind` = the wind's unit direction (+x when none) ×
`grass_wind_scale` (`0x0183A368`, 1.0). The gust (`g_gust`, CONFIRMED `0x01247E90`): with
`t = (level − 1) × 0.01010101`, it grows by `push × (low·(1 − t) + high·t) × dt`, then a positive
gust loses `gust / falloff × dt` and stops at 0. `low`, `high` and `falloff` are the exe's variables
`grass_gust_rampup_low` (registered `0x00441240`, stored `0x01839030`, 1.0),
`grass_gust_rampup_high` (`0x004411F0`, `0x01838A30`, 5.0) and `grass_gust_falloff` (`0x004411A0`,
`0x018383C0`, 1.5); ours reads them from `grass.toml` (`GrassLook`), so a mod can change them.
PROVISIONAL: the two weather inputs (we use the
battle's wind speed as `push` and the trees' strength on a 1..100 scale as `level`, sharing
`speedtree::battle_wind_target`) and the game→Bevy axes of the wind.

## Drawing (ours)

One mesh per (cell, atlas page), 4 vertices per blade at its foot; `grass.wgsl` spreads them into
a view-facing quad (grass.fx's rule), fades them over the last 35% of `78·(1 − 0.2·phase)` m with
heights counted 3 times and the jitter dissolve, sways them and adds the gust. Colour: the picture
added to the ground colour (colour map at the foot over the tiled ground texture), turning to the
ground colour as it fades. Built once per cell, no per-frame allocation; the material uniform
changes only while the gust is still settling (relative change > 1e-3).

- INFERRED: the corner weights 0.4/0.6 (sides) and 0.2/0.4 (foot/top) (prefilled by `0x011D13F0`,
  not traced; these make the quad span the blade's width centred on its foot and its height from
  the ground up, all multiples of 51/255); the tint image (read at the foot in `0x01248590`: we use
  the colour map, level 0).
- PROVISIONAL: grass.fx's `g_sin_elapsed` (we give the elapsed seconds); the lighting (sun +
  ambient on the ground normal, no lightmap, shadows or fog).
- UNKNOWN: `generate_grassmap_from_blendmap` (a map with no grass map draws no grass in ours).

## Cost (release build)

Measured 2026-11-11 on `--battle --battle-map hb_austerlitz` (Alpine_winter, 9 pictures on 1 page,
grass map 1024x1024, the default quality `radius_cells 5, cell_size 16, blades_per_side 24`; 786 men,
10 units), `NAPOLEON_FPS_SHADOWS=0`, `NAPOLEON_FPS_LOG=24`, mean of the 11 two-second samples at
t = 2..22 s:

| Build | Grass on | Grass off (`NAPOLEON_GRASS_QUALITY=0`) | Cost |
|---|---|---|---|
| release | 180.7 fps (5.53 ms) | 182.6 fps (5.48 ms) | ~0.06 ms, within run-to-run noise (samples 175..196 fps) |
| debug | 166.8 fps | 166.7 fps | none measurable |

Worst frames are 6.0..7.5 ms both ways. Every vanilla `grassmap.tga` is 1024x1024
(`every_grass_map_and_atlas_reads`, 51 maps, all square), so the z/width-versus-z/height ORIGINAL BUG at
`0x01248590` changes nothing on vanilla data; it matters only for a modded non-square map.

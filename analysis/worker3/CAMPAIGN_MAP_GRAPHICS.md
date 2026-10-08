# Campaign map graphics (nap_europe) — structure notes

Tags: **CONFIRMED** = checked in the bytes (a count means "on every such file"); **INFERRED** = strong reading; **UNKNOWN**.
Tools (Rust, read-only): `analysis/worker5/terrain_tools` — new `hexf`, `spln-survey`, `stp` commands that read the LOOSE files in
`data\campaign_maps\` in place. The logic files (`regions.esf`, `pathfinding.esf`, `trade_routes.esf`, `sea_grids.esf`, `poi.esf`,
`metadata.dat`, `*_lookup.tga`) are described in `WORKER3_REPORT.md` §5 and are not repeated here. Structure only; no map data
is copied.

## 1. Files (CONFIRMED, folder listing of `data\campaign_maps\nap_europe\display\`)
| path | count | format | role (INFERRED unless said) |
|---|---|---|---|
| `heightmap\heightmap.tga` | 1 | TGA type 3 (uncompressed grey), 8 bpp, **4096×2048**, size 18 + 4096·2048 exactly | terrain height |
| `supertexture\supertexture.stpi` + `.stpd` | 1 + 1 | custom tiled, zlib (§3) | the whole-map colour "virtual texture" (32768×16384) |
| `borders\eur_<region>_<n>.rigid_spline` | 77 | SPLN (§4) | region border lines |
| `roads\*.rigid_spline` | 247 | SPLN | roads |
| `rivers\NN.rigid_spline` | 84 | SPLN | rivers |
| `traderoutes\{trade_routes,land_trade_routes}.rigid_spline` | 2 | SPLN, 299 and 486 splines in one file | sea / land trade routes |
| `coastline\coastline_group0.rigid_mesh` | 1 | magic 0x12345678, version 5 (same family as `.rigid_model`) | coastline mesh (see GRAPHICS_EXE.md / `ntw_formats::rigid_model`) |
| `arrows\arrows.rigid_model`, `tradenodes\all_tradenode.rigid_model` | 2 | rigid_model | UI arrows, trade-node markers |
| `trees\campaign.rigid_trees` | 1 | magic "G@M=", u32 1, f32 1024.0, u32 6 (INFERRED: 6 tree models), then u32 length + UTF-16 model paths ("@RigidModels/CampaignTrees/Campaign_tr…") | tree instancing |
| `world.markers`, `bridges\bridge.markers` | 2 | u16-length UTF-16 "BASE_MARKER_REPOSITORY", then named entries with floats (same family as the battle `.markers`) | settlement/port/bridge markers |
| `environment\{spring,summer,autumn,winter}.lighting` | 4 | XML `SCENE_ENVIRONMENT` → `LIGHTING` (light_direction euler, light_colour, light_colour_scale, ambient cube colours, …) | per-season lighting |
| `detail\campaign_detail.dds` | 1 | 1024×1024, uncompressed 32 bpp, 11 mips | tiling detail texture |
| `detail\campaign_detail_mask.dds` | 1 | 1024×512 DXT3, 11 mips | detail mask |
| `detail\campaign_rocks.dds` | 1 | 512×512 DXT5 | rock texture |
| `fogmask\fogmask.dds`, `snow\winter.dds` | 2 | 4096×2048, 8 bpp, no mips (same size as the heightmap) | fog-of-war mask; winter snow cover |
| `snowmask\campaign_snow_mask_{summer,winter}.dds` | 2 | 2048×1024 DXT5, 12 mips | seasonal snow blend |
| `README.txt` | 1 | text | a CA artist note ("final UNIQUE display data for this campaign…") |

The other four maps (`nap_egypt`, `nap_italy`, `nap_spain`, `nap_tut`) have the same layout (W3 §5; the spline check below ran on
all five, the supertexture check on Europe and Italy).

## 2. Heightmap — CONFIRMED format, INFERRED meaning
8-bit grey TGA, 4096×2048 for Europe. `regions.esf` theatre bounds are (-410, -190)..(340, 195) logic units (W3), i.e. 750 × 385,
roughly the same 2:1 aspect (INFERRED mapping: pixel column = (x + 410) / 750 · 4096, rows likewise; origin and flip UNKNOWN).
The vertical scale is UNKNOWN; `regions.esf` also has a `map_heights` block (W3) that may be the logic-side copy.

## 3. Supertexture (`.stpi` index + `.stpd` data) — CONFIRMED on nap_europe and nap_italy
```
stpi:
  u32 levels (6) | u32 width (32768) | u32 height (16384) | u32 tile_size (512) | u32 tile_bytes (262144) | u32 6 (levels again?)
  levels x { u32 tiles_x; u32 tiles_y; tiles_x*tiles_y x TILE (20 bytes) }      finest level first, each level halves
      TILE: u32 offset_in_stpd, u32 size_in_stpd, u32 raw_size (262144), u32 tile_id, u32 value (UNKNOWN; e.g. 0xEB1CF400, maybe an average colour)
  u32 1 (trailer)
stpd:
  tiles back to back; a tile = 8 chunks { u32 compressed_size; u32 raw_size (32768); zlib stream (78 DA ...) }
```
Checks: Europe levels 64×32, 32×16, 16×8, 8×4, 4×2, 2×1 = 2,730 tiles; the walk ends exactly 4 bytes before EOF (the trailer);
every tile's chunks sum to 262,144 and end exactly at the tile's size; tiles are contiguous and cover all 124,681,432 bytes of
`.stpd`. Italy (16384×16384): 32×32 … 1×1 = 1,365 tiles, same result.

Tile pixel format: 262,144 bytes = 512×512 at 1 byte per pixel, which is exactly **DXT5 or DXT3** for a 512² tile (INFERRED;
DXT1 would be 131,072). Confirm by inflating one chunk; the std-only tools have no zlib yet (the game will need an inflate,
e.g. the `miniz_oxide` crate).

## 4. `.rigid_spline` ("SPLN") — CONFIRMED on all 903 files of the 5 campaign maps (and the 1 battle-terrain wall)
```
"SPLN" | u32 version (1) | u32 spline_count
spline_count x { u16 n; n x UTF-16 name; u32 flag (1 in all 2,469 splines); u32 point_count; point_count x f32[3] (x, y, z) }
```
Every file parses exactly to EOF. 899 files hold 1 spline; the two trade-route files per map hold 299 and 486.
Names are tagged by kind (text before the first ':'): `border` (250), `road` (468), `river` (181), `land` (872, land trade routes),
`Sea`/`sea` (610, sea routes), `none` (88). Example: "border:eur_austria:1".
Coordinates: x ∈ -19.5..11.3, z ∈ -6.7..8.7, y (height) ∈ 0..0.13; 1,127 splines lie entirely at y = 0; 65 are closed
(first point == last point). These are **display units, not logic units**: the Europe range (-13.9..11.3 × -6.7..6.2) is about
1/30 of the logic theatre (INFERRED scale ≈ 30; the exact transform is UNKNOWN, probably in the exe). The flag is constant, so its
meaning is UNKNOWN.

## 5. Region data
Region polygons, adjacency, settlements and slot positions are in `regions.esf` (`region_data` → shared vertices, per-region
`areas` with triangle `faces` and `outlines`/`connectivity`; W3 §5). That is the source for filled regions and for picking
(already readable through `ntw_formats::esf`). The `*_lookup.tga` (palette index per minimap pixel) maps the minimap to region
indices. Borders drawn on the map are the separate `borders\*.rigid_spline` lines (§4).

## 6. What a Bevy campaign map needs (plan)
1. Terrain mesh from `heightmap.tga` (18-byte header + raw bytes).
2. Colour: decode supertexture tiles (zlib + DXT) at a coarse level (8×4 or 4×2 tiles) first.
3. Lines: borders/roads/rivers from SPLN as line meshes (gizmos are enough at first).
4. Regions and picking from `regions.esf`.
5. Models (coastline, trade nodes, trees) through `ntw_formats::rigid_model` (now in main).

## 7. Open questions
- Display ↔ logic coordinate transform; heightmap vertical scale.
- Supertexture tile pixel format, the stpi `value` field and the repeated `6`.
- `.rigid_trees` instance records, `.markers` entry layout, `.rigid_mesh` v5 body.

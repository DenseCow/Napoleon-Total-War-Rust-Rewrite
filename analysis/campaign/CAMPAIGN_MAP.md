# Campaign map: formats, coordinates and the Bevy view

Worker: campaign-map (2026-10-03). Builds on `analysis/worker3/CAMPAIGN_MAP_GRAPHICS.md` (file
inventory, SPLN and supertexture layouts) and W3 §5 (`regions.esf`). Tags: CONFIRMED / INFERRED /
UNKNOWN; stand-ins PLACEHOLDER / PROVISIONAL.

Code: `crates/ntw_formats/src/campaign_map.rs` (readers), `crates/napoleon/src/campaign/` (view).
Probe: `cargo run --release -p ntw_formats --example campaign_probe -- <cmd>` with
`tree <path> [depth] [items]`, `hex`, `regions <map>`, `fit2 <map> [borders|rivers|roads]`,
`hfit <map>`, `hpng <map> <out.png>`, `tile <map> <level> <index> <out.png>`.
Run: `cargo run -p napoleon -- --campaign eur_napoleon [--screenshot shot.png]`,
`--campaign list`. `NAPOLEON_CAMPAIGN_CAMERA=x,z,distance` (logic units) sets the start view.

## 1. Where the files are
`data\campaign_maps\<map>\` and `data\campaigns\<campaign>\startpos.esf` are **loose files**, not in
any pack (CONFIRMED: none of the 1,419 loose files in `data\` is also in a pack). `main`'s `Vfs`
mounts packs only, so the readers go through `campaign_map::GameFiles` = Vfs first, then the loose
file under `data\`. When a Vfs with loose-file layers lands (parked `work/mod-loading`), the
fallback simply stops triggering.

## 2. Coordinates
- Logic map units: x east, z north; positions are Fixed20 in the startpos (W3, CONFIRMED).
- Map bounds `region_data` (-640,-320)..(640,320) for Europe; theatre (-410,-190)..(340,195).
- **Display units (splines, rigid models) = logic × 0.0254**, i.e. `DISPLAY_TO_LOGIC = 1/0.0254 =
  39.37`, no offset, no axis flip. INFERRED (strong): least-squares search of scale/offset/flip
  that puts border-spline points on `regions.esf` outline vertices gives 39.37 on Europe, Italy and
  Spain and 39.38 on Egypt, offset 0, same z sign (mean distance 0.18-0.6 units). The value is
  exactly inches-per-metre. Rivers drawn with this scale sit exactly on the rivers painted into the
  supertexture, and the settlement template models come out at a sensible size with the same scale.
- Bevy: `(x, height, -z)`, 1 Bevy unit = 1 logic unit (north = -Z, like the battle view).

## 3. Heightmap
- `display\heightmap\heightmap.tga`, 8-bit grey, covers the whole `region_data` bounds,
  **row 0 = north** (CONFIRMED by statistics: with that mapping sea triangles sample 0.3 on average
  and land 5.6; flipped, sea samples 7.1).
- Vertical scale **PROVISIONAL 0.02 logic units per step** (`HEIGHT_SCALE`). Not found in the files:
  spline y values and settlement slot y values do not correlate with the heightmap (r ≈ 0), so they
  are not terrain heights. 0.02 makes 255 ≈ 5.1 units, the highest spline y (0.13 display units).

## 4. Supertexture
- `.stpi` / `.stpd` layout as W3. Tiles are **DXT5** 512×512 (CONFIRMED: inflated tiles start with
  DXT5 alpha blocks, `01 00 49 92 24 49 92 24`, and decode to the painted map including the wooden
  table frame; north up, tile row 0 = north, row-major).
- RGB = map colour. Alpha = a mask of UNKNOWN use (low on land, high on sea); drawn opaque for now.
- The view uses the first level at most 4,096 px wide (Europe level 3 = 8×4 tiles; Italy level 2).
  PROVISIONAL: no streaming of finer levels near the camera.

## 5. regions.esf (parsed by `RegionMap`)
`theatres_and_region_keys` (theatre bounds, region label positions), `region_data` (shared
vertices, bounds, 101 regions for Europe: key, "land"/"sea", bbox, `areas` with triangle `faces` and
`outlines`, then `settlement_and_slots` {position, footprint, `slot_descriptions` v1 {key, slot type,
Coord3d position, Coord2d, Angle, ...}}), `trade_nodes`. Slot types seen on Europe:
`settlement_1..4_slot`, `settlement_fortification`, `settlement_road`, `settlement_prestige`, `port`,
`town-commercial/industrial/intellectual`, resource slots (`gold`, `iron`, `timber`, `wheat`, ...).
`RegionMap::region_at(x, z)` does point-in-triangle picking (Paris → `eur_france`).

## 6. Camera (CONFIRMED tweaker defaults)
From `analysis/worker1/re_tools tweak` on `Napoleon.exe` (`CampaignCamera.cpp` 91-100,
`EmpireCampaignController.cpp` 70-71): min distance 15, max distance 100, tilt from -y between
0.5 and 1.0 rad, FOV 57.2957° (1 rad), pan speed 50, tilt speed 50. The startpos also has
`CAMPAIGN_CAMERA_MANAGER/CAMPAIGN_CAMERA` v2 {f32 0.5, u32 0, Coord3d (-54, 0, 151.12), bool}
(meaning UNKNOWN; the scripts move the camera at the start, e.g. `SetCameraTargetInstant(-215,-2)`).
INFERRED/PROVISIONAL: tilt linear in distance (1.0 rad zoomed in → 0.5 zoomed out), vertical FOV,
pan speed = distance per second, target clamped to the theatre, no rotation; start view on the
human faction's first army.

## 7. Models
- Settlements: `rigidmodels\campaignbuildings\templates\eu\eu_city_<n>_slot.rigid_model` where `<n>`
  is from the region's `settlement_<n>_slot` slot type. INFERRED from names; culture prefix `eu`
  PROVISIONAL (`ott`, `ind`, `na` sets exist), fortification models
  (`..._slot_fortifications_lvl0..`) not placed yet, rotation 0 PROVISIONAL. Scale ×39.37.
- Faction ID badges `campaignagenticons\agent_id_<faction>.rigid_model` float over armies and
  agents (INFERRED use). The army/navy/agent bodies are PLACEHOLDER shapes (cone/box/ball) in the
  faction's primary colour (`factions` table).
- Owner flags on settlements: PLACEHOLDER coloured banner.

## 8. Questions for Ghidra
1. The heightmap vertical scale (campaign terrain build; `heightmaps/default.tga` string at init).
2. The display↔logic transform constant (confirm 0.0254) and the settlement model scale/rotation.
3. CampaignCamera: tilt vs distance law, FOV axis, what pan speed 50 is per, clamping area, and the
   meaning of the startpos `CAMPAIGN_CAMERA` fields.
4. Supertexture alpha channel use, the stpi `value` field and the second `6`.
5. Settlement template choice (culture prefix, fortification level, rotation) and army/agent models.

## 9. Not done yet
(Trees, river ribbons, coastal surf, finer supertexture and port facing are done: §10.) Fortification and
resource-slot models, textured borders, the animated sea, per-season lighting (`environment\*.lighting`), fog of war, region picking/selection UI, labels, and the frontend hand-off (the
frontend page only needs to insert `CampaignStart { campaign }` and enter `GameMode::Campaign`).

## 10. Campaign visuals (worker campaign-visuals, branch `work/campaign-visuals`)
### Where I am
- Done: trees (§10.1) and the heightmap vertical scale (§10.1). The campaign FPS harness is `NAPOLEON_FPS_LOG=<s>`,
  and `NAPOLEON_CAMPAIGN_TREES=0` leaves the trees out.
- Next, in order: forts and settlement composition (§10.2 leads), river ribbons (`fx\campaignriver.fx`), the
  coastline mesh, borders, finer supertexture levels near the camera, and the facing of the 113 town and port models.

### 10.1 Trees (CONFIRMED format) and the heightmap scale (INFERRED, strong)
- `display\trees\campaign.rigid_trees` is read by `0x00966500`. Layout:
  - header: `"G@M="`, u32 1, two discarded u32 fields (1024.0 and 6), u32 model count (174 on every map);
  - per model: u16 n and an n-char UTF-16 path, then u32 group count;
  - per group: u32 instance count, vec3 min, vec3 max;
  - per instance: vec3 position (logic units), f32 scale.

  Each instance goes to `0x00914A30`. The scale multiplies the model's bounding box there, so it is the uniform
  scale from the model's display units: 35..38.5 on Europe, up to 51 on Italy, about 39.37 × 0.9..0.98. The exe
  turns each tree by a CRT `rand()` value; our angle is a fixed pseudo-random one from the position (PROVISIONAL).
  It also loads a `_low.rigid_model` variant for distance, with an UNKNOWN switch rule; we draw the full model.
- Tree counts: Europe 8,903, Italy 7,803, Egypt 1,553, Spain 3,095, tutorial 8,873
  (`campaign_map_install::all_maps_load`).
- **Vertical scale.** The tree heights are exact multiples of 1/51 = 5/255 where a tree stands on a pixel centre.
  A least-squares fit of tree height against the heightmap value under it gives 0.0191 per step on Europe and
  0.0187 on Italy. So `HEIGHT_SCALE` = 5/255, i.e. value 255 = 5.0 units (was 0.02, PROVISIONAL).
- FPS, debug build, Paris view at distance 30, four interleaved 14 s runs:
  - trees on: 440–514 and 311–413;
  - trees off: 523–535 and 445–488;
  - cost about 10–25 %.

### 10.2 Settlement composition leads (for the forts)
- `0x00A88420` builds the town model name: the template from DB `slots_templates_model`, then `_slot`, then
  `_minibuildings` or `_minibuildings_wealth<n>`.
- `0x00B42B90` builds the **settlement's fortification slot** model name from DB `slots_art` →
  `slots_templates_model` (folder at `+0x30`):
  `<template><n>_slot_fortifications_lvl<level>`. CONFIRMED by read (0-D round 9): it looks the
  `slots_art` row up by key, reads its **`+0x30`** column (#4, the template key), refuses to build a name when
  that string is not a key of `slots_templates_models`, and appends the literal **`_slot_fortifications_lvl`**
  plus the level. **It is NOT the region's fort** — the `fort` rows of `slots_art` carry no template key at all.
- **Forts: `FORT_ARRAY` is loaded (0-E, 2026-10-06); the model is now closed from the data (0-D round 9).**
  - The fortification building lives in `REGION_SLOT_MANAGER/FORTIFICATION_SLOT`, beside `ROAD_SLOT`;
    `ntw_campaign::world` reads it (`Region::fortification`).
  - `FORT_ARRAY` is a **child of `REGION`** (between `RELIGIOUS_MISSION_BUILDING_ARRAY` and
    `RESOURCES_ARRAY`), **CONFIRMED from the shipped data**: it is there in every shipped file.
    (The region reader `0x00A51E30` walking it there is the free model's reading; review 0-E found
    no kept decompile of it.) It holds **0 items in all eight `campaigns/*/startpos.esf` and all ten
    vanilla saves**, so nothing in the shipped data shows a fort.
  - Per item, the region reader reads one `u32` and then lets the fort object read a map position
    (two f32, fort +0x120) and a string (fort +0x188), plus two more version-gated strings
    (**INFERRED** -- review 0-E downgraded this from CONFIRMED: the decompile of `0x00AEB190` is
    not kept in `ghidra_evidence/0e`, and no shipped item exists to check it). The field meanings are UNKNOWN;
    `ntw_campaign::world::read_forts` reads them by shape. What a fort *is* is CONFIRMED from the
    loc: a garrison residence a general builds in his own region, which armies move into
    (`army_fort_Tooltip_12006e`, `campaign_map_tooltips_tooltip_line_armyplayer_fort` = "Right click
    to enter fort").
  - **The region's fort is a building in the chain `fFort`, and its model is `slots_art`'s `fort`
    row column #10 (`Fort_lvl1`, the same for all six cultures).** CONFIRMED from the install
    (0-D round 9, `crates/ntw_data/tests/slots_install.rs`):
    - `building_chains` has `fFort`; `building_levels` has **exactly three** rows in it —
      `fFort1_wooden_artillery_fort`, `fFort2_western_artillery_fort`, `fFort3_star_fort` at chain
      levels 0/1/2 (3/6/12 construction turns, 4000/8000/16000 cost, 0/10/20 military prestige).
    - the pack has **exactly three** `rigidmodels\campaignbuildings\buildings\generic\fort_lvl<n>_blend.rigid_model`
      files, n = 1..3, and three `ui\buildings\icons\fort_lvl<n>.tga`. So a fort at chain level *n*
      draws `fort_lvl<n+1>`, 1:1, with nothing invented.
    - six further models (`{eu,ind,ott}_fort_lvl4/5`) have **no** building level behind them —
      INFERRED battle-map or unused.
    - **Nothing can create a fort yet** (`BuildFort` is 0-B's), so this will not be visible in game.
  - Also open: the build cost rule beyond the table's three numbers, how many forts a region may
    have, and whether one needs the region to be attacked. Those are 0-B's / the AI's.
  - The rule for when the *settlement's* fortification slot is drawn (now closed from the data):
  - chain `sFortifications` has **two** levels (`sFortifications1_settlement_fortifications`,
    `sFortifications2_improved_settlement_fort`), so file level = chain level + 1 and `_lvl0` is the
    "not fortified yet" mesh;
  - model = `<folder>\<stem>_<n>_slot_fortifications_lvl<level>.rigid_model` beside the city template,
    e.g. `eu_city_1_slot_fortifications_lvl1.rigid_model`; only the **city** templates have them
    (`EU`/`IND`/`OTT`), and the **tribal `NA` folder ships none at all** — CONFIRMED over the pack.
- Resource slots: `slots_art` names a model per slot type and culture, e.g. `slot_resource_mining_gold_lvl0`, but the
  files are `..._gold_blend.rigid_model`. The name mapping is not decoded. `slots_art` and
  `slots_templates_models` now load (`ntw_data::campaign::{SlotArtRecord, SlotTemplateModelRecord}`).

### 10.3a Textured borders (0-E, 2026-10-06: the texture is found; the draw is not yet ported)

`rigidmodels\campaignborders\*.rigid_spline` is the border geometry we already read (Europe has 77
border splines), beside it `rigidmodels\campaignborders\textures\border_diffuse.dds`: **16,512 bytes,
128 × 128, DXT5/BC3** (interpolated 8-bit alpha; review 0-E corrected "4-bit alpha", which is
DXT3) (read from the install's header: `DDS `, 128 × 128,
`DXT5`, mip count 1). So the original's border is a **textured ribbon**, not a line, and the asset is
in the pack we can read -- `campaign::scene::spawn_lines` still draws borders (and roads) as untextured
`LineList` meshes in a flat colour, tagged PLACEHOLDER there.

Still UNKNOWN, so not attempted: the ribbon's **width** in logic units (rivers are 1.5, from
`0x0116E410`, but no border equivalent is traced), the **UV mapping** along and across the ribbon, the
alpha rule at the edges, whether it scrolls, and whether roads use the same texture (there is no
`campaignroads\textures\` folder in the pack, only `campaignborders`, so roads are probably untextured
or share it -- UNKNOWN). Reading the border draw would mean the exe function that loads
`border_diffuse.dds`; not traced.

#### 10.3a-1 Round N+2 (0-E, 2026-10-06): the spline header, and one shipped ribbon mesh

**CONFIRMED (data, self-consistent arithmetic): the `.rigid_spline` header.** `pack_probe hex
"rigidmodels\campaignborders\france_1.rigid_spline"` starts

- `"SPLN"`, u32 1, u32 1,
- a **UTF-16LE** name: `border:France:1` (so each file names the region and the border segment),
- u32 1, u32 **139**,
- then 139 records of **12 bytes** = three little-endian f32 (the first triple is
  `-0.0004389, -0.0007576, 3.2704`, i.e. logic units on the map).

52 bytes of header + 139 x 12 = **1720**, exactly the file's length. So the centreline is 139 map
points and the file carries **no width, no UV and no material** -- the ribbon is generated by the exe
from this centreline, which is why round N+1 could not close the width.

**NEW LEAD -- a pre-generated border ribbon mesh ships in the pack.** `testdata\westerneuborders.rigid_mesh`
(39,858 bytes, `data.pack`) is **not** a `.rigid_model` (our `rigid_info` reader rejects it with
`UnexpectedEof`), but its bytes are the same vertex layout without the file header: repeated records
of three f32 positions in **map logic units** (`-0.6699, 0.0, 692.63`), then three f32 reading
`(0, 0, 1)` (an up normal), then two f32 reading `(0, 0)`. That is a **border ribbon with its UVs**,
for western Europe. If a later round can parse it (the record stride and the trailing 8 bytes per
vertex are not yet pinned), the ribbon's **width** and **V mapping** become measurable from the data
instead of from the exe -- which is the whole open question above.

### 10.3a-2 `testdata\westerneuborders.rigid_mesh` -- parsed, and **it has no UVs either** (0-E round 4)

**This section corrects the lead above.** The lead's reading of the record (three f32 positions "in
map logic units", then `(0, 0, 1)` as an up normal, then `(0, 0)`) was a phase slip: the file's
positions are *not* `(-0.6699, 0, 692.63)` and they are not map logic units, and the two trailing
floats are **both** zero on every vertex -- there are no UVs to measure a `V` mapping from. The
reader is `ntw_formats::campaign_map::BorderRibbon`; the install test
`the_shipped_border_ribbon_is_geometry_only_and_parses_to_the_byte` prints it.

**CONFIRMED, and it closes on the byte:**

```text
u16 flags (0) | u32 vertex_count (586) | 586 × 56-byte vertex | u32 index_count (1758) | 1758 × u32
6 + 586×56 + 4 + 1758×4 = 39858      <- the file's exact length
```

The 56-byte record is the standard **version-0 rigid-model vertex**
(`rigid_model::vertex_size(0)`): position f32×3, normal f32×3, uv f32×2, tangent f32×3, binormal
f32×3. **1758 = 3 × 586** -- three indices per vertex, which is the six-per-segment unrolled list a
two-vertex ribbon needs (293 segments).

**Read off all 586 records:**

- `y` is exactly `0.0` on every vertex;
- the normal is exactly `(0, 1, 0)` (an **up** normal, not `(0, 0, 1)`) and the tangent and binormal
  are all zero;
- **both** texture coordinates are exactly `0.0` on every vertex -- CONFIRMED, the file carries **no
  UVs at all**;
- `x` spans `[-2.1924, 3.0935]` (span **5.2859**) and `z` spans `[19.7945, 23.8329]` -- so the vertices
  are **not** campaign map display units (which run to about 1000 and are scaled by
  `DISPLAY_TO_LOGIC` = 39.37008);
- the index list starts `0, 1, 2, 0, 3, 1, 0, 4, 3, 4, 5, 3` and ends `2, 1, 585, 2`: it is a **fan
  from vertex 0**, not a strip of `(2k, 2k+1)` pairs. So the "distance between consecutive pairs"
  is not the ribbon's width -- it ranges 0.0102 to 0.2013 (median 0.0554), i.e. no constant width is
  readable.

**What this changes, and why the border is still not drawable.** Round 3's plan -- "parse the rigid
mesh and the width is measurable" -- is **refuted**: this file has no UVs and no constant width, and
its coordinates are not the map's. So the border ribbon's width and its `V` mapping are still the
exe's job, and the two open leads are unchanged and now named precisely:

1. the **border** ribbon builder in the exe (the coast ribbon builder is `0x011C51B0`; the border one
   is the sibling that takes a `border:` spline and a width) -- it generates the width and the `V`
   from the distance along the centreline, exactly as `0x0111A080` does for rivers (which *is*
   CONFIRMED: 1.5 logic units, §10.3);
2. `fx\campaignborder.fx` (or whatever the border's shader is called), which is where the
   `border_diffuse.dds` mapping is defined. `rigidmodels\campaignborders\textures\border_diffuse.dds`
   is a shipped 128 × 128 DXT5, so borders *can* stop being flat lines; the mapping is the only
   missing piece.

**Not drawn, and the reason is evidence rather than buildability** (unlike the note above, which was
buildability): this round *could* compile `crates/napoleon` (`cargo check -p napoleon` clean, and
`cargo test --workspace` exit 0), so the blocker is gone -- but dropping this mesh into the map scene
would draw a 5-unit ribbon in the wrong place with no UVs, which is a fiction rather than a port.

### 10.3a-3 The border's shader, texture and uniforms are named in the exe (0-E round 5) — and roads are textured after all

Round 4 left two named leads. **Lead 2 is now closed**, and **the recorded negative about roads is
wrong**.

The exe's constant pool holds the engine's own **shader/material declaration table**: a run of
NUL-separated strings in `.rdata` at VA `0x01418500` that pairs a material name, a `.fx` file, its
texture paths and its uniform names. Read verbatim (`| ` = NUL):

```text
supertexture_border | SupertextureTile.fx | RigidModels/CampaignBorders/Textures/border_diffuse |
g_border_colour_a | g_border_colour_b | t_border_diffuse |
overlay | campaignoverlay.fx | Campaign/Farms/ | _summer_diffuse | _winter_diffuse |
t_summer_diffuse | t_winter_diffuse |
sm_campaign_river | CampaignRiver.fx | river_distortion | t_distortion | g_length | g_tile_factor |
supertexture_road | RigidModels/CampaignRoads/Textures/primitive_diffuse |
RigidModels/CampaignRoads/Textures/dirt_diffuse |
RigidModels/CampaignRoads/Textures/stone_diffuse |
RigidModels/CampaignRoads/Textures/tarmac_diffuse
```

**So, CONFIRMED:**

- **The border is a supertexture ribbon**: `SupertextureTile.fx`, textured with
  `RigidModels/CampaignBorders/Textures/border_diffuse` (the shipped 128 × 128 DXT5 this section
  already records), and tinted by **two** colours, `g_border_colour_a` and `g_border_colour_b`. Two
  colours on one ribbon is the first hard evidence for *how* it is built: a frontier is very likely
  two bands, one per side, or one ribbon per side. **INFERRED**, and it is the most useful thing here
  for the next round.
- **Roads are textured too, with four materials** — `primitive`, `dirt`, `stone` and `tarmac`
  diffuse. **This corrects the negative above**, which said "there is no `campaignroads\textures\`
  folder in the pack". There is: `rigidmodels\campaignroads\textures\dirt_diffuse.dds` and
  `...primitive_diffuse.dds` both ship in `rigidmodels.pack` (`pack_probe list "Textures\"`),
  and the exe's table names all four.
- **Rivers** are `sm_campaign_river` on `CampaignRiver.fx` with uniforms `river_distortion`,
  `t_distortion`, `g_length` and `g_tile_factor`; §10.3-1 has the pixel function and the two scroll
  rates. **The river's four shipped textures** are `rigidmodels\campaignrivers\textures\{river_diffuse,
  river_distortion, river_normal, river_surface}.dds`, of which the shader samples only the diffuse.

**What is still the exe's job, and why nothing was drawn this round:**

- **Lead 1, the border ribbon's geometry** -- the width and the `V` mapping -- is untouched, and
  round 4's refutation of the pre-generated mesh still stands. But the *shader* is no longer part of
  the unknown: a later round that has the width builds `SupertextureTile.fx`'s border technique
  against the shipped texture, with the two colours named.
- **The campaign sea.** `Ocean.fx` is in the same table but under `ADIAL_MESH`, i.e. the **battle**
  sea: its uniforms are `g_sea_uv_scale`, `g_swell_uv_scale`, `g_sea_deep_colour`,
  `g_sea_shallow_colour`, `g_fresnel_R0`, `g_sea_decay`, `g_sea_shininess`,
  `g_reflection_flattening_factor`, `g_froth_value`, `g_froth_distortion_speed`,
  `g_froth_distortion_amount`. The campaign map's own shaders in the table are `CampaignTerrain.fx`
  (trade routes: `g_trade_route_colour`, `g_trade_route_length`, `g_trade_route_smooth_a/b`),
  `campaignoverlay.fx` (farms, by season) and `CampaignRiver.fx`. **No campaign sea shader is named in
  the table**, so the animated campaign sea remains **UNKNOWN** and is left alone: the two-layer
  scrolling water in `Grid.fx` (`g_water_direction_1/2`, `g_water_offset_1/2`, `g_water_scale_1/2`)
  belongs to the battle terrain grid, and borrowing it for the campaign map would be an invention.
  **Named lead:** the campaign map's sea surface is drawn by something this table does not name --
  most likely a hard-coded technique rather than a material, so the next place to look is the
  campaign scene loader rather than the material declarations.

**Not attempted, notes only, and the reason was buildable-code, not effort** (superseded by
§10.3a-3): textured borders and the animated sea/rivers are both `crates/napoleon` work, the sandbox
**could not build `crates/napoleon`** at the time (`HANDOFF.md` round 1: "STATUS_ACCESS_VIOLATION
on a fresh target"), and the worker rules forbid `cargo build/run -p napoleon`. A ribbon generator or a
custom material written there would be code nobody could compile before it lands, so neither was touched.
**`crates/napoleon` now checks and tests clean in this worktree** (round 5), so that reason is gone;
what stands in its place is positive evidence for the borders and the rivers (§10.3a-3, §10.3-1) and
an UNKNOWN for the campaign sea.

### 10.3 Spline curves and river ribbons (CONFIRMED rules, simplified drawing)
- **Splines are cubic Bezier curves.** Every `.rigid_spline` on Europe (84 rivers, 77 borders, 218 roads) has 3k+1
  points. The river builder `0x0111A080` reads them as k segments of four control points, evaluates them with
  `0x005AFED0`, and scales display units by **39.37008**. That constant is now CONFIRMED in the exe, so
  `DISPLAY_TO_LOGIC` is CONFIRMED. All lines are now drawn along the curves (8 samples per segment).
- **River ribbons.** The ribbon is 1.5 logic units wide: `0x0116E410(…, 1.5)` builds it from −0.75 to +0.75.
  The texture is `display\rivers\textures\river_diffuse.dds`. `fx\campaignriver.fx` (CONFIRMED source) does this:
  - samples the texture at `(0.875 v, 0.1 s)` and `(0.7 v, 0.15 s)`, where `s` is the distance along the river and
    `v` = 0..1 across;
  - scrolls both samples with time and averages them;
  - alpha = `saturate(s) · saturate(len − s) · (1 − |2v − 1|)^0.2`;
  - lights with an up normal;
  - draws at height 0 with the depth test off.

  `campaign::scene::spawn_rivers` takes the first sample only, with no scrolling. The alpha goes in vertex colours
  over five vertices across. The ribbon lies on the terrain, lifted a little and depth-tested (PROVISIONAL).
- FPS with rivers (debug, Paris view): 440–510, no measurable cost.

#### 10.3-1 The scroll rates are CONFIRMED too (0-E round 5) — the animation is one line, not a question

`fx\campaignriver.fx` **ships** (3,227 bytes, `data.pack`), so the two scroll rates do not have to be
read out of the exe at all. The pixel function is, in full and unchanged:

```hlsl
float tile_factor = 1;
float timescale   = 1;
float2 tex    = float2(v.tex.y * 0.35, v.tex.x * g_length * 0.10);
float2 tex_a  = 0.5 * float2(5, tile_factor * 2) * tex + float2(0, timescale * 0.02 * g_elapsed);
float2 tex_b  = 0.5 * float2(4, tile_factor * 3) * tex + float2(0, timescale * 0.05 * g_elapsed);
float4 diffuse = lerp(tex2D(s_diffuse, tex_a), tex2D(s_diffuse, tex_b), 0.5);
```

Which settles the three numbers the §10.3 note above could only describe in words:

- the across-ribbon frequencies are **0.875** and **0.70** (they are `0.5 · 5 · 0.35` and
  `0.5 · 4 · 0.35`, not chosen constants -- that is where the note's figures came from);
- the along-ribbon scale is **`0.10 · g_length`**, so `s` is the distance along in logic units divided
  by the river's own length, exactly as `spawn_rivers` already assumes (`uv.push([v * 0.875, dist[i] * 0.1])`);
- **the two scroll rates are `0.02` and `0.05` texture units per second**, times `g_elapsed`.

`tile_factor` and `timescale` are **shader-local `= 1`**, not uniforms -- so nothing outside the shader
can change them, and the rates above are the original's, not ours to tune. `g_length`, `g_elapsed`,
`g_tile_factor`, `t_distortion` and `river_distortion` are the shader's declared uniforms (the exe's
material table names `sm_campaign_river` → `CampaignRiver.fx` → `river_distortion`, `t_distortion`,
`g_length`, `g_tile_factor`); `DECLARE_TEXTURE(normal)` and `DECLARE_SRGB_TEXTURE(distortion)` are
declared and **never sampled** in this revision, so the shipped `river_normal.dds` and
`river_distortion.dds` do no work in the original either.

**So the whole remaining river gap is one line:** offset the second texture coordinate by
`0.02 · t` and `0.05 · t` in a material of our own. `StandardMaterial` cannot offset a UV, so this
needs a small `Material` impl; it is not attempted this round (§10.3a-3 says why) and nothing about it
is a research question any more.

### 10.4 Coastal surf (CONFIRMED format and shader, still frame)
- `display\coastline\coastline_group<n>.rigid_mesh` holds the surf strips, not the coast itself. The scene loader
  `0x011C73A0` loads groups 0, 1, … until one is missing and draws them with `CampaignTerrain.fx` technique
  `sm3_campaign_coast`. `0x011C51B0` reads them. Layout (all five maps parse to the byte):
  - header: u32 0x12345678, u32 5, 19 vertex-format bytes;
  - u32 vertex count, then 80-byte vertices: position @0, `tex` @24, `tex2` @72;
  - u32 index count, then u32 indices.

  The strips are 1 unit wide. `tex.v` runs 0/1 across; `tex2` = (distance along, strip length).
- The shader:
  - colour 0.65 grey, unlit, at height 0.1;
  - texture `Campaign\Coastline\waves` at `(0.5 · tex2.u, 1.7 · (tex.v − 0.5) + 0.5)`, wrapping along and clamped
    across;
  - alpha = two rolling wave samples blending the texture's r, g, b frames, raised to 1.25 and × 1.5;
  - end fade `min(sat(t / 0.15), sat((1 − t) / 0.15))`, with `t = tex2.u / tex2.v`.

  The foam front sits on the shore edge of each strip and rolls in and out with time.
- `spawn_coast` draws one still frame (PROVISIONAL). The alpha is baked from `0.25 r + 0.25 g + 0.5 b` and nothing
  moves, so on a screenshot it is only a thin light line on the shore edge (PSNR 50 dB against no surf). Animation
  needs a custom material.

### 10.5 Finer supertexture near the camera (PROVISIONAL distances)
- The terrain keeps one level for the whole map: level 3, 4,096 px wide, 3.2 px per unit. Near the camera,
  `campaign::detail` draws a patch over the same terrain grid vertices with a finer level:
  - level 1 (12.8 px per unit, 3 × 3 tiles = 120 units) within distance 50;
  - level 0 (25.6 px per unit, 4 × 4 tiles = 80 units) within distance 25.
- Each window's tiles are read from `supertexture.stpd` by offset (seek and read), then inflated and DXT5-decoded,
  all on the async compute pool. The original streams the same tiles; its own distances are UNKNOWN.
- FPS, debug, distance 15 over Paris: 330–490 FPS steady, worst frame 3–5 ms. The 90–100 ms first-second frame
  also happens without the patch (start-up).

### 10.6 Town and port facing (CONFIRMED by the data)
- Every port slot in `regions.esf` has a `dock` point. With θ = angle / 65536 · 2π, it lies at (−sin θ, −cos θ) × 1.3
  from the slot position in logic (x, z). That holds for all 63 ports on Europe, Egypt and Spain (Italy has none)
  to within 6° (`examples/slot_facing_probe`).
- So the model, whose dock side faces south at θ = 0, turns clockwise seen from above: a Bevy rotation of −θ about
  +Y. The view had +θ, which mirrored every port; at Porto the houses stood in the sea and the quay faced inland.
- Towns use the same slot angle (they have no dock point to check).
- Screenshots: `target/tmp/shots/porto_1.png` (old), `porto_-1.png` (fixed).

# Water: the battle sea, shorelines, and the campaign map's rivers and sea (§2 water slot)

Goal: the game **looks like the original**. Our own WGSL, written from what the original's shipped
shaders do. The original shaders are HLSL **source** in `data.pack` `fx\` (Creative Assembly's
files): they are read only to learn the behaviour (inputs, maths, constants) and are **never
committed or pasted**. Tags: CONFIRMED / INFERRED / UNKNOWN / BLOCKED; stand-ins PLACEHOLDER /
PROVISIONAL.

Read first: `analysis/graphics/SHADERS.md` §1 (the survey of all 136 shipped effects) and §2 (the
scene constants the engine registers by name at `0x01121D30`). This file only covers water.

Tools (both read-only, both in this repo):
- `cargo run -p ntw_formats --example water_probe -- [name]` — per battle preset: the level-0
  heightfield range, the ground-type histogram with each index's **editor palette colour**, and the
  share of the map under y = 0. Everything in §2 comes out of this.
- `cargo run -p ntw_formats --example sea_probe -- header <path>`: the header of the engine's own
  texture container (§3). The full decoder is `ntw_formats::sea_waves`.
- `cargo run -p ntw_formats --example pack_probe -- extract-prefix "fx\" <scratch dir>` — the
  shipped shader sources, into a scratch dir only.

## Where I am / what's next
- **DONE: the battle-map sea surface** (§4, §5). One flat sea plane at the confirmed sea level, a
  port of `fx\ocean.fx`, a camera-centred ring grid, the sea-bed depth blend, the shipped foam texture.
- **DONE 2026-10-11 (sea-water, after the user check of 2026-10-10: pale, too bright, long stripes,
  no foam):** the `sea\sea` / `sea\swell` container is decoded and animated (§3); every value the
  exe hands `ocean.fx` is traced to the per-draw setter `0x0119E830` and the two db tables (§4); the
  sky reflection evaluates the map's SKYGEN model (§4, PROVISIONAL).
- **NEXT, in this order** (§8): the original's CPU-displaced sea mesh (`0x01123820`), the sky cube,
  the shoreline/surf blend and the foam pass choice, then planar reflections, then the campaign map's
  river and sea shading.

---

## 1. What the original ships for water

From `SHADERS.md` §1, read and confirmed here against the sources:

| effect | what it is |
|---|---|
| `ocean.fx` (22.7 KB) | **the sea surface**. Two tiling wave textures, `sin/cos` normal rebuild, sun, Schlick, Blinn-Phong, foam, a sky cube-map reflection and (behind `ENABLE_REFLECTION`/`ENABLE_REFRACTION`) refraction. This is the file to port. |
| `seainteraction.fx` | the wake/ripple cards ships and troops leave on the sea (two scrolling diffuse textures, alpha-blended, no depth write, drawn on a decal plane). Instance data in `fx\FX_Sea_Interaction.h`. |
| `filterfoam.fx` | a one-pass foam-texture **generator**: it samples the foam texture twice (linear and point) and packs `r` from one and `gba` from the other. It feeds the particle system's foam, it is not shoreline surf. |
| `shipwake.fx`, `shipbowwave.fx`, `sea_particles.fx`, `navalship.fx`, `navalsail.fx` | ships: hull shading, sails, wakes, bow waves, spray. |
| `rope.fx`, `cloth.fx` | rigging and flags (not water, listed for completeness). |
| `naval.fx` | **not** the sea: it is the rigid-mesh shader for ship models (diffuse/normal/specular, damage-panel UV offset, alpha test 84). |
| `campaignriver.fx` | the campaign map's river ribbons — already recreated (`campaign::scene::spawn_rivers`, from the same source read). |

The sea is drawn from **one flat plane at the sea level** plus, optionally, render targets for
reflection and refraction. There is no per-vertex water body, no lake mesh and no river mesh on a
battle map: every body of water on a battle map is the same plane.

## 2. The sea level is y = 0 in map metres — CONFIRMED

`ntw_formats::battle_terrain::SEA_LEVEL = 0.0`, with the evidence in `SEA_LEVEL_NOTE`. Two
independent sources agree on all 60 shipped presets (`water_probe`, columns *typed %* = share of
cells the ground-type map marks `water_deep` 17 / `water_shallow` 20 / `water_medium_ford` 19, and
*below 0 %* = share of cells whose heightfield sample is under 0 m):

| preset | heightfield min…max | typed water % | under 0 % | highest water cell |
|---|---|---|---|---|
| `caribbean`, `hb_naval`, `hb_nile`, `hb_trafalgar` | −100.00…−100.00 (flat) | — | 100 % | — |
| `hb_toulon` | −134.94…129.26 | 47.50 % | 47.66 % | −0.00 m |
| `hb_pyramids` | −51.86…77.03 | 15.50 % | 15.90 % | −0.03 m |
| `rti_fort_5` | −65.50…52.91 | 38.01 % | 38.52 % | −0.00 m |
| `rti_fort_1` | −51.30…116.27 | 27.29 % | 27.50 % | −0.03 m |
| `hb_arcole` | −18.84…39.71 | 14.30 % | 16.30 % | +0.11 m |
| `hb_austerlitz` | −10.99…91.54 | 4.76 % | 5.76 % | +2.29 m |
| `hb_waterloo` | +6.05…83.90 | 0.00 % | 0.00 % | — |

- The four all-sea naval presets have a **flat** level-0 heightfield at exactly −100 m (`scale` 100,
  `bias` −100, every sample 0), so the surface cannot be at the sea bed and the only other round
  number in range is 0.
- On every preset that marks water, the **highest** water cell is at ≈ 0 m, and the share of cells
  under 0 matches the share of typed water to within 0.5–2 %.
- `sea_coverage() > 0.5 %` is the "draw a sea" test (`MIN_SEA_COVERAGE`), which is 100 % on the naval
  presets, 0 % on the land battles, and 0.01 % on `nap_mp_italian_grassland`.

**NOT the sea level (UNKNOWN, open):** inland water bodies at their own height. The lake of
`nap_mp_lakeside_[sa]` has typed water up to **+105.80 m** (mean +22.94 m), `nap_mp_valley_[sa]`
+21.97 m, and the Adda at `hb_lodi` +24.37 m. Those are rivers and lakes. The original's per-body
level is in **no map file** — there is no `sea_level` in `definition.xml`, `weather.xml` or any
`*.environment` (searched every preset). Recording the negative.

**INFERRED, not resolved:** the `[sa]` stand-alone maps share **one identical** `ground_type_map_0.tga`
(byte-for-byte the same histogram on all 16 of them, including `nap_mp_gorge_[sa]`, `nap_mp_island_[sa]`
and `nap_mp_coastal_[sa]`, whose maps have no water types at all but 26 %, 5 % and 23 % of their
ground below 0). It is a placeholder, so on those maps the height rule alone decides, and the basins
read as sea. The original draws one plane, so it very likely floods them too — but that is untested.

## 3. `sea\sea` and `sea\swell`, the wave textures: CONFIRMED (decoded 2026-10-11)

The sea's two wave textures are the only files in the install's `sea\` folder without an extension:

| file | bytes | size | tile metres | loop period s | frames | angle range lo (x, z) / hi (x, z) |
|---|---|---|---|---|---|---|
| `sea\sea` | 32,771,262 | 256 | 70 | 10 | 100 | (0x6754, 0x6614) / (0x98AA, 0x99EA) |
| `sea\swell` | 2,951,102 | 128 | 800 | 30 | 60 | (0x7A5B, 0x7A3F) / (0x85A3, 0x85BF) |
| `wind_level_0..4_sea` | 32,771,262 each | same header as `sea\sea`, other frames | | | | no reader found (UNKNOWN who loads them) |

**The container** (loader `0x010D25C0`; both files decode to their exact length, test
`ntw_formats/tests/sea_waves_install.rs`; layout in the module doc of `ntw_formats::sea_waves`):
magic `0x12345678`, version 1, size, tile metres, loop period, frame count, two flag bytes, the
height range, the 16-bit range of the two angle channels, the displacement range, the frame count
again. Then per frame: a heights image (1 byte per texel), the angle image (2 bytes per texel, x and
z interleaved), a horizontal-displacement image (2 bytes per texel; empty in the swell), and two
item lists (empty in the shipped files).

The earlier reading was wrong in one place: **10 and 30 are the loop periods in seconds**, not wave
amplitudes. 70 and 800 are the tile sizes in metres, CONFIRMED by the setter (§4).

**The animation** (CONFIRMED):
- `0x010E21F0` rebuilds the shader's texture every frame from two neighbouring frames:
  `f = fmod(t, period) · frames / period`, frames `a = floor(f)` and `b = (a + 1) mod frames`,
  weight `w = round((f − a) · 256)` (a lerp in 1/256 steps). The update is driven from `0x010E0A80`.
- `0x010E3140` expands each stored byte to 16 bits: `trunc(byte · (hi − lo) / 255 + lo)`. The shader
  sees that value over 65535, and `convert_to_angle` turns it into radians; every shipped angle stays
  within a quarter turn of flat.
- The heights and the displacement drive the original's CPU-displaced sea mesh (`0x01123820`, a grid
  around the camera). We do not build it: our mesh stays flat (**PROVISIONAL**, §5).

**Ours** uploads every frame once as a layer of an `Rg8Unorm` texture array. The shader does the
frame blend and the dequantisation per texel. Both steps are linear, so the texels match the
original's apart from its 16-bit rounding. Two parts are **PROVISIONAL**:
- Each layer's mip chain is our own box filter. Whether the original's per-frame texture has mips,
  and how they are built, is not traced; `ocean.fx` samples it with `MipFilter = LINEAR`.
- The clock is Bevy's elapsed time. Which clock the original's sea runs on, and whether it pauses,
  is not traced.

A container that is missing or fails to decode logs one warning, and that layer is drawn flat. So does
a container with more frames than the GPU's `max_texture_array_layers` (one layer per frame). The
original has no frame limit (it rebuilds one 2D texture per frame); the shipped files have 100 and 60.

The chop, swell and foam textures are decoded and uploaded on the first sea battle and kept for the
session (`SeaTextures`; the install does not change while the game runs). Each battle adds only its
material and sea-bed depth texture, both freed when it ends (`remove_sea`). The decode runs on the
main thread: the first sea battle needs it in the frame it starts. It takes 21 ms in a release
build (73 ms in debug), once per session (2026-10-11, `NHB_Nile`).

`sea\combined_foam.tga` is **not** in that container: it is a plain 32-bit uncompressed 256² TGA with a
26-byte TGA extension area after the pixels, and it is read straight from the install. `ocean.fx`
samples it with `MipFilter = NONE`, so ours has one level.

## 4. The surface shader: `crates/napoleon/src/terrain/water.wgsl`

A port of `ocean.fx`'s non-refraction path: `render_vertex` plus the `!USE_ULTRA_SEA` half of
`render_pixel_ps3`, the branch that runs when `ENABLE_REFLECTION` and `ENABLE_REFRACTION` are both
undefined. Everything below is CONFIRMED as the shipped source unless marked otherwise.

### Where the values come from: CONFIRMED (per-draw setter `0x0119E830`)

The constants written in the shader file are only defaults. The per-draw setter `0x0119E830`
overwrites them from the battle's sea settings object:

| shader constant | value the exe sets | source |
|---|---|---|
| `g_fresnel_R0` | **0.02037** (float constant `0x3CA6D903`) | the exe |
| `g_sea_uv_scale`, `g_swell_uv_scale` | 1/70, 1/800 | 1 / the containers' tile metres |
| `g_sea_deep_colour` | (0.02, 0.032, 0.035), dark grey-green | `sea_climate_details`, first colour |
| `g_froth_value`, `g_froth_distortion_speed`, `g_froth_distortion_amount` | 0.7, 0.35, 0.05 | `sea_surfaces` |
| `g_sea_shininess` | 200 / 1000 / **378** / 1000 / 1000 (wind levels 0..4) | `sea_surfaces` |
| `g_sea_decay` | 0.0006 per metre (the chop fades out by 1.7 km) | `sea_surfaces` |
| `g_reflection_flattening_factor` | 0.5 (no code on the non-ultra path reads it) | `sea_surfaces` |

The settings object holds the climate record's four colours first, then the `sea_surfaces` floats in
field order. Both tables are read through the merged database (`ntw_data::schemas::{SeaSurface,
SeaClimateDetail}`, row readers `0x00F3FE40` / `0x00F3FCF0`; test `ntw_data/tests/sea_install.rs`).
The leading fields of `sea_surfaces` (e.g. `wind_level_0`: 1.95, 1.9e-5, 0.5, 6.86, a seed) feed the
wave generator and are UNKNOWN. All 33 `sea_climate_details` rows are identical.

**Which row** (CONFIRMED):
- The lookup `0x006AB800` takes the climate key and the sea-surface key, and falls back to the first
  row when a key is not in its table.
- For a historical battle the sea-surface key comes from the battle XML (parser `0x00511210`): first
  `weather/wind_level` (read at `0x00511E86`, from the `weather` child found at `0x00511D3F`),
  otherwise the top-level `sea_surface_name` (`0x00511F28`). Both fill the same field, which
  `0x0059482F` hands to the lookup.
- `NHB_Nile` names `wind_level_2`, so its shininess is 378.
- From the campaign, `0x00535B30` (called from `0x008C4E50`) picks the wind level from a random roll
  against the thresholds 0.58317894 / 0.68722874 / 0.82029885 / 0.91646695. Historical battles don't
  need it, so it is not ported yet.
- Ours: `BattleSpec::sea_surface` and `BattleSpec::climate`. An unknown key takes the first row, as the
  lookup does. A battle with no sea-surface key (a campaign battle) also takes the first row,
  **PROVISIONAL** until the roll above is ported; where a battle without a climate key gets its
  climate is not traced (UNKNOWN; all rows are equal, so nothing visible depends on it). Each fallback
  is logged once per battle. The two tables load as optional: without them the sea is not drawn,
  logged once.

So the measured failures of the first slice (`g_sea_decay` 0.2 gave a flat mirror, `g_sea_shininess`
1 gave a white screen) came from the shader's defaults. Our hand-picked stand-ins (1/400 m, exponent
8) are gone. The pale-blue, too-bright sea of the 2026-10-10 user check came from R0 0.5 (a mirror at
every angle) and the shader's default deep colour (0, 0.1, 0.5). The long regular stripes came from
the generated sine PLACEHOLDER, now replaced by the shipped frames.

### The steps

| `ocean.fx` | what we do |
|---|---|
| `convert_to_angle(texel) = (texel − 0.5) · 2PI` | as written, on the blended, dequantised value (§3) |
| `uv_sea_swell = position.xz · g_sea_uv_scale`, `.zw = · g_swell_uv_scale` | as written |
| `sea_scale.x = max(1 − g_sea_decay · position.w, 0)` | as written; the vertex's clip w is passed to the fragment |
| `surface_angle = swell_angle + sea_angle · sea_attenuation` | as written. `sea_attenuation` is a per-vertex attribute in the original (source **UNKNOWN**); ours is the sea-bed depth below the surface over 12 m (**INFERRED**) |
| `sincos`: `n.xz = sin(angle)`, `n.y = cos(x)cos(y)`, normalised | as written |
| `water = g_sea_deep_colour · (dot(n, −light_direction) + 0.5)` | as written |
| `reflect(view_vec, n)`, then `y = abs(y)`, looked up in the sky cube | vector as written; we render no cube, see below |
| `fresnel = saturate(R0 + (1 − R0)(1 − dot(view, −n))^5)` | as written |
| `blinn_phong(−view_vec, n, g_sea_shininess · 0.7)` | as written |
| foam branch (non-ultra): `foam_value` = red, faded over `MAX_FOAM_DIST` 300 m of view distance (`view_vec.w`); `foam_froth` = green × 0.7 (a literal in the shader, not `g_froth_value`) at `distorted_uv(uv_sea · 15)`; `g_froth_value` (0.7 from `sea_surfaces`, setter `0x0119E830`) is only the threshold: at or above it the white amount is `foam_value · foam_froth`; below it, `foam_tendril` = blue at `uv_sea · 15`, `t = (foam_value / g_froth_value)³`, white amount `lerp(foam_tendril, foam_froth, t) · foam_value` | as written; **off by default**, see below |
| `distorted_uv(uv)`: `uv.y += g_time · speed`, offset `(alpha · 2 − 1) · amount` | as written, with the setter's 0.35 / 0.05 |
| `apply_fog(…, true)` + `hdr_encode` | **not ported**: `terrain.wgsl` applies neither, and the water has to match the terrain. The colour is written as computed (**PROVISIONAL** colour space, with the fog/HDR item) |
| shadow-map occlusion on `light_frac` and the specular | **not ported** |
| the colour overlay decal, `g_rain_ripples_texture` | **not ported** |

Two fixes against the first port:
- The foam fade used the clip w; the original uses the view distance.
- `g_froth_value` was the shader's default 0.75; the exe sets 0.7.

### The sky reflection: SKYGEN stands in for the cube map (PROVISIONAL)

`get_sky_reflection_color` samples `s_sky_reflection_texture`, a cube map the engine renders. We
render none. Instead, `water.wgsl` evaluates the original's own sky model for the reflection vector:
- the map's `.environment` `SKYGEN` element holds the parameters of `fx\sky.fx`'s
  `skygen_get_sky_color` / `CalcSkyColour`, the model the original paints its sky with;
- ours reads them into `Skygen` in `battle_terrain.rs`, apart from the sky colour, which is kept once,
  in `Lighting::sky_colour` × `sky_colour_scale` (the battle's clear colour reads it too).

The shipped files store the circumsolar and horizon decays as **positive** numbers (`HB_Nile`:
circumsolar 0.3495, horizon 0.1956, backscatter 0.6893), but `sky.fx`'s defaults are negative
(−0.84, −0.07; backscatter +1.0). With a positive decay, `exp(decay · angle)` saturates to 1 and the
whole sky takes the circumsolar colour, so the engine must negate them when it loads them.

This is **INFERRED**. The setter is not traced: the name at `0x0142512C` is referenced only from the
name table at `0x01469884`. So the two negations are **PROVISIONAL**. The backscatter decay is used
as written.

### Foam is off by default (PROVISIONAL)

`ocean.fx` compiles foam into the pixel shader, but the engine picks **per draw** between:
- `sm_render_sm3` (no foam);
- `sm_render_foam_sm3` (foam);
- the four `sm_render_sea_stenciling_*` variants, which clip it to where the sea meets something.

Those techniques are CONFIRMED to exist. Which pass each region gets is **UNKNOWN**. With foam always
on, it covered `hb_toulon`'s whole bay in whitecaps (measured). `NAPOLEON_SEA_FOAM=1` draws the
foam pass over the whole surface.

Why `NAPOLEON_SEA_FOAM=1` showed nothing on 2026-10-10:
- the foam is white, and the sea under it was itself near-white (R0 0.5 reflecting a bright sky);
- the froth distortion ran at the shader's default speed and amount, 1.0, which offsets the lookup by
  a whole texture and smears the froth.

With the exe's values the foam is easier to see against the dark sea (§7 shots).

## 5. The mesh and the depth blend — `crates/napoleon/src/terrain/water.rs`

- **The grid** is camera-centred: 13 concentric square rings, each 33×33 vertices (32×32 cells) whose
  cell size doubles every ring, the innermost at `g_grid_increment · g_start_band = 0.5 · 4 = 2 m`
  (`ocean.fx`). A ring emits only the cells **outside** the square the previous ring filled — two
  centred squares of 32 cells always overlap in the middle, and coplanar overlap on a flat plane
  z-fights. Cost is constant in the world: 13,533 vertices and 20,480 triangles, out to a 16,384 m
  cell (far past the camera's 20 km far plane), so the sea always runs to the horizon. `g_band_size`
  64 is a band's width in cells; our 32-cell rings reach the same order of detail. **INFERRED** — the
  ring builder is in the exe and was not disassembled.
- **The entity follows the camera**, snapped to the innermost cell so the ring pattern does not crawl.
- **The mesh stays flat (PROVISIONAL).** The original builds its own grid around the camera and moves
  each vertex on the CPU by the containers' heights and horizontal displacement (`0x01123820`, §3).
  Ours only shades the chop; the surface does not rise and fall.
- **`sea_attenuation`** comes from a 512² R8 sea-bed depth texture over the level-0 heightfield
  (`0..12 m`), sampled in the fragment shader by world xz. This is what damps the chop in the shallows
  and leaves the shoreline calm. **INFERRED**.
- **The camera follows the sea surface, not the bare ground** (`terrain::surface_height`). On a naval
  map the ground *is* the sea bed, 100 m down, so `hb_nile` first rendered from under the water
  looking at the sand (measured, screenshot 1). Two lines in `battle::view.rs`.
- Land above the sea level occludes the plane, and nothing else clips it — the original's
  `sm_render_sea_stenciling_*` techniques suggest it does the same with a stencil pass, but how the
  sea is clipped to the map is **UNKNOWN**.

## 6. Resolved / open

| # | item | state | evidence |
|---|---|---|---|
| 1 | sea level = y = 0 map metres | **CONFIRMED** | two independent sources on 60 presets (§2) |
| 2 | one flat plane, no per-body mesh | **CONFIRMED** | `ocean.fx` techniques, no lake/river mesh on a battle map |
| 3 | `ocean.fx` ported (angles, decay, sincos, water, Schlick, Blinn-Phong, foam) | **CONFIRMED** | the shipped source, ported step by step (§4) |
| 4 | `sea\combined_foam.tga` read from the install | **CONFIRMED** | plain 32-bit 256² TGA |
| 5 | `g_sea_decay`, `g_sea_shininess`, froth values, deep colour from `sea_surfaces` / `sea_climate_details` | **CONFIRMED** | setter `0x0119E830`, lookup `0x006AB800`, XML key `0x00511210` (§4) |
| 6 | `g_fresnel_R0 = 0.02037` | **CONFIRMED** | constant `0x3CA6D903` in `0x0119E830` (§4) |
| 7 | `g_sea_uv_scale = 1/70`, `g_swell_uv_scale = 1/800` | **CONFIRMED** | `0x0119E830`, the containers' tile metres (§3) |
| 8 | the wave-slope textures: shipped frames, blended and dequantised as the exe does | **CONFIRMED**; per-layer box mips and the clock **PROVISIONAL** | `0x010D25C0`, `0x010E21F0`, `0x010E3140` (§3) |
| 9 | the sky reflection cube map | **PROVISIONAL** (the map's SKYGEN model; the two decay negations **INFERRED**) | we render no sky cube; the SKYGEN setter is not traced (§4) |
| 10 | `sea_attenuation` from the sea-bed depth | **INFERRED** | the attribute's own source is unknown |
| 11 | the ring grid's sizes | **INFERRED** | `ocean.fx`'s band constants |
| 12 | foam off by default | **PROVISIONAL** | the per-draw pass choice is unknown |
| 13 | `sea\sea` / `sea\swell` container layout | **CONFIRMED** | loader `0x010D25C0`; both files decode to their exact length (§3) |
| 14 | `sea_attenuation`'s real source | **UNKNOWN** | exe not disassembled |
| 15 | inland water bodies at their own level | **UNKNOWN** | no map file records it (§2) |
| 16 | how the sea is clipped to the map | **UNKNOWN** | stencil techniques only hint at it |
| 17 | fog and HDR on the water | **not ported** | `terrain.wgsl` has neither; one change must cover both |
| 18 | reflection / refraction / sea-bed visibility | **not ported** | needs render targets; `USE_ULTRA_SEA`'s shallow/deep blend with them |
| 19 | the CPU-displaced sea mesh | **not ported** (flat mesh, PROVISIONAL) | `0x01123820` builds and displaces it (§3, §5) |
| 20 | campaign wind-level roll | **CONFIRMED**, not ported (campaign battles take the first row, **PROVISIONAL**) | `0x00535B30`, thresholds in §4 |
| 21 | who reads `wind_level_0..4_sea` | **UNKNOWN** | no reader found |

## 7. Proof in-game

Screenshots go in `%USERPROFILE%\Documents\ntw-evidence\screens\ours\` (never committed, never
inside a `target` folder). Add `--skip-deployment` to the `--battle-key` runs to get past the
deployment banner.

**2026-10-11 shots** (`2026-10-11_sea_nile.png`, `_nile_foam.png`, `_toulon.png`, against the
`2026-10-10_*` ones): the sea is now dark grey (it was pale sky-blue); the chop is small and
irregular (it was long regular stripes); with `NAPOLEON_SEA_FOAM=1`, foam patches show within
300 m of the camera (before, nothing changed). No shot of the original was taken: the user's
description of 2026-10-10 is the comparison. A side-by-side with the original is still needed
(PROVISIONAL sky stand-in and flat mesh). The log line now also prints the db row values in use.

The commands:

```
$shots = "$env:USERPROFILE\Documents\ntw-evidence\screens\ours"

# the whole sea: hb_nile's level 0 is a flat -100 m bed, so the surface is all of it
cargo run -p napoleon -- --battle-key NHB_Nile --screenshot $shots\<date>_sea_nile.png --skip-deployment

# the coastline: hb_toulon's bay fills to the shore and the land is untouched
$env:NAPOLEON_BATTLE_CAMERA="0,0,900,0,0.55"
cargo run -p napoleon -- --battle --battle-map hb_toulon --screenshot $shots\<date>_sea_toulon.png

# a land battle is unchanged (0 % water, no surface spawned)
cargo run -p napoleon -- --battle-key NHB_Waterloo --screenshot $shots\<date>_sea_waterloo.png --skip-deployment

# the foam branch
$env:NAPOLEON_SEA_FOAM=1
cargo run -p napoleon -- --battle-key NHB_Nile --screenshot $shots\<date>_sea_nile_foam.png --skip-deployment
```

The log line `Battle sea: N% of <map> is below the sea level, drawing the surface` is the switch that
says a surface was spawned, and its number is `BattleMap::sea_coverage()`.

## 8. What is next, in order

0. **The sea's remaining PROVISIONALs** (2026-10-11): the CPU-displaced mesh (`0x01123820`, item
   19); the sky reflection cube (or trace the SKYGEN decay setter, item 9); the sea's clock and the
   per-frame texture's mips (§3); the campaign wind-level roll (`0x00535B30`, item 20).
1. **Shoreline / surf blending** (`WATER.md` next slot). The pieces are identified: `ocean.fx`'s foam
   passes and the `sm_render_sea_stenciling_*` techniques, `seainteraction.fx` (two scrolling diffuse
   cards, alpha-blended, on a decal plane), `filterfoam.fx` (the foam generator). `g_decal_projection`
   in `Util.fx_fragment` projects the sea onto the terrain, which is how the original gets foam to
   follow a sloping shore. This also turns the foam back on where it belongs.
2. **Planar reflections** — `ENABLE_REFLECTION`: the reflection render target sampled at
   `clip_uv + n.xz · 0.2`, mixed with the sky by the target's own alpha, plus
   `ENABLE_REFRACTION` (`clip_uv + n.xz · 0.05`) and the `USE_ULTRA_SEA` shallow/deep colour blend
   `lerp(g_sea_shallow_colour, g_sea_deep_colour, saturate(dot(lookat, deep_normal)))`.
3. **Rivers and lakes on a battle map** at their own level (§2's open item), and the ships:
   `navalship.fx`, `navalsail.fx`, `shipwake.fx`, `shipbowwave.fx`, `sea_particles.fx`,
   `seainteraction.fx`.
4. **The campaign map's river and sea shading** — `campaignriver.fx` is already ported
   (`campaign::scene::spawn_rivers`, still a still first sample, PROVISIONAL: no scrolling, alpha in
   the vertex colours); `campaignterrain.fx`'s `sm3_campaign_coast` surf is ported as a still frame.
   Both want the scrolling back.
5. **Fog and HDR on water and terrain together** (item 17) — one change, both surfaces.

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
- `cargo run -p ntw_formats --example sea_probe -- header <path>` — the header of the engine's own
  texture container (§3).
- `cargo run -p ntw_formats --example pack_probe -- extract-prefix "fx\" <scratch dir>` — the
  shipped shader sources, into a scratch dir only.

## Where I am / what's next
- **DONE, this slice: the battle-map sea surface** (§4, §5). One flat sea plane at the confirmed sea
  level, a port of `fx\ocean.fx`, a camera-centred ring grid, the sea-bed depth blend, the shipped
  foam texture. Proof in-game on `hb_nile` and `hb_toulon`.
- **NEXT, in this order** (§7): the shoreline/surf blend (`seainteraction.fx`, `filterfoam.fx`, the
  `ocean.fx` foam and stencil passes), then planar reflections, then the campaign map's river and
  sea shading.

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

## 3. `sea\sea` and `sea\swell` — the wave textures — BLOCKED (clean negative)

The sea's two wave textures are the only files in the install's `sea\` folder without an extension:

| file | bytes | header (u32 magic 0x12345678, u32 version 1) |
|---|---|---|
| `sea\sea` | 32,771,262 | u32 **256**, f32 **70.0**, f32 **10.0**, … |
| `sea\swell` | 2,951,102 | u32 **128**, f32 **800.0**, f32 **30.0**, … |
| `wind_level_0..4_sea` | 32,771,262 each | same first 20 bytes as `sea\sea`, different payload |

This is the engine's **own** texture container (magic `0x12345678`, version 1 — the same family as
`.rigid_model` and `.weighted_mesh`), not a DDS: the payload after the header is a mix of
compressed-looking and 8-bit runs whose block layout did not fall out of the bytes (no self-similar
chunk chain lands on the file length, and the colours are not valid DXT1/DXT5). **Decoding it needs
the engine's texture loader in the exe** — i.e. Ghidra — which this slot did not have. Recorded as
**BLOCKED**, not guessed.

What *is* usable from the header, used in the code and tagged **INFERRED**:
- **256** and **128** are the resolutions of `sea\sea` and `sea\swell`.
- **70.0** and **800.0** are the tile sizes in metres (0.27 m and 6.25 m per texel — a plausible
  short-chop/long-swell pair), so `g_sea_uv_scale = 1/70` and `g_swell_uv_scale = 1/800`. The
  alternative reading (70× and 800× tiling, i.e. 1 m tiles) is what makes the shipped foam tile every
  metre, which was measured to look wrong (§4).
- **10.0** and **30.0** are the wave amplitudes in metres, which the sea *mesh* would use for its
  "chop" displacement. Our mesh is flat, so they are unused.

Until the container is decoded, `terrain::water::wave_map` generates tiling slope maps with the same
layout (**PLACEHOLDER**): red and green hold the two slope angles as `0.5 + slope / 2PI`, blue the
slope magnitude. They are the exact gradients of a fixed table of sine waves with whole-number wave
numbers, so they are seamless and reproducible from the table alone — no RNG, no external state.

`sea\combined_foam.tga` is **not** in that container: it is a plain 32-bit uncompressed 256² TGA with a
26-byte TGA extension area after the pixels, and it is read straight from the install.

## 4. The surface shader — `crates/napoleon/src/terrain/water.wgsl`

A port of `ocean.fx`'s non-refraction path (`render_vertex` + the `!USE_ULTRA_SEA` half of
`render_pixel_ps3`, which is the branch that runs when `ENABLE_REFLECTION` and `ENABLE_REFRACTION`
are both undefined). Everything below is CONFIRMED as the shipped source unless marked otherwise.

| `ocean.fx` | what we do |
|---|---|
| `convert_to_angle(texel) = (texel - 0.5) · 2PI` | as written |
| `uv_sea_swell = position.xz · g_sea_uv_scale`, `.zw = · g_swell_uv_scale` | as written, scale per §3 |
| `sea_scale.x = max(1 - g_sea_decay · position.w, 0)` | as written, `position.w` = the clip w (the fragment stage's `@builtin(position).w` is its reciprocal) |
| `surface_angle = swell_angle + sea_angle · sea_attenuation` | as written; `sea_attenuation` is a per-vertex attribute in the original (source **UNKNOWN**), ours is the sea-bed depth below the surface over 12 m (**INFERRED**) |
| `sincos`: `n.xz = sin(angle)`, `n.y = cos(x)cos(y)`, normalised | as written |
| `water = g_sea_deep_colour · (dot(n, -light_direction) + 0.5)` | as written, `g_sea_deep_colour = (0, 0.1, 0.5)` |
| `reflect(view_vec, n)`, then `y = abs(y)`, looked up in the sky cube | vector as written; **PROVISIONAL** the map's `SKYGEN sky_colour · sky_colour_scale` stands in for the cube map |
| `fresnel = saturate(R0 + (1-R0)(1 - dot(view, -n))^5)`, `R0 = g_fresnel_R0 = 0.5` | as written |
| `blinn_phong(-view_vec, n, g_sea_shininess · 0.7)` | as written |
| foam branch: `foam_value` = red, `foam_froth` = green × 0.7, `foam_tendril` = blue, `g_foam_uv_scale = 15`, `g_froth_value = 0.75`, `pow(t, 3)` ramp, `MAX_FOAM_DIST = 300`, `distorted_uv` = alpha as a signed scroll | all as written; **off by default**, see below |
| `apply_fog(…, true)` + `hdr_encode` | **not ported** — `terrain.wgsl` applies neither and the water has to match the terrain |
| shadow-map occlusion on `light_frac` and the specular | **not ported** |
| the colour overlay decal, `g_rain_ripples_texture` | **not ported** |

### Two constants in the shipped file cannot be used as shipped — CONFIRMED by measurement; that the engine overrides them is INFERRED

Both were rendered in front of the install before being changed, so the failure is measured, not
guessed. That the engine sets other values at runtime is INFERRED (no setter traced in the exe):

- **`g_sea_decay = 0.2f`.** The shader multiplies it by `position.w`, which is the clip w **in
  metres**, so 0.2 puts the short chop out **within 5 m of the camera** — the sea renders as a flat
  mirror. Like `g_time = 1.0f` and `g_sea_uv_scale = 1.0f` it is a default the engine replaces.
  **UNKNOWN** what it sets; **PROVISIONAL** `1/400 m` here.
- **`g_sea_shininess = 1.0f`**, used as `g_sea_shininess · 0.7` = an exponent of **0.7**, i.e.
  *below* Lambert: `pow(saturate(dot(n, halfway)), 0.7)` is ≈ 1 over the whole sea, so the sun colour
  drowns it and `hb_nile` rendered as a white screen. Same story. **PROVISIONAL** exponent **8** here
  (half-width `acos(8^(-1/8))` = 0.41 rad, a broad band; at 28 the glitter broke into hard speckle,
  also measured).

**Not the same kind of thing:** `g_fresnel_R0 = 0.5` is used **exactly as shipped**. Real water is
0.02, so the original's sea is a strong mirror at every angle, and that is what we render.

### Foam is off by default — PROVISIONAL

`ocean.fx` compiles foam into the pixel shader but the engine picks **per draw** between
`sm_render_sm3` (no foam), `sm_render_foam_sm3` (foam) and the four `sm_render_sea_stenciling_*`
variants, which clip it to where the sea meets something. Those techniques are CONFIRMED to exist;
**UNKNOWN** which pass each region gets. One surface and one pass here, so there is nothing to choose
by, and with foam always on it covered `hb_toulon`'s whole bay in whitecaps (measured).
`NAPOLEON_SEA_FOAM=1` turns it on. The branch is implemented and readable in `water.wgsl`.

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
| 5 | `g_sea_decay` and `g_sea_shininess` cannot be used as shipped; engine-overridden | **CONFIRMED** unusable (measured white screen / flat mirror); the override is **INFERRED** | measurement; no setter traced |
| 6 | `g_fresnel_R0 = 0.5` used as shipped | **CONFIRMED** | source |
| 7 | `g_sea_uv_scale = 1/70`, `g_swell_uv_scale = 1/800` | **INFERRED** | the container's header floats (§3) |
| 8 | the wave-slope textures | **PLACEHOLDER** (generated) | container **BLOCKED** (§3) |
| 9 | the sky reflection cube map | **PROVISIONAL** (the map's sky colour) | we render no sky cube |
| 10 | `sea_attenuation` from the sea-bed depth | **INFERRED** | the attribute's own source is unknown |
| 11 | the ring grid's sizes | **INFERRED** | `ocean.fx`'s band constants |
| 12 | foam off by default | **PROVISIONAL** | the per-draw pass choice is unknown |
| 13 | `sea\sea` / `sea\swell` container layout | **BLOCKED** | needs the engine's loader in Ghidra (§3) |
| 14 | `sea_attenuation`'s real source | **UNKNOWN** | exe not disassembled |
| 15 | inland water bodies at their own level | **UNKNOWN** | no map file records it (§2) |
| 16 | how the sea is clipped to the map | **UNKNOWN** | stencil techniques only hint at it |
| 17 | fog and HDR on the water | **not ported** | `terrain.wgsl` has neither; one change must cover both |
| 18 | reflection / refraction / sea-bed visibility | **not ported** | needs render targets; `USE_ULTRA_SEA`'s shallow/deep blend with them |

## 7. Proof in-game

Screenshots go in the worker's own `target/tmp` (never committed). The commands:

```
# the whole sea: hb_nile's level 0 is a flat -100 m bed, so the surface is all of it
cargo run -p napoleon -- --battle-key NHB_Nile --screenshot target/tmp/sea_proof/nile.png

# the coastline: hb_toulon's bay fills to the shore and the land is untouched
$env:NAPOLEON_BATTLE_CAMERA="0,0,900,0,0.55"
cargo run -p napoleon -- --battle --battle-map hb_toulon --screenshot target/tmp/sea_proof/toulon.png

# a land battle is unchanged (0 % water, no surface spawned)
cargo run -p napoleon -- --battle-key NHB_Waterloo --screenshot target/tmp/sea_proof/waterloo.png

# the foam branch
$env:NAPOLEON_SEA_FOAM=1
cargo run -p napoleon -- --battle-key NHB_Nile --screenshot target/tmp/sea_proof/nile_foam.png
```

The log line `Battle sea: N% of <map> is below the sea level, drawing the surface` is the switch that
says a surface was spawned, and its number is `BattleMap::sea_coverage()`.

## 8. What is next, in order

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

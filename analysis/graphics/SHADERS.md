# Shaders and materials (shaders worker, BACKLOG §2)

Goal: the game **looks like the original**. Our own WGSL, written from what the original's shaders do. The original
shaders ship as HLSL **source** in `data.pack` `fx\` (Creative Assembly's files): they are read only to learn the
behaviour (inputs, maths, constants) and are **never committed or pasted**. Ghidra (main project, read-only) is used for
what the engine feeds them. Tags: CONFIRMED / INFERRED / UNKNOWN; stand-ins PLACEHOLDER / PROVISIONAL.

Tools: `analysis/graphics/run_ghidra.ps1 <targets> <out> [maxLines]` runs `ghidra_scripts/GfxDecomp.java` (a copy of the
SpeedTree worker's script, plus `wbytes:` = find a UTF-16 string). Raw output stays in `ghidra_out/` (gitignored).
`cargo run -p ntw_formats --example pack_probe -- extract-prefix "fx\\" <scratch dir>` copies the shaders out for
reading (scratch dir only).

## Where I am / what's next (updated with each push)
- DONE: task 0 (UI layout unknowns, see `analysis/frontend/UI_LAYOUT_FORMAT.md` "Update (shaders worker)").
- NOW: §1 survey (below), then the unit shader.

## 1. Survey of the shipped shaders
136 files under `fx\` (data.pack 135, boot.pack `fx\loadingscreen.fx`): 70 `.fx` effects, 52 `.fx_fragment` includes,
11 `.h` headers (`fxconfig.h`, `fx_instancing.h`, `fx_veg_*.h`, `fx_flag.h`, `fx_weighted.h`, ...) and `cardcaps.txt`
(GPU name -> quality preset list). The engine compiles them at run time (D3DX effects) with macros it defines itself
(CONFIRMED `0x011A7F10`: `GAMMA_VALUE` from a float setting, `MIN_FILTER`/`MAG_FILTER`/`MIP_FILTER`/`FILTER_MAX_ANISOTROPY`
from the texture-filter option, `BORDER_ADDRESS_SUPPORT`, `NVIDIA_GPU`/`ATI_GPU`, `MRT_SUPPORT`, `LOW_QUALITY_SHADERS`).

| area | effect files |
|---|---|
| units, horses, equipment | `variantmesh.fx` (weighted = skinned soldiers/horses, rigid = equipment; colour mask; BRDF cloth lighting) |
| buildings, props, campaign settlements | `textured_rigid.fx` (+ `textured_rigid_vertex`, `standard_rigid_matrices`, `rigid_imposter`, dirt), `rigid.fx`, `campaignbuilding.fx`, `weighted.fx` (animated rigids) |
| battle terrain | `terrain_shared.fx(_fragment)`, `terrain_ao.fx`, `terraindecal.fx`, `lightmap.fx(_fragment)`; `battlefieldterrain.fx` is an empty stub (the battle terrain technique is not in `fx\`) |
| campaign map | `campaignterrain.fx`, `supertexturetile.fx`, `campaignriver.fx`, `campaignoverlay.fx`, `campaigntree.fx`, `campaignflag.fx`, `shroud.fx` |
| vegetation | `vegetation_branch/frond/leaf_card/leaf_mesh/billboard.fx` + `vegetation.fx_fragment`, `grass.fx`, `imposter.fx` |
| water, naval | `ocean.fx`, `naval.fx`, `navalship.fx`, `navalsail.fx`, `shipwake.fx`, `shipbowwave.fx`, `seainteraction.fx`, `filterfoam.fx`, `sea_particles.fx`, `rope.fx`, `cloth.fx` (flags) |
| sky, weather | `sky.fx`, `skydome.fx`, `skybox.fx`, `sun.fx`, `cloud.fx`, `cloud_plane.fx`, `weather.fx`, `volumefog.fx`, `lenseflare.fx` |
| effects | `particle.fx`, `particle2.fx`, `particle_distortion.fx`, `explosion_particle.fx`, `projectiletrail.fx`, `ribbon.fx`, `decal.fx`, `spline.fx` |
| post | `hdr_to_screen.fx`, `luminance.fx`, `high_pass.fx`, `gaussianblur.fx`, `directional_blur.fx`, `ssaox.fx`, `depth.fx`, `outline.fx` |
| UI / misc | `sprite.fx`, `custom_sprite.fx`, `extendedsprite.fx`, `icons.fx`, `strengthbar.fx`, `formationlayouticons.fx`, `grid*.fx`, `loadingscreen.fx`, debug/example/template |

Shared fragments: `lighting.fx_fragment` (sun, back light, ambient cube, sky/env cube maps, Blinn/Phong helpers),
`lighting_brdf.fx_fragment` (the BRDF lighting used by units and buildings), `fog.fx_fragment` (distance + height fog),
`shadowmap.fx_fragment`, `util.fx_fragment` (normal-map unpacking), `screeneffects.fx_fragment` (HDR encode, gamma).

## 2. Scene constants the engine feeds every shader
The engine registers the shared parameters by name (CONFIRMED `0x01121D30`): `g_ssao_texture`, `light_direction`,
`light_colour`, `g_specular_scale`, `g_gamma_output`, `g_brightness`, `ambient_cube_lr/fb/tb`, `g_ambient_scale`,
`g_fog_distance_start/strength/scale`, `g_fog_height_bottom/top/strength`, `g_fog_clear_distance`, `g_fog_colour_blend`,
`g_volume_fog_colour/density`, `g_hdr_bloom/cutoff/exposure`. Their values come from the battle's `.environment`
(XML `SCENE`): `LIGHTING` (light_direction euler, light_colour + scale, the six `ambient_cube_*` faces + scale,
hdr_bloom/cutoff/exposure, specular_scale), `FOG` (the `g_fog_*` names one to one), `POST_PROCESSING` (overlay,
brightness/contrast/saturation per channel, hue: INFERRED to build the final `colour_matrix`), `SKY_DOME/SKYGEN`
(software sky: sky, circumsolar, horizon, backscatter colours), cloud planes, `SUN_MESH`, `WIND_SETTINGS`.
The attribute names are UTF-16 strings in name lists read through a table of accessors (`0x01189E80..`, lists at
`0x014688D8..0x01469228`).

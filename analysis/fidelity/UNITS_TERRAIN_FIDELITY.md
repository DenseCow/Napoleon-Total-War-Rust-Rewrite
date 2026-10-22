## Review 0-D (2026-10-06, branch `work/review-next-0d`, base `origin/work/port-next-1006` + round 11)

This block **overrides** the verdicts below where they disagree. Round 11 (`829c50c`) came in
cleanly (`git apply --3way`, code and notes only).

**Evidence audit: exe claims with no kept decompile.** The 0-D Ghidra dumps kept in the sandbox
(`ghidra_evidence/0d/{a..q}_out.txt`) and the other workers' dumps were searched for every address
below. None of them carries the decompile the claim rests on (at most an address or an instruction
list), and round 8's `run12.txt` and the `target/tmp/0d_*` scratch files were never kept. Each is
therefore **INFERRED**, not CONFIRMED, until its decompile is kept:

| claim | was | now |
|---|---|---|
| `0x00732190` loads `standard_bearer_flag.logic` | CONFIRMED | INFERRED (`0b_round12__step4.txt` lists the function, not its strings) |
| `0x01227BD0` completes `flag_` and falls back to `flag_default.tga` | CONFIRMED | INFERRED (`gh__r11_e8_per_item.txt` shows `naval_id_*` strings referenced from it, not `flag_`) |
| `0x00B42B90` builds `_slot_fortifications_lvl`; literal at `0x0137D604`, one reference | CONFIRMED | INFERRED (the *file names* are CONFIRMED from the pack) |
| `MIN_TREE_SCALE` 0.5 / `MAX_TREE_SCALE` 1.4 via `0x00EC33A0` | CONFIRMED | INFERRED (`sb0a_g8_out.txt` has the function header only) |
| the `+0x1B0` value set `{0,1,2,3,5,6,7}` (`0x0064F790` tests 3) | CONFIRMED | INFERRED (`0d/f_out.txt` lists the five predicates' `CALL [EAX+0xA8]` sites, not their compares) |
| the wind-audio `|v|` thresholds 0.583 .. 0.916 | stated | INFERRED |

**Downgraded from data over-reach:**
- *Flag frame offset `(0, -0.967, -0.075)`.* The numbers are CONFIRMED data (the six numbered mast
  particles; `flag_install` checks y to 0.01, z only to 0.15). That the exe shifts the verlet item by
  it is **INFERRED**; round 8's "RESOLVED (CONFIRMED)" conflated the two, and round 7's UNKNOWN
  ("sail 0.97 m sideways" vs "offset onto the pole") is only settled by choosing the reading that is
  a flag. The code now applies it (`ntw_formats::cloth::verlet_frame`); before, `FRAME_OFFSET` was
  defined and **unused**, so the drawn mast sat 0.97 m beside the pole.
- *Walls file level = chain level + 1.* A vertex-count ladder does not say which building level asks
  for which file: **INFERRED**. Under it `_lvl0` would be the unfortified mesh on every settlement;
  we deliberately draw nothing there (open, target `0x00B42B90`'s caller). "Additive" stays INFERRED.
- *Tree scale byte.* "A relative scale" is the data's reading; the `/128` decode is PROVISIONAL (the
  code said CONFIRMED and INFERRED at once). The `I32` doc "0 in every shipped map" was wrong (391).
- *Shrubs.* "The original draws far shrubs as geometry" is INFERRED from the absence of any other
  representation. *Wind:* 16 m/s is the table maximum (data, now with an install test
  `speedtree_install::wind_ladder_is_the_shipped_wind_levels_table`); dividing by it is PROVISIONAL.
- *Cloth move (round 9).* Accepted: the `step_with` body and every constant diff identical between
  `89190f3:ntw_formats/src/cloth.rs` and `ntw_sim/src/battle/cloth.rs` (checked in review). This
  is a claim about our port, not the exe.

**Bugs fixed:** the flag query matched nothing (bearer is a *child* of the unit view), so no flag
was ever stepped and all sat at the origin; `mirror_z` negated column 2 instead of row 2; every
frame overwrote the atlas UVs with the raw `t(u, v)`; a NaN `dt` passed the clamp and poisoned the
cloth for good; the explicit drag could diverge for a tiny mass; `FlagCloth::new` trusted indices;
the `.logic` reader panicked on a fourth coordinate or a bare `particle`/`rope`; walls could take
another culture's art than the city's; a `"nan"` tree-wind override poisoned the wind matrices.
**Tests made honest:** an unscaled-list check that skipped the lists it checked, a clamp-inside-
clamp assertion, two const-equals-itself / function-equals-its-own-body tests, and a looser
"at most two presets" than the notes' one.

**In-game checks this review adds:** the flag must now hang **on** the pole (not beside it), carry
the right faction's picture after the first frame, and appear at each bearer rather than at the map
origin. Walls: a European city's walls are the `EU` set.

## Round 11 in one block (2026-10-06, branch `work/next/0d-units`, base `d4b35a0` = `sandbox/next`)

**Shrubs are DRAWN, from their own `.spt` geometry.** Item 3 (per-tree rotation) is closed **negatively**
from data, item 4 (the wind) now has a real source instead of an invented one, and one part of item 2
(the alpha-test reference) is measured rather than guessed. No Ghidra this round: every claim is data.

| # | item | verdict | evidence kept |
|---|---|---|---|
| 1 | the shrub layer | **DRAWN**, from the species' own SpeedTree geometry | **CONFIRMED** that shrubs ship no billboards (every tree has exactly 8, every shrub none — `battle_terrain_install::every_tree_species_resolves`), so there is nothing to cross-fade them to; **CONFIRMED** that their geometry is complete enough to draw: **34 shrub species / 524,295 instances** over the presets and **every one** produces leaf cards and/or fronds with a composite-map rectangle and an existing diffuse texture (`speedtree_install::every_shrub_species_generates_drawable_geometry`). `speedtree::{SHRUB_FAR_DISTANCE, is_shrub, shrub_far_distance, Layer}`, `NearTrees::{trees, shrubs}`, `update_layer`. §4.3 |
| 2 | billboard alpha reference | **measured; the game's value still UNKNOWN** | the atlas alpha is a **soft ramp, not a two-level cut-out** — 633,547 partial texels against 3,126 fully opaque inside the billboard rectangles of 29 atlases — so "any threshold in (0,1) looks the same" is **refuted**. 0.33 (what we draw with) loses **0.4%** of the picture against a true cut-out; 0.50 loses 24.8%. Test `billboard_atlas_alpha_is_a_soft_ramp`. §4.4 |
| 2b | which direction picture 0 faces | **still UNKNOWN** | nothing in `*_compositemap.txt` states an azimuth: the eight rectangles are just eight UV rects, and the atlases scatter them across the sheet. Not attempted this round. |
| 3 | per-tree rotation / slope tilt | **the data has none — NEGATIVE, closed** | `TREE_ITEM` is **closed at `Coord2d, U8, I32`** (the parser errors on a single leftover child, and every shipped list parses), so there is no fourth column to hold an angle. The unmapped `I32` is 0 on **3,843,976** of **3,844,367** instances and takes only the values 1 (256) and 2 (135), **all on `ottoman_great_fortress`** and **all in its two unscaled outer lists** — a per-list property, and three values cannot encode 360°. Test `the_tree_instance_i32_is_not_a_rotation`. The angle is therefore the renderer's (UNKNOWN), which is what §4.1 already said. |
| 4 | the wind sway's source | **the battle's own `prevailing_wind`**; the scale is CONFIRMED, the division INFERRED | the same vector the flag cloth uses. **New data:** `db\wind_levels_tables\wind_levels` (schema `s,s,f,f,i`, 10 rows) is the game's wind table, and every row is either **westerly (+x) or northerly (−y)** with magnitude exactly **3, 6, 9, 12 or 16 m/s** — so **16 m/s is the game's strongest wind**. Strength = speed / 16, direction = the vector. `WIND_LADDER`, `wind_level`, `battle_wind_target`, `TreeWind::{wind, applied}`. §4.5 |

**A negative worth more than the divisor:** four of the five distinct speeds the 29 shipped battle
files declare — `(0,9)`, `(0,5)`, `(0,0)`, `(10,0)`, `(−5,5)` — are **not** on that five-rung ladder,
so the vegetation's wind is **not** a `wind_levels` lookup. And the exe cannot recover the table's own
audio column from its `|v|` thresholds (0.58317894 / 0.68722874 / 0.82029885 / 0.91646695) by scaling
the magnitude: dividing by 16 bands the five speeds **0, 0, 0, 2, 4**, while the table assigns them
**0, 1, 2, 3, 4**. Those thresholds therefore act on a *time-varying* battle wind, and the ladder only
fixes the scale.

### 4.3 Shrubs (round 11) — drawn as geometry, at every distance

The whole argument is three data facts, no RE:

1. **Shrubs have no billboards.** A `*_compositemap.txt` entry with an empty `Billboards` section is a
   shrub's; every tree's has exactly 8 (CONFIRMED on every preset, `every_tree_species_resolves`). So
   a shrub cannot be handed to `trees.rs`, and there is nothing to cross-fade it *into*.
2. **Their geometry is complete enough.** `speedtree_install::every_shrub_species_generates_drawable_geometry`:
   **34 shrub species, 524,295 instances** over the presets, **0** with nothing drawable. Per species:
   leaf cards (up to 317), frond triangles (up to 848), 1–4 composite leaf rectangles and exactly 1
   frond rectangle, and the diffuse texture present in every case. **9 of 34 have no bark** — correct,
   a shrub has no trunk worth texturing; those draw from cards and fronds alone.
3. **There is no cheaper far representation.** Shrub `.spt` files ship **2 or 3** branch LOD levels
   against trees' 2/3/4 (`shrubs_ship_no_extra_branch_lods`) — i.e. nothing special — and the shipped
   `.spt` files carry **no leaf-cluster LODs at all** (`SptFile::leaf_lods` is `None` throughout,
   §spt.rs). Our generator emits the same strips for every branch LOD (`generate.rs::build_branch_lods`,
   PROVISIONAL), so selecting one by distance would buy nothing today. **Targets:** the exe's
   per-species LOD distances (`branch_lods` 9007/9008/9012–9014) and `VegetationMeshManager`'s cell
   streaming radius.

`SHRUB_FAR_DISTANCE` = **400 m**, PROVISIONAL, `NAPOLEON_SHRUB_DISTANCE` to override. The data bounds
the cost, not the number. The census (`shrub_instances_within_a_draw_radius`) is what it is chosen
against — the densest preset's count of shrub instances within a radius of the map centre:

| radius | worst map | shrubs |
|---|---|---|
| 200 m | `hb_arcole` | 957 |
| 400 m | `hb_arcole` | 5,139 |
| 800 m | `hb_arcole` | 19,358 |

For scale: the whole-map worst is `rti_fort_2` with 34,828 shrubs and `hb_arcole` with 34,063, against
`hb_arcole`'s 17,160 trees. The two layers are streamed independently (`Layer`, one 64 m cell map and
one 24-cell-per-frame budget each), so a dense shrub field cannot starve the trees.

### 4.4 The billboard alpha reference (round 11) — measured, not guessed

`speedtree_install::billboard_atlas_alpha_is_a_soft_ramp`, over the rectangles of all **29** shipped
`*_diffuse_billboards.dds` (all DXT5): alpha spans 0..255 and the channel is a **ramp** — 633,547
partial texels against 3,126 fully opaque (a picture's rectangle is mostly the atlas's empty surround,
which is why "fully opaque" is the small minority). Texels surviving each reference, as a share of the
0.10 baseline:

| reference | surviving | lost vs a true cut-out |
|---|---|---|
| 0.10 | 635,765 | — |
| 0.20 | 635,765 | 0.0% |
| **0.33** (ours) | 633,349 | **0.4%** |
| 0.50 | 478,017 | 24.8% |
| 0.80 | 190,682 | 70.0% |

So "any value in (0,1) gives the same picture" is **refuted**, and 0.33 is a safe default: anything up
to about 0.45 looks the same, while 0.5 visibly thins the canopy. **The game's own value is still
UNKNOWN** — the alpha reference is one of SpeedTreeRT's, and the `.spt` files do not carry it.

### 4.5 The wind (round 11) — the battle's own, over the game's own scale

`db\wind_levels_tables\wind_levels`, schema `s,s,f,f,i`, **10 rows** (CONFIRMED, decodes whole):

| key | sound | x | y | order |
|---|---|---|---|---|
| moderate_westerly | `wind_level_2` | 9 | 0 | 0 |
| calm_westerly | `wind_level_0` | 3 | 0 | 1 |
| light_westerly | `wind_level_1` | 6 | 0 | 2 |
| strong_westerly | `wind_level_3` | 12 | 0 | 3 |
| storm_westerly | `wind_level_4` | 16 | 0 | 4 |
| light_northern | `wind_level_0` | 0 | −3 | 5 |
| moderate_northern | `wind_level_1` | 0 | −6 | 6 |
| strong_northern | `wind_level_2` | 0 | −9 | 7 |
| gale_northern | `wind_level_3` | 0 | −12 | 8 |
| storm_northern | `wind_level_4` | 0 | −16 | 9 |

- **CONFIRMED**: the game's wind speeds are exactly **3, 6, 9, 12, 16 m/s**, and its only two wind
  directions are **westerly (+x)** and **northerly (−y)** — one axis each, never a diagonal. Column 2
  is the wind audio sample and it rises with the magnitude, identically in both directions.
- Column 5 is the frontend's display order: `WindLevelOptionsString` (`0x0047A260`, CONFIRMED) sorts
  the rows by it, so this table is the **custom-battle setup's wind picker**, not the per-battle wind.
- **The per-battle wind is `weather/prevailing_wind`** (`battle_spec`, all 29 shipped battle files), and
  it is **not** a row of this table: the five distinct vectors are `(0,0)`, `(0,5)`, `(0,9)`, `(10,0)`,
  `(−5,5)` — only 9 is on the ladder, and `(0,9)` is `strong_northern`'s magnitude with the **opposite
  sign**. It is also a separate field from `<wind_level>`, which only the `testdata` battle files carry
  (always `wind_level_2`).
- **So:** strength = `speed / 16` (the divisor CONFIRMED, the division INFERRED), direction = the
  vector's horizontal pair. **PROVISIONAL** that the game multiplies the base strength by a
  time-varying factor (`FUN_01200be0`, not decoded).
- **The axis convention is the one real risk.** The ported `SpeedWind::advance` reads the wind's
  horizontal components as `d[0], d[1]` (`theta = d[0].atan2(d[1])`) with its `z` vertical, so the
  horizontal pair must land in `x,y`. Austerlitz's `(0,9)` therefore becomes SpeedTree `(0, 9, 0)` and
  bends the trees about the x axis, along ±y. Before this round the direction was hard-coded along
  SpeedTree +x, which is this same reading with the battle's y component zero. If the trees lean the
  wrong way in game, `FUN_01240980` (the game → SpeedTree permutation) is the target.

---

## Round 10 in one block (2026-10-06, branch `work/next/0d-units`, base `b7dcbfd` = `sandbox/next`)

**Item 1 did not settle, but it moved one whole step, and item 3 is now drawn on the campaign map.**

| # | item | verdict | evidence kept |
|---|---|---|---|
| 1 | the string `0x01227BD0` is handed for a dependent faction | **UNKNOWN, and the round-9 target was wrong** | `0x01227BD0` has **exactly one** direct caller (`0x011CE120`) and it is handed the **literal `"default"`**; the ASCII `flag_` is referenced from **two** exe addresses only (`0x01227C32`, `0x0123C585`). `0x01227BD0` is the flag system's *registry lookup*, not a per-faction lookup; `0x0123C570(index, name, kind)` is the setter that completes `flag_<name>.tga` and has **zero** direct callers. What remains unknown is **which string names a flag record** — a record field at `+0x38`, from a table this pass did not identify. §3.2a |
| 1b | is it even the battle flag? | **probably not** | everything around `0x01227BD0` is **campaign** flag art: `flags.tai` / `exp_flags.tai` / `naval_id.tai` / `overlay.tai`, `campaignflag.fx`, `Flag_primary` / `Flag_secondary` / `naval_ID_frame`, `naval_id_damage_*`, `flag_orn_group_0..10`, `flag_orn_general_*`, and the CVars `army_flag_scale` / `navy_flag_scale` / `agent_flag_scale` / `occupied_settlement_flag_scale` / `occupied_slot_flag_scale` / `occupied_fort_flag_scale` / `unoccupied_*`. Also: **8** atlas images that no `factions` column names, among them `flag_pirates.tga` and `flag_rebels_other.tga`. So whether the standard bearer's cloth reads this atlas at all is open. Test `flag_faction_install::the_battle_flag_target_is_the_campaign_flag_system_not_the_bearer` |
| 1c | the rival columns, refuted by count | **CONFIRMED** | only `flag_path` resolves **39/39** dependents: `model_faction` 22, `rebel_flag_path` 26 (13 give nothing), `republic_flag_path` 10 (29 give nothing), `faction_group` 7, `subculture` 0. The rivals also *disagree* (`french_rebels` -> `france` vs `rebels_europe`; `egy_bedouin` -> `rebels_eastern` vs `bedouin`). Test `only_flag_path_resolves_every_dependent_faction`; probe `unit0d_probe flagcands` |
| 2 | is `slots_art` useful yet? | **yes, through the settlement** | the table was already read by the settlement path; what was missing was a *draw*. Answered this round — see item 3 |
| 3 | the settlement's fortification mesh | **DRAWN, INFERRED (additive)** | `slots_art`'s `settlement` row -> `slots_templates_models` -> `<folder>\<stem>_<n>_slot_fortifications_lvl<level>.rigid_model`, and `_lvl0 < _lvl1 < _lvl2` in vertex count for **every** culture and every `<n>` (**23 ladders**), all three sharing the plain city's exact three textures. §3.2b |

**Visible:** a settlement that has walls now shows them, and building or demolishing walls changes
the model. `napoleon::campaign::scene::{SettlementWalls, SettlementWallsView, sync_walls}`.

### 3.2a The flag key: parked, one step further on (round 10)

Eleven read-only Ghidra runs (r14..r28 in the scratch `target/tmp/0d_g/`, out of git) on this
worker's own project copy `%USERPROFILE%\Documents\NR-sb-0d-ghidra-r10` (a fresh copy of the
retired 0-A project), runner `analysis/fidelity/run_ghidra_0d.ps1`.

**The negative that moved it, CONFIRMED:**

- `callers:0x01227BD0` -> **one** caller, `FUN_011CE120` (5063 bytes, `this` = the flag manager,
  vtable `0x01423124`). Its single call is `FUN_01227BD0(<the string "default">, 0, 0)`, and the
  result is stored at `this+0x3008` — the **default** flag record.
- `nbytes:flag_$` -> the ASCII `flag_` is at `0x014237AC` and is referenced from **exactly two**
  addresses: `0x01227C32` (inside `0x01227BD0`) and `0x0123C585` (inside `0x0123C570`). There is no
  third place in the exe that completes a flag image name.
- `FUN_0123C570(index, name, kind)` builds `flag_<name>` (+ `naval_id_` when the record's `+0x64`
  is 5), looks it up, and writes the texture to the record at `index` — and it has **zero** direct
  callers and no data reference (`find:0x0123C570` is empty). So it is either dead or reached
  through a dispatch this pass did not resolve.
- `FUN_01227BD0` is a **registry lookup**, not a build: it first walks the record array at
  `this+0x2FF4` (count `+0x2FF0`) comparing each record's name (`FUN_004F0E50`, a string compare)
  and its `+0x64` against the kind, and returns the index if it finds one. Only on a miss does it
  build `flag_<name>.tga` and fall back to `flag_default.tga`.
- The flag system's own name is at **record `+0x38`**: `FUN_01206E80/20/EE0` (one per group) read
  `record[i].+0x38` and compare it with `FUN_01180A50` against each already-built flag, which is how
  `FUN_011CE120` de-duplicates by name. So *the string is a record field at `+0x38`*, and those
  records come from a table this pass did not identify.

**So the open question is no longer "what is `0x01227BD0` handed"** — it is **"what fills a flag
record's `+0x38`"**. That needs the writer of the record array at `+0x2FB4` / `+0x2FC4` / `+0x2FD4`
(`scal:0x2FB4` finds only two *reads*, `0x011CE5C4` and `0x01206E8A`, so the fill is not a plain
`MOV [reg+imm]`), or the debugger.

**And it is probably the campaign's flag, not the bearer's.** The constructor `0x011CE120` also
builds `Flag_strength`, `Flag_strength_2`, `naval_id_damage_left/right`, `naval_id_damage_sails`,
`flag_pole_0`, `campaign_flag` + `Flag_primary`, `Flag_secondary`, `naval_flag` +
`naval_ID_frame`, `flag_orn_group_0..10` and `flag_orn_general_1..4`, and the meshes go through
`campaignflag.fx` (`FUN_011CF5D0`). The public accessor `FUN_011F3650` (vtable slot 1) draws the
**naval ident** case explicitly (`record[0x19] == 5`) and otherwise an army flag with a primary and
a secondary. The CVars beside it are `army_flag_scale`, `navy_flag_scale`, `agent_flag_scale`,
`occupied_settlement_flag_scale`, `occupied_slot_flag_scale`, `occupied_fort_flag_scale` and the
`unoccupied_*` set. Two shipped campaign scripts read the `factions` row's `flag_path` **and**
`prev_flag_1`, `prev_flag_2`, `next_flag_1`, `next_flag_2` — a five-image flag *chain*, which is a
campaign-UI concept (`FUN_009C1700` = `CreateEndTurnFlagsDisplay`, `FUN_005A5D30` =
`ShowVictoryTicketDisplayPanel`). Nothing in that set is a standard bearer.

**In game, unchanged and still worth one look:** a Spanish or rebel unit's standard bearer should
fly a real flag rather than `flag_default` (round 9's line). What round 10 adds is that if the
original shows `flag_default` there too, the reason is **not** that the exe hands over the faction
key — it would be that the battle never asks this atlas at all. Both readings still predict "a
real flag on a Spanish unit in the remake"; only the mechanism is now open.

### 3.2b The settlement's walls (round 10, drawn)

**What exists (CONFIRMED, probe `unit0d_probe slotfort`, install test
`slots_install::the_settlement_fortification_levels_are_a_rising_ladder`):**

| culture | template | folder | slots `<n>` with fortification art |
|---|---|---|---|
| `european` | `EU_Settlement` -> `EU_City` | `Templates\EU` | 1, 2, 3, 4, 5 |
| `indian` | `IND_Settlement` -> `IND_City` | `Templates\IND` | 1, 4, 5 |
| `middle_east`, `egy_middle_east`, `egy_european` | `OTT_Settlement` -> `OTT_City` | `Templates\OTT` | 1, 2, 3, 4, 5 |
| `tribal` | `NA_Settlement` -> `NA_City` | `Templates\NA` | **none** |

**23 ladders**, every one `_lvl0 < _lvl1 < _lvl2` in vertex count, and all three files of a ladder
use the **exact same three textures** as the plain `<stem>_<n>_slot.rigid_model`
(`eu_cities_napoleon_{diffuse0,normal0,specular_map0}`, `OTTTemplates_napoleon_*` for OTT). So the
three files are variants of one city model, and the vertex ladder is what makes the file level the
`sFortifications` **chain level + 1** — round 9's offset, now proved rather than assumed.

**The meshes are flat ground layers**, not buildings: model y spans about `-0.03 .. 0.06`, and
`DISPLAY_TO_LOGIC` is `1/0.0254`, so the plain city stands ~2.4 m above the settlement's origin and
the fortification layer lies roughly at or below it.

**INFERRED, and the one thing to look at:** the fortification mesh is drawn **in addition to** the
plain city mesh, not instead of it. The data forces that for the Ottoman templates — `OTT_City`'s
`_lvl1` and `_lvl2` do not overlap the plain mesh's footprint at all (`n=1`: plain x `-0.17..0.18`,
lvl1 x `0.15..0.27`) — and the name keeps `_slot_` as a qualifier of the *same* slot rather than
replacing it. The risk is co-planar geometry with the city in the European templates, which is why
the in-game line asks about flicker.

**`sFortifications` has exactly two levels**, 5 turns / 8000 and 10 turns / 16000
(`sFortifications1_settlement_fortifications`, `sFortifications2_improved_settlement_fort`).

**And there is nothing to see until walls are built:** `sFortifications` occurs **zero** times in
all **eight** shipped start positions (probe `unit0d_probe wallcount`), so the feature is only
visible after the player builds walls — which the settlement panel already does (0-B's
`ConstructBuilding`, 0-E's fort tab).

**Code:** `ntw_data::campaign::{SLOT_FORTIFICATIONS_SUFFIX, SLOT_TYPE_SETTLEMENT,
SlotTemplateModelRecord::{folder_path, slot_model, fortification_slot_model}}`;
`napoleon::campaign::scene::{SettlementWalls, SettlementWallsView, sync_walls, warm_walls,
settlement_slot_number}`. `warm_walls` loads **every** walled level at scene build, because
`sync_walls` has no model source of its own — without it a settlement you build walls on *during* play
would never show them (found in review, before commit).

`cargo test --workspace` exit 0; `cargo check -p napoleon` clean; no clippy warning in any file this
round touched. Probes `cargo run -p ntw_data --example unit0d_probe -- slotfort|wallcount|flagcands`.

## Round 9 in one block (2026-10-06, branch `work/next/0d-units`, base `eeb9750` = `sandbox/next`)

**A refactor that changes a simulation number is a bug; this one changes none, and it is proved.**

| # | item | verdict | evidence kept |
|---|---|---|---|
| 1 | the cloth solve moves into `ntw_sim` | **CONFIRMED** — `ntw_sim::battle::cloth` (new) holds `Link`, `ClothParts`, `FlagCloth`, `step`/`step_with`, the projection, the rod push-out, `normals`, `ClothMesh`; `ntw_formats::cloth` keeps the reader (`ClothSpec`, `SpecLink`, `standard_bearer_flag`, `cloth_spec`, `FRAME_OFFSET`, `bone3_at_ground`, `mirror_z`) | **`ntw_sim/Cargo.toml [dependencies]` is still empty.** The data crosses as a tuple of std types (`ClothParts`), because both crates are dependency leaves and a conversion needs one to depend on the other; the install test uses `ntw_formats` as a **dev**-dependency |
| 2 | the flag solves identically after the move | **CONFIRMED** — every measured line byte-identical | `cargo test -p ntw_sim --test flag_install -- --ignored --nocapture`, before (in `ntw_formats`) and after (in `ntw_sim`); both `test result: ok. 5 passed; 0 failed`. Depth `1.390 m`, hang `y 1.119`, sweep `1.126/0.3% .. 1.135/0.0%`, step trace `1.4232 .. 1.1186`, all twelve particle positions identical. The two logs differ **only** in the wall-clock duration |
| 3 | `slots_art` for the fort models | **CONFIRMED** from the install, with a **correction for 0-E** | `ntw_data::campaign::{SlotArtRecord, SlotTemplateModelRecord}` load; `slots_art` = 12 slot types × 6 cultures = 72 rows; its #4 and #6 are keys of `slots_templates_models` (54/54); the region's fort is `fFort` (3 levels) -> `fort_lvl<n+1>_blend.rigid_model` (exactly 3 models). `0x00B42B90` is the **settlement's** `_slot_fortifications_lvl` builder, not the fort's. Tests `crates/ntw_data/tests/slots_install.rs` |
| 4 | the 39 dependent factions' flag key — **CLOSED** | **CONFIRMED** over all 77 `factions` rows; **PROVISIONAL** on the exe's caller | it is **not** the faction key and **not** `faction_group`: it is the **last segment of `factions.flag_path`** (`data\ui\flags\france` -> `flag_france.tga`), and that segment is a key of `flags.tai` for **77 of 77** — so all 39 dependents now get a real flag instead of `flag_default`. `faction_group` would resolve only **1** of the 12 rebel groups. Test `crates/ntw_data/tests/flag_faction_install.rs`; probe `unit0d_probe flagparents` |

Round 9 added two assertions for the move itself: the reader -> solve crossing is field-for-field lossless
(including the link order, which matters because the projection is Gauss-Seidel), and `ntw_sim`'s own
`transform_point` equals `ntw_formats::anim::transform_point` on all 42 shipped particles of the shipped
bone. `cargo test --workspace` exit 0; `cargo check -p napoleon` clean; no clippy warning in any file
touched.

**Not visible in game, and it cannot be:** loading `slots_art` changes no draw, and nothing in the model
can create a fort yet (0-B's `BuildFort`; `FORT_ARRAY` is empty in all eight startpos files and all ten
vanilla saves).

**Item 4 is visible.** A dependent faction's unit flag stops being `flag_default` and becomes the image
its own `factions` row names — e.g. `egy_britain` now flies `flag_britain.tga`, `ita_piedmont` flies
`flag_sardinia.tga`, and the twelve rebel groups split between `flag_rebels_europe.tga` and
`flag_rebels_eastern.tga`. PROVISIONAL: `0x01227BD0` completing `flag_` and falling back to
`flag_default.tga` is CONFIRMED, but what string it is handed for a dependent faction is UNKNOWN; the
target is that caller.

## Round 8 in one block (2026-10-06, branch `work/next/0d-units`, base `cd71601` = `sandbox/next`)

**The flag is drawn.** `crates/napoleon/src/battle/flag.rs` (new) + `ntw_formats::cloth` (new).
`cargo check -p napoleon` **clean**, `cargo test --workspace` **exit 0**. (The round-7 brief said the
sandbox could not check `napoleon`; it could this round, so re-run it on the install.)

| # | item | verdict | evidence kept |
|---|---|---|---|
| 1 | the flag cloth | **DRAWN.** The six `auto_` ropes are **zero-rest-length welds** onto the mast (CONFIRMED: the six pinned cloth particles sit *on* mast particles 1..6 to the millimetre) and the 101 `srope` constraints carry the shape (CONFIRMED rest lengths 0.148 .. 0.300 m). The solve's *shape* is CONFIRMED; how `mass` / `surface_area` / `drag_coefficient` / `gravity_coefficient` combine is INFERRED; the wind and the solver constants are PROVISIONAL with named targets | `flag_install::the_shipped_flag_cloth_solves` (42 particles, 60 triangles, 6 pins at rest 0, 101 ropes, `bone 3 Weapon3`, authored world box y 1.422 .. 2.579, 1.31 m of sail out in a 9 m/s wind, hangs to y 1.119 in still air, every rope within 1% of its rest length, the six welds never leave the mast); 10 unit tests in `cloth.rs`; probe `flagcloth` |
| 1b | the flag atlas | **CONFIRMED**: `*.tai` is a **plain-text** list (the `AtlasCreationTool.exe` header documents its own schema). Every shipped one parses, every page it names exists, every rectangle lies inside its page: 34 files, `flags.tai` = 68 `flag_<key>.tga` images over 3 pages including `flag_default.tga`. The keys are the campaign faction keys — **38 of the 77 faction keys have their own image** and the other 39 are dependent factions (`spa_*`, `ita_*`, `egy_*`, `tut_*`, `*_rebels`) that must take a parent's flag | `flag_install::every_shipped_tai_parses`, `::every_factions_flag_is_in_the_shipped_atlas`; probe `flagtex` |
| 1c | the pole bone | **CONFIRMED and now used.** `bone3_at_ground()` is the exe's own `Weapon3` frame-0 matrix and a unit test asserts it against the probe line for line. Plain arithmetic on it: the sail's hoist edge is **1.43 m to 2.58 m above the bearer's feet**, the pole's butt 0.62 m below them, and the file's flat-in-y sheet becomes a **vertical** flag because the bone's +x is world up | `cloth.rs` unit test `bone3_at_ground_puts_the_flag_where_the_shipped_bone_says`; probe `equipbones` |
| 2 | the idle-stance enum | **CLOSED-ATTEMPTED** (§9.8). Not named, no code changed | kept decompiles of `0x0064F6C0`, `0x0064F710`, `0x0058EDD0` (`run2.txt`) and `0x005D85D0` (`run9.txt`) |
| 3 | fort models | not reached (time-box spent on 1 and 2) | — |

Files this round: `crates/ntw_formats/src/{cloth,texture_atlas}.rs` (new), `crates/ntw_formats/src/lib.rs`,
`crates/ntw_formats/tests/flag_install.rs`, `crates/ntw_data/examples/unit0d_probe.rs` (new
sub-commands `flagcloth`, `flagtex`, `wind`), `crates/napoleon/src/battle/{flag,view,mod,setup}.rs`,
`analysis/fidelity/UNITS_TERRAIN_FIDELITY.md`, `docs/BACKLOG.md`, `docs/HANDOFF.md`.

**Two real solver bugs, found against the shipped file and fixed** (both would have shipped):

1. **The constraint projection was erasing the gravity velocity.** Moving `pos` without moving
   `prev` cancels the displacement the integrator just built; because the sheet is stiff and pinned
   along a *vertical* edge, the sail then never fell at all — the sweep over relaxation counts was
   non-monotonic and one setting left it dead still at its authored height. Moving `prev` with `pos`
   makes the correction a positional fix rather than a force: the sweep is then monotone and the
   droop grows with the iteration count as it should.
2. **The cloth started in the file's frame, not world space.** `FlagCloth::new` seeded `pos`/`prev`
   with the authored *file-frame* positions while `step` works in world space, so the first frame
   began with a 2.4 m jump that one projection then resolved in a single step — the flag visibly
   snapped down on frame 1. A `placed` flag now forces `reset(bone)` before the first step.

# Units, animation, terrain and trees fidelity pass (§0-D)

Worker: fidelity-units (`work/fidelity-units`). Ghidra project `%USERPROFILE%\Documents\NR-s1-ghidra\NTW.gpr`
(read-only runs), script `analysis/fidelity/ghidra_scripts/UnitDecomp.java` (FidDecomp + `bytes:`/`wbytes:` raw text
search with pointer-table and scalar indexes, `ptab:` table dump, `strlist:`, `cooc:` functions using several scalars,
`funcs:`). Headless (Git Bash):
`analyzeHeadless.bat %USERPROFILE%\Documents\NR-s1-ghidra NTW -process Napoleon.exe -readOnly -noanalysis -scriptPath <...>\ghidra_scripts -postScript UnitDecomp.java <targets> <out> [maxLines]`.
Decompiled output stays in `target/tmp`; only specs are written here. Addresses are `Napoleon.exe` VAs.

Tags: CONFIRMED (code/bytes), INFERRED, UNKNOWN. Stand-ins in code: PROVISIONAL / PLACEHOLDER.

## Round 7 in one block (2026-10-06, branch `work/next/0d-units`)
Base `a1093fa`. 17 auto-save commits `4858c23 .. 4e0bc5f` (the sandbox watchdog committed the round as it went;
they touch only the files listed at the end of this section) plus one hand-written summary commit.

| # | item | verdict | evidence kept |
|---|---|---|---|
| b | tree scale byte | **CONFIRMED** it is a relative scale drawn at random inside an artist band; **INFERRED** `u8 / 128` clamped to 0.5 ..= 1.4 | `battle_terrain_install::every_scaled_tree_byte_lands_on_its_own_scale_band` (3,844,367 instances, 39 scaled lists, bytes exactly 63..=253); `data.tree_model` heights; scratch `target/tmp/0d_treescale.txt`, `0d_treescale_clamps.txt`, `0d_treegroup.txt`; three kept RE negatives in §4.1 |
| c | flag cloth | **CONFIRMED** files, pole geometry, personal-equipment route and bone 3 = `Weapon3`; **INFERRED** the frame offset; **UNKNOWN** the exe's transform | `flag_install::the_standard_bearers_flag_matches_its_pole` and `::every_verlet_logic_parses`; `euro_equipment.variant_weighted_mesh` attachment 1; `standard_bearer_flag.logic`; `FLA_StandT.anim` bone 3; `warscape_equipment_themes.euro_standard_bearer` |
| a | idle-stance test | **value set CONFIRMED** (`{0,1,2,5,6,7}` over the whole exe), **enum still UNNAMED**; training level and `man_animation_type` **REFUTED** from data | kept `target/tmp/0d_g/run1.txt` .. `run11.txt` (36 `CALL [reg+0x1B0]` sites, `0x0064F6C0`/`0x0064F710`/`0x0058EDD0`/`0x005D85D0` decompiles, the `nwbytes` negatives); probes `stance`, `manclass`, `enum`; the shipped fragments' own `// WALK IRREGULAR` comments |

Files this round: `crates/ntw_formats/src/verlet.rs` (new), `crates/ntw_formats/src/lib.rs`,
`crates/ntw_formats/src/battle_terrain.rs`, `crates/ntw_formats/tests/flag_install.rs` (new),
`crates/ntw_formats/tests/battle_terrain_install.rs`, `crates/ntw_data/examples/unit0d_probe.rs` (new),
`crates/napoleon/src/terrain/{trees,speedtree}.rs`, `analysis/fidelity/run_ghidra_0d.ps1` (new),
`analysis/fidelity/UNITS_TERRAIN_FIDELITY.md`, `docs/BACKLOG.md`, `docs/HANDOFF.md`.

## Where I am / what's next (updated with each push)
2026 - **Round 11 (0-D sandbox worker): DATA ONLY, no Ghidra.** Three of the four open items moved.
  - **(1) SHRUBS ARE DRAWN** — from their own `.spt` geometry, at every distance, because they have no
    billboards to hand over to and their geometry is complete enough to draw (**34 species / 524,295
    instances**, 0 with nothing drawable). New `SHRUB_FAR_DISTANCE` (400 m, PROVISIONAL,
    `NAPOLEON_SHRUB_DISTANCE`), `is_shrub`, `shrub_far_distance`, and `NearTrees` split into two
    independently streamed `Layer`s (trees reach `tree_far_distance` and cross-fade to their billboard;
    shrubs reach the shrub radius and **must not** fizzle, so their material gets near == far).
    Also corrected a stale claim: `trees.rs` said shrubs "are therefore not drawn yet" — they were
    already drawn as near geometry, and the real gap was only past the far distance. Round 11's survey:
    §4.3.
  - **(2) The alpha-test reference is measured.** The billboard atlas's alpha is a soft ramp, not a
    cut-out (633,547 partial texels vs 3,126 opaque over 29 atlases), so "any threshold looks the same"
    is refuted; 0.33 loses 0.4% against a true cut-out, 0.5 loses 24.8%. Still UNKNOWN which the game
    uses. §4.4. **Picture 0's direction remains UNKNOWN** — the composite map states no azimuth.
  - **(3) Per-tree rotation: closed NEGATIVE from data.** `TREE_ITEM` is closed at `Coord2d, U8, I32`,
    and the unmapped `I32` is non-zero on 391 of 3,844,367 instances, all on one preset's two outer
    lists. There is no rotation column; the angle is the renderer's. §4.1.
  - **(4) The wind now comes from the battle.** `prevailing_wind` drives both strength
    (`speed / 16`) and direction; new `db\wind_levels_tables\wind_levels` read as CONFIRMED data gives
    the game's five speeds (3/6/9/12/16 m/s) and its two directions. **And a negative:** four of the
    five shipped battle speeds are *not* on that ladder, so it is not a `wind_levels` lookup. §4.5.
  - **Still open:** the exe's own shrub LOD distances (per species in `.spt`, which we do not select by
    distance yet) and `VegetationMeshManager`'s streaming radius — the two targets behind
    `SHRUB_FAR_DISTANCE`; the alpha reference; picture 0's direction; the flat billboard lighting; and
    whether the game multiplies the tree wind's base strength by a time-varying factor (`FUN_01200be0`).
  - **Tests:** `speedtree_install` grows to 6 install tests (`every_shrub_species_generates_drawable_geometry`,
    `shrub_instances_within_a_draw_radius`, `shrubs_ship_no_extra_branch_lods`,
    `billboard_atlas_alpha_is_a_soft_ramp`), `battle_terrain_install` gains
    `the_tree_instance_i32_is_not_a_rotation`, `wind.rs` gains `the_ladder_is_the_game_s_own_five_speeds`,
    and `napoleon` gains `terrain::speedtree::tests::{the_sway_follows_the_battle_files_own_wind,
    the_strength_is_the_speed_over_the_games_strongest_wind}`. Probe: `tree_probe -- table <packed file>`
    (ASCII and UTF-16 string runs out of any packed file, at both byte alignments — how the
    `wind_levels` and `small_vegetation_climates_jct` tables were read).
- **Round 7 (0-D sandbox worker): DATA FIRST, as instructed. All three residuals moved; the tree byte and
  the flag's geometry are settled from the shipped data, the `+0x1B0` enum is narrowed and its last
  standing hypothesis refuted. 11 read-only Ghidra runs (F..Q in the scratch `target/tmp/0d_g/run*.txt`,
  out of git) on this worker's own project copy `%USERPROFILE%\Documents\NR-sb-0d-ghidra`, runner
  `analysis/fidelity/run_ghidra_0d.ps1`. Data probe `crates/ntw_data/examples/unit0d_probe.rs`
  (sub-commands `enum stance tables flagsplit manclass treescale treescale_clamps treepermap treegroup
  flagpole flagbones equipbones`).
  - **(b) tree byte: CONFIRMED that it is a relative scale; the decode is `u8 / 128` clamped to the
    picker's `MIN_TREE_SCALE` 0.5 .. `MAX_TREE_SCALE` 1.4 (INFERRED, §4.1).** Survey of every shipped
    map: 3,844,367 instances in 101 lists, of which **39 are "scaled" (449,611 instances)** and 62
    carry no byte at all. Bytes are exactly `63..=253`; **1..=62 and 254..=255 never occur**.
    1,151 bytes (0.26%) ask for a scale under 0.5 and 8,174 (1.82%) for one over 1.4. Shrubs and trees
    share one byte span although their `.spt` boxes differ 10x, so it is a multiplier, not a size
    (install test `every_scaled_tree_byte_lands_on_its_own_scale_band`, §4.1). The `I32` is 0 in every
    scaled list and 1 (256 instances) or 2 (135) in three unscaled ones -- the old "always 0" note is
    narrowed. Three RE negatives stand: no `1/255` (`0x3B808081`), `1/128` (`0x3A000000`), `255.0f`
    (`0x437F0000`), `256.0f` (`0x43800000`), `0.9/255` (`0x3B44D673`) or `0.9/128` (`0x3BE66666`)
    immediate in `0x00E00000..0x00F00000`, and the only `0.9f` there (`0x3F666666`, three functions)
    is a rectangle width in `0x00EC5E20`, not a scale. The divisor is computed from the two tweak
    floats at run time, so it leaves no constant to find.
  - **(c) flag cloth: CONFIRMED files, geometry and attachment bone (§3).** `rigid_equip_euro_flagpole01`
    in `unitmodels\euro_equipment.variant_weighted_mesh` (140 vertices, 504 indices) is the drawn pole:
    a **4.40 m rod along its own +x**, box x `-1.020..3.379`, y `+-0.038`, z `-0.046..0.040` -- so its
    origin sits 1.02 m up the pole. `mast_rigid_euro_flagpoleA_01` in `standard_bearer_flag.logic` is a
    **collision proxy** (mass 100, radius 0.072) whose 8 particles are all at y `-0.955..-0.986` and span
    x `1.021..2.183` -- **the same axis, offset `-0.967` in y**; the six `auto_<n>` ropes pin the sail's
    `u ~ 0` column to it. The sail is 42 particles / 60 triangles, flat in y, spreading **+z** 1.65 m
    (install test `the_standard_bearers_flag_matches_its_pole`). The standard bearer's variant lists the
    pole under `equipment_personal`, and `warscape_equipment_themes.euro_standard_bearer` puts
    `euro_flagpole` in the instrument column, so it is the bearer's **personal equipment**, not a unit
    piece. The equipment containers bind `rigid_equip_euro_flagpole_lod1/_lod2` to **bone 3**, and in
    the FLAG_BEARER skeleton (`Animations/MEN/FLAG_BEARER/FLA_STAND/FLA_StandT.anim`, 41 bones) bone 3
    is **`Weapon3`** (a parentless root) whose frame-0 x axis is `(0.004, 1.000, -0.002)` -- **+x is
    world up**, so the pole runs 0.62 m below the bone to 3.78 m above it. New reader
    `ntw_formats::verlet` (`crates/ntw_formats/src/verlet.rs`). STILL UNKNOWN: the transform the exe
    applies to the verlet item (the `0.967` y offset means its origin is not the bone origin), and
    whether the sail hangs from the pole or streams sideways (§3).
  - **(a) idle stance: the `+0x1B0` enum is still UNNAMED, but its value set is now CONFIRMED and the
    training-level hypothesis is REFUTED from data (§1.8, §9.6).**
    - CONFIRMED: the **complete exe-wide list of `CALL [reg+0x1B0]` sites** (36 in total; 20 in battle
      code `0x005B0000..0x00680000`, in 15 functions), so the value set is closed. It is a small
      **class code**, tested against `{0,1}`, `{2}` (`0x0064F6C0`), `{5,6,7}` (`0x0058EDD0`, `0x005D85D0`,
      `0x0066CE00`), `{1}` (`0x0064F710`), `nonzero && ==1` (`0x00655250`), and `-(v != 7) & 2` splits
      a list in two (`0x005D85D0`).
    - CONFIRMED: the class carries **both** `+0xA8` (soldier -> parent) and `+0x1B0`; `0x006A7DD0`
      (soldier slot 42) walks `this+0x670[]` calling each sub-object's `+0xA8`, and `0x0065B0A0` /
      `0x0065B0E0` walk the same `+0x66C`/`+0x670` array through `+0x98` / `+0x9C`.
    - Superseded for `training_level` (§9.1, formation4): its parser `0x00EED960` maps the six keys to 0..5
      (6 otherwise), and `0x0057E1F0` reads the level's `entity_training_levels` row. Round 6 claimed:
      CONFIRMED negative: **the exe has no string enum** for `training_level`, `drill_set`,
      `melee_animation_category` or `man_animation_type` -- `nwbytes:well_trained`,
      `drill_set_infantry_line`, `foot_bayonet`, `man_musket`, `rider_musket_sabre` are all **not found**
      in `Napoleon.exe`. Those names live only in the DB, so `+0x1B0` cannot be a direct index into one.
    - CONFIRMED from data (probe `stance`, 328 rows of `unit_stats_land`): **every** unit's men's
      animation table resolves **both** `STAND_TRAINED` and `STAND`, so the choice is made by the exe at
      run time, not by the table. `man_musket` alone serves line, light, elite, grenadiers, militia, mob,
      skirmishers and irregulars (probe `manclass`) -- which **refutes `man_animation_type`** as the
      `+0x1B0` value. `training_level` is too small (6 values, round 4) to yield 6 and 7.
      Two 7-valued columns survive as candidates (0..6 plus an "unknown" 7): `drill_set`
      (artillery, cavalry, infantry_grenadiers, infantry_light, infantry_line, infantry_melee,
      infantry_mob) and `melee_animation_category` (foot_bayonet, foot_rifle_butt, foot_sword,
      mounted_lance, mounted_slash, mounted_sword, one_handed). UNKNOWN which.
    - CONFIRMED naming: the shipped fragments label the two families themselves. `standard_bearer_fragment.txt`
      heads the second set `// MOVEMENT UNTRAINED` / `// WALK IRREGULAR`; `musket_fragment.txt` has 87
      `_TRAINED` slot lines and 97 `IRREGULAR` comments, `pitchfork_fragment.txt` 81 / 186, `pike` 39 / 1.
      So `_TRAINED` = drilled, plain = irregular -- the exe's own words.
  - Code: `TreeInstance::scale` (`ntw_formats::battle_terrain`) replaces the two copies of the byte
    decode in the view; `ntw_formats::verlet` is the new reader; two install tests added
    (`every_scaled_tree_byte_lands_on_its_own_scale_band`, `flag_install`).
  - Open next: the class behind `+0xA8` (round 6's lead, still the blocker) -- its vtable slot 108.
    Second-best: read `0x005D85D0`'s two-array split, which is the clearest statement of what the code
    *does* with 5 / 6 / 7.
- **Round 6 (0-D sandbox worker, follow-up to round 5):** residual (a) researched, Ghidra-first (7 read-only runs,
  F..M, output in `target/tmp/f_..m_*.txt`, out of git; project copy in `target/tmp/NR-s1-ghidra-copy`, out of git).
  No code changed (parent class and value semantics still UNKNOWN — nothing reached CONFIRMED).
  - CONFIRMED the real `+0xA8` parent getter: battle soldier vtable `0x0132BE3C` slot 42 is `0x006A7DD0`
    = first non-null `(*sub+0xA8)()` over the sub-object array `[+0x670]` (count `[+0x66C]`), else 0 (§9.5).
    Same getter in the intermediate vtable `0x0132B7D8` (slot 42 = `0x006A7DD0`).
  - CONFIRMED the soldier vtable lifecycle: setup `0x01333714` (`0x0061CB90`) → intermediate `0x0132B7D8`
    (`0x0059EC50`, also `0x0068C750`) → battle `0x0132BE3C` (activator `0x0058EF40` ← `0x0059EAB0` ←
    `0x005A38C0`/`0x005A1EB0`); teardown `0x005A8140` → reset `0x00623960` → back to `0x01333714`.
  - CONFIRMED the full `0x006631A0` caller list (13 code sites; the 10 unseen decompiled) and the
    `CALL [reg+0xA8]` sites in `0x005B0000..0x00680000`, incl. a second `+0xA8`→`+0x1B0` chain in `0x00655250`
    and a triple `+0x1B0` test (5/7/6) in the live soldier update `0x0066CE00`.
  - INFERRED against the training-level hypothesis: `+0x1B0` values 6/7 exceed the `unit_stats_land` #41 enum
    (0..5), and 0/1 would be mob/rabble — the opposite of "trained" (§9.5). Value semantics UNKNOWN.
  - Residuals (b) tree byte and (c) flag cloth untouched.
- **Round 5 (0-D sandbox worker):** residual (a) researched, Ghidra-first (5 read-only runs,
  output in `target/tmp/a_..e_*.txt`, out of git). No code changed (nothing reached CONFIRMED).
  - CONFIRMED the exact branch in `0x006631A0`: `unit = this->vtable[+0xA8]()`; if `unit && stance == 0 &&
    unit->vtable[+0x1B0]() in {0,1}` then stance 4 (`STAND_TRAINED_IDLE_1..6`) (§9.1).
  - CONFIRMED negatives: soldier setup vtable `0x01333714` has `return 0` at `+0xA8` (branch dead at setup);
    `0x0133AF38` slot `+0xA8` is the void update `0x0066B2F0` (cannot be the getter — would deref garbage);
    battle-unit vtable `0x01321298` has the void `0x005289E0` at BOTH `+0xA8` and `+0x1B0` (neither `this`
    nor parent is that unit class). `0x0133AF38` is installed by no instruction in `0x005B0000..0x00680000`.
  - INFERRED: the parent class carrying the real `+0x1B0` getter is still unidentified; a `+0x1B0` BYTE field
    read exists (`0x00664A80`: any `soldier[+0x1EC]->[+0x1B0] == 0` → 1). Next lead: `ins:`-enumerate
    `CALL [..+0xA8]` sites in battle code and decompile the 10 not-yet-seen callers of `0x006631A0`.
  - Residuals (b) tree byte and (c) flag cloth untouched.
- **Round 4 (fidelity-battle worker, after its own round 16):** merged with main.
  - (2) Trained idle: the training-level enum is decoded (§9.1), but it argues against the training level being the
    `+0x1B0` test, and the getter is not found.
  - (4) Tree byte: the scale tweaks are traced into the vegetation picker, but the byte decode is not found (§9.2).
  - (5) Flag cloth: not started.
  - No code changed. Next: the picker readers of `+0x48/+0x4C`; the soldier-to-unit getter used by `0x006631A0`;
    `0x00732190` for the flag.
- **PAUSED (planned pause, 05:30).** Round 3 stops at the commit with this note. Workspace build and test passed at
  ffa1eda; only notes changed since then. Round 3 status:
  - DONE: the second deployment group (guerrilla deployment, #88) and horse LODs.
  - (2) Trained vs irregular clips, PARTLY FOUND (§1.8): a standing man's idle clips come from a per-man weighted
    palette (`+0x45C`) that `0x006631A0` fills from his stance. Stance 0 uses `STAND_IDLE_1..11`; it becomes stance 4
    (`STAND_TRAINED_IDLE_1..6`) when the unit's virtual function `+0x1B0` returns 0 or 1. Next: decode that function (the
    soldier vtable `0x0133AF38` slot `+0xA8` is not the unit getter; find the object `0x006631A0` is called on)
    (INFERRED: the training level of `unit_stats_land` #41; the table holds elite, mob, poorly_trained, rabble,
    trained, well_trained) and the exe's enum order for it. Then find where the stand loop itself (`STAND` vs
    `STAND_TRAINED`) and the walk/run families take the same test (callers of `0x006631A0`; the order → stance map is
    `0x00663730`).
  - (4) Tree scale byte, NOT DONE: follow the ESF `TREE_LIST` reader from `0x00ECE7C0` (record vtable call) to the
    vegetation instance builder that reads the u8.
  - (5) Flag cloth attachment, NOT DONE: in `0x00732190` (verlet item set-up), find how the `rigid mast_*` particles
    are bound to the standard bearer's bone or flagpole piece. Their coordinates (x 1.0..2.2, y ≈ −0.97) do not match
    the flagpole piece in bone 3's frame.
- Round 2 done (merged with main at ed48cad; workspace build, test and clippy pass with no new warnings):
  1. `groupformations.bin` reader (`ntw_formats::group_formation`, all 26 templates, install test
     `group_formation_install` (now in ntw_sim/tests)) and the original's default deployment (§5.2): template choice, greedy assignment,
     element layout and pull-back into the area. The test armies use it (`setup::deploy_with_templates`); historical
     battles keep their file positions, as the exe does. It does not overlap 0-A's movement or formation work: it only
     sets start positions.
  2. Fire, reload, melee, death and knock-down clips from the model's unit state (display only; `battle/actions.rs`,
     §1.7). Screenshots: `target/tmp/keep_firing.png`, `keep_melee.png` (not committed).
  3. Unit LOD switching with the CONFIRMED 5/10/15 m thresholds (§2). FPS on Austerlitz (release, 4,350 men; the PC was
     at 99% CPU from other workers' builds, so the numbers are noisy), three interleaved pairs of 6 × 2 s samples:
     LOD off medians 57 / 56 / 71, LOD on 61 / 71 / 64. No loss, maybe a small gain. Close-up screenshots with LOD on
     and off look the same at 22 m. `NAPOLEON_UNIT_LOD=0` turns it off.
- Open list, after round 2: the selection seed source is CONFIRMED (the battle generator is seeded from the clock at
  set-up, `0x004847F0`; ours from the battle seed). The trained vs irregular choice is still UNKNOWN (row 12 lead). The
  tree u8 scale: the binary `TREE_LIST` record is read through ESF record vtables (`0x00ECE7C0`); the decode was not
  reached (no 1/255 or 1/128 constant in the vegetation code). The flag cloth frame is not started. Next: then horse LODs (`warscape_animated_lod`), the second deployment list (`unit+0x1C5`, placed 25 m
  ahead) and the AI's use of the templates (§6, `ntw_ai`).
- Round 1 done: the engine's animation slot table (864 slots, kinds, rider↔mount pairing); the per-slot clip store and its
  getter; per-man selection numbers; speed-matched clip choice with playback rate; random death/knockdown families;
  soldier LOD tweaks; flag cloth source; tree list record and tree/wind tweaks; heightfield normalisation (+ survey).
  Code: per-man alternative clips, per-frame speed level + playback rate, exe rider pairing (`unit_animation`,
  `soldiers`, `battle::view`); `Heightfield::span`. FPS on Austerlitz (release, 4,350 men, noisy shared PC): base
  34-47, branch 38-57 / 45-51: no loss.

## Resolved / still open

| # | Question (source note) | Answer | Tag | Code |
|---|---|---|---|---|
| 1 | Slot vocabulary, rider ↔ mount pairing (CAVALRY §7.2, ANIM §7.5) | §1.1: fixed table of 864 slots; `RIDER_x` pairs with `x`, except `RIDER_WALK`↔`WALK_1`, `RIDER_RUN`↔`RUN_1` | CONFIRMED | `unit_animation::rider_slots` |
| 2 | How one clip of a slot's alternatives is chosen (ANIM §7.2/§7.5, CAVALRY §7.1) | §1.2–1.3: clip `selection % count`; one 16-bit selection number per man from the battle LCG, seeded from the clock at battle set-up | CONFIRMED (seed source CONFIRMED, exact state PROVISIONAL) | `unit_animation::{alternative, SelectionRng}`, `battle::view` |
| 3 | Speed-level choice and time-scaling (CAVALRY §7.1) | §1.4: closest clip root speed, playback rate `speed / clip speed` | CONFIRMED for moving-death and mounted-attack families; INFERRED for walk/run | `unit_animation::pick_level`, `battle::view::sync_views` |
| 4 | Root motion: used or stripped? (ANIM §3, CAVALRY §7.6) | §1.6: the model samples each clip's root track per frame (heading-rotated deltas) | INFERRED (strong) | doc only; view still loops in place (PROVISIONAL) |
| 5 | Random families (death, knockdown, attacks) | §1.5: `{first slot, count}` table at `0x01452100`; knock-downs pick with the battle LCG | CONFIRMED (table), INFERRED (use per family) | doc only (no death clips in the view yet) |
| 6 | Soldier LOD distances (ANIM §7) | §2: `variant_lod1/2/3` = 5 / 10 / 15, compared squared; `override_variant_lod` = on; `variant_lod_skip` = 2 by unit-detail option | CONFIRMED values, distance unit INFERRED (metres) | not used yet (battle draws lod 0) |
| 7 | Standard bearer's flag cloth (CAVALRY §5, §7.4) | §3, §3.1, §3.2: the verlet cloth `RigidModels/VerletItems/standard_bearer_flag.logic` (reader `ntw_formats::verlet`, solve `ntw_sim::battle::cloth` since round 9), the drawn pole `rigid_equip_euro_flagpole01` on the bearer's personal equipment, bone 3 = `Weapon3` whose +x is world up, texture `flag_<key>.tga` from the plain-text `flags.tai` atlas else `flag_default.tga`. **Drawn** in `napoleon::battle::flag` | files, geometry, bone, the **zero-rest-length welds**, the `.tai` format and the faction keys all CONFIRMED; the frame offset CONFIRMED (0, -0.967, -0.075); the exe's own transform UNKNOWN, ours is `bone3_at_ground() ∘ mirror_z`; wind and solver constants PROVISIONAL | `napoleon::battle::flag`, `ntw_sim::battle::cloth` |
| 8 | Tree-list per-instance data (BATTLE_TERRAIN §9, SPEEDTREE NEXT) | §4.1: item = x, y, u8 `scale`, source; **no per-tree rotation is stored** | CONFIRMED | doc only |
| 9 | Tree scale byte → scale | §4.1: a **relative scale** (shrubs and trees share one byte span), a random draw in an artist band; `u8 / 128` clamped to the picker's `MIN_TREE_SCALE` 0.5 .. `MAX_TREE_SCALE` 1.4; bytes are exactly 63..253 over 3.84 M instances | CONFIRMED (that it is a scale, and the range); decode INFERRED, `/255`-span rival not excluded | `TreeInstance::scale` |
| 10 | Tree/wind tweak defaults (SPEEDTREE §6–7) | §4.2 | CONFIRMED values | doc only |
| 11 | Heightfield formula (BATTLE_TERRAIN §8) | §5.1: `normalize` rescales by the level's own sample range; every shipped level is normalised, from 0 to 65534 or 65535 | CONFIRMED structure + survey, INFERRED per-sample step | `battle_terrain::Heightfield::span`, install test `every_battle_map_parses` |
| 12 | Trained vs irregular slots | §9.6, §9.8: the `_TRAINED` slots are the **drilled** set and the plain ones the **irregular** set (the shipped fragments say so). The exe picks with `unit->+0x1B0() in {0,1}`; the whole call list is closed (36 sites) and the value set is **`{0,1,2,3,5,6,7}`** (round 8 corrected it: `0x0064F790` tests 3, and 4 never occurs). It is a **unit class code**, read through a reciprocal-parent back link at `+0x214`. The training level and `man_animation_type` are both refuted from data; `drill_set` and `melee_animation_category` survive as UNKNOWN candidates | value set and shape CONFIRMED, **enum UNNAMED — CLOSED-ATTEMPTED in round 8** | `_TRAINED` preferred (PROVISIONAL) |
| 13 | Per-man animation phase | not found | UNKNOWN | hash (PROVISIONAL) |
| 14 | Default deployment layout (BATTLE_TERRAIN §10, setup PLACEHOLDER) | §5.2: `groupformations.bin` template chosen, units assigned greedily, laid out by element rules, pulled back into the area | CONFIRMED (area half-depth, group union, unit order INFERRED) | `group_formation::{choose, assign, layout}`, `setup::deploy_with_templates` || 15 | Fire, reload, melee, death, knock-down clips (CAVALRY §6) | §1.7: engine slot families played from the model's unit state | slots CONFIRMED, staging PROVISIONAL | `battle::actions`, `battle::view` || 16 | Soldier LOD switching in the battle | §2: 5/10/15 m on the squared camera distance, per man, only on a change | CONFIRMED values | `unit_model::unit_lod`, `view::PartLods` || 17 | Unit class ids used by templates | §5.2: alphabetical class list, unknown → 0 | CONFIRMED | `group_formation::UNIT_CLASSES` |
| 15 | Fire, reload, melee, death, knock-down clips (CAVALRY §6) | §1.7: engine slot families played from the model's unit state | slots CONFIRMED, staging PROVISIONAL | `battle::actions`, `battle::view` |
| 16 | Soldier LOD switching in the battle | §2: 5/10/15 m on the squared camera distance, per man, only on a change | CONFIRMED values | `unit_model::unit_lod`, `view::PartLods` |
| 17 | Unit class ids used by templates | §5.2: alphabetical class list, unknown → 0 | CONFIRMED | `group_formation::UNIT_CLASSES` |
| 18 | Second default-deployment group (unit `+0x1C5`) | §5.2: `unit_stats_land` #88 guerrilla deployment, placed 25 m ahead with the same template | CONFIRMED | `setup::deploy_with_templates` |
| 19 | Horse LODs (CAVALRY §6) | §2: `warscape_animated_lod` ranges, first that covers the distance | CONFIRMED loop, metres INFERRED | `mount::animated_lod`, `view::PartLods` |

## 1. Animation

### 1.1 The slot table (CONFIRMED, `0x013AEBE4`)
864 records of 24 bytes (`0x360` is the "none" value everywhere): `{char* name, u32 kind, u32 family, u32 mount_slot,
u32 flag, u32 id}` with `id = index + 1`. Index 0 is `MISSING_ANIM`, 1 `STAND`, 10..14 `WALK_1..5`, 19..23 `RUN_1..5`,
28 `TROT`, 33 `CANTER`, 38 `GALLOP`, 59 `STAND_TRAINED`, 62..66 `WALK_TRAINED_1..5`, 67..71 `RUN_TRAINED_1..5`, ...
then idles, combat (`COMBAT_1..90`), attacks, deaths (`DEATH_*`), riders (`RIDER_*` from 427), engines, climbing,
pikemen. Only one function reads the names (`0x00E5FA30`, name → index for the fragment parser).
- `kind`: 0 stationary loop, 2 locomotion loop, 3 transition, 5 turn, 7 step, 0xC/0xD turn-to-walk, 0xE moving
  one-shot, 0xF combat, 0x11 action, 0x12 stationary one-shot, 0x14/0x15 death (stand / moving), 0x16 rider one-shot,
  0x17 rider combat, 0x18 rider loop, 0x19 rider turn, 0x1B rider action, 0x1C rider state, 0x1D/0x1F jump deaths
  (INFERRED names from the slot names). The 864 kinds themselves are read from the table (CONFIRMED, polish16
  2026-10-14) into `battle_animation::SLOT_KINDS` (test `slot_kinds_match_the_exe_table_for_the_slots_the_display_plays`).
- `family`: index of the family's base slot (`WALK_2`'s is `WALK_1`).
- `mount_slot`: for a `RIDER_*` slot, the mount slot it plays with. All are `RIDER_<x>` ↔ `<x>`, except
  `RIDER_WALK` ↔ `WALK_1` and `RIDER_RUN` ↔ `RUN_1`.
- `flag`: 1 on stands, turns, one-shots and every rider slot; 0 on locomotion, steps and moving one-shots. INFERRED:
  1 = the clip's root motion does not move the entity.
- After the table: a transition table (`0x013B3CE0`-ish, used by `0x00E60260`) and the skeleton names
  `man 0, horse 1, camel 2, elephant 3, artillery 4, gun_train_2 5, gun_train_6 6, ammo_caisson 7`.

### 1.2 The clip store and its getter (CONFIRMED)
An animation set (built per table by `0x00E60260`) keeps at `+0x2C` an array of 864 slot records of 24 bytes:
5 clip pointers and a count (so a slot holds at most 5 alternatives: the fragment lines that repeat its name).
`0x00E5F760(slot, n)` returns clip `n % count` (slot 0's if the slot is empty). 43 callers.

### 1.3 Selection numbers (CONFIRMED arithmetic)
The soldier entity set-up (`0x0061CB90`) draws `s = s * 0x343FD + 0x269EC3` from the battle object's generator
(`battle + 8 → +0x50`, shared by all entities; seeded at battle set-up from `timeGetTime()`, or from the
`constant_random_seed` option, CONFIRMED `0x004847F0` → `0x00511210`) and stores `s >> 16` at entity `+0x1E0`. Every clip lookup of that
man passes `+0x1E0` as `n` (`0x005BE800`, `0x005E4B20`, `0x00663B70`, `0x0066CE00`, ...). So one number per man picks
the alternative of every slot. (A state exit, `0x00623A70`, restores `+0x1E0` from `+0x5E4`.) The draws made
before the men are UNKNOWN. Code: `SelectionRng` seeded with the battle's seed (the same source; state PROVISIONAL), one
draw per man in spawn order.

### 1.4 Speed-matched clips (CONFIRMED for two families)
`0x006611E0` (death while moving, slots `DEATH_MOVING_1..12`) and `0x005B7B20` (mounted attack while moving) loop over a
slot range; for each slot with clips: clip = getter(slot, `+0x1E0`), its speed = root displacement at t = 0.1 s
(`0x010F1160`, frame lerp) × 10; keep the slot with the smallest `|clip speed − entity speed|` (entity speed at
`+0x14C`) and return `rate = entity speed / clip speed`. Walk / run levels do not use this rule: they follow the
locomotion graph (§1.10: up at midpoints, down at quarter points; `0x007CEAA0` / `0x007CE840`, CONFIRMED).
Code: each figure's level comes from the unit's speed over its last moving model tick(s) (`view::GroundSpeed`, the
model's equivalent of the entity speed `+0x14C`), through `view::on_levels` / `on_ladder` (§1.10). Until 2026-10-07 the
speed was smoothed from the per-frame displacement, which swings every 0.1 s tick because the model only moves units on
ticks: the men switched level and rate several times a second (the walking jitter, first bad commit `e5c08bcb`).
Since 2026-10-08 the speed is observed after every model tick (`view::observe_ticks`, `FixedUpdate`), so a frame
spanning several ticks no longer averages a moving tick with still ones. Only a tick in which the model moved the unit
(`LandUnit::moved`) on open ground before and after it counts: a reinforcement is placed and walks a step in the same
tick (it made the men run); that tick, when it comes onto the field, is skipped, as is a deployment drag (no tick). The views
carry the `BattleSim::build` they were made for, so a restart (R) rebuilds them even with the same unit ids.

### 1.5 Random families (CONFIRMED table, `0x01452100`)
Pairs `{first slot, count}`: `MOUNT_ATTACK` 5, `RIDER_ATTACK` 10, `RIDER_ATTACK_BLOCKED` 5, `RIDER_CHARGE_ATTACK` 5,
`DEATH_STAND` 13, `DEATH_STAND_TRAINED` 10, `DEATH_WALK` 5, `DEATH_MARCH` 5, `DEATH_RELOAD` 5, `DEATH_POISED` 10,
`DEATH_RUN` 7, `DEATH_RUN_TRAINED` 5, `DEATH_CHARGE` 7, `DEATH_ALT_STAND1..3` 5 each, `DEATH_KNEEL` 5,
`DEATH_KNEEL_POISED` 5, `DEATH_COMBAT_READY` 10, `DEATH_MOVING` 12, `RIDER_DEATH_STAND` 5, `COMBAT_IDLE` 10,
`KNOCKBACK` 5, `KNOCKDOWN` 5, `REFUSE` 4, ... For `KNOCKDOWN` the death chooser steps the battle generator and takes
`min(count · (s >> 16) / 0xFFFF, count − 1)`.

### 1.6 Root motion (INFERRED, strong)
`0x0061E9E0` builds, for the clip a man starts, a per-frame table of root deltas (position and heading, rotated into
the entity's heading) from the clip's root track; the soldier update (`0x0066CE00`) steps a frame counter (`+0x1C0`)
against the clip's frame count (`+0x50`) and uses the clip root for one-shots (kinds 0x14/0x15). So the model moves
men by their clips' root motion; the view's in-place looping stays PROVISIONAL.

### 1.7 Action clips in the battle view (slots CONFIRMED, staging PROVISIONAL)
The view (`battle/actions.rs`) reads the model's unit state and plays the engine's slots: `COMBAT_READY` / `AIM` while a
shooting unit has a target; on each volley every living man plays `FIRE` (after a per-man delay of up to 0.35 s) then
`RELOAD_1` or `RELOAD_2`; in melee `COMBAT_IDLE_n` loops with `ATTACK_n` now and then (riders: `RIDER_` slots, the horse
keeps `COMBAT_IDLE_n` or its gait clip when the rider slot has no mount pair); the men the model removes play a death from
the family that fits what the unit was doing (`unit_animation::death_family`, INFERRED; `DEATH_MOVING_n` matched by speed
first, CONFIRMED rule) and stay where they fell (detached, last frame held); when a unit is caught by charging cavalry,
about 3 in 10 front-rank men play `KNOCKDOWN_n` then `FACE_DOWN_GET_UP`. Family members are drawn with the knock-down
formula (§1.5); alternatives by selection number (§1.3). One-shots keep their root motion; loops have it removed.

### 1.8 Stand idles and the trained palette (CONFIRMED tables, test INFERRED)
`0x006631A0(stance)` fills a man's weighted idle palette (`+0x45C`, drawn with the battle generator by
`0x0080E020`, then played by `0x006666E0`) from a `{first slot, count}` per stance (`0x014520B0..`): stance 0
`STAND_IDLE_1..11`, 1/2/3 `STAND_ALT_n_IDLE_1..5`, 4 `STAND_TRAINED_IDLE_1..6`, 5 `CROUCH_IDLE_1..5`, 6
`STAND_FOR_STOKE_IDLE_1..5`, 7 `STAND_NO_WEAPON_IDLE_1..4`; 0x3C/0x3D the wounded front/back sets with weights
0.9/0.02/0.02/0.02/0.04 and 0.9/0.02/0.02/0.06. Weight 1.0 elsewhere; slots without clips are skipped. Stance 0 is
replaced by stance 4 when the unit's virtual function `+0x1B0` returns 0 or 1. That function is **still not named**;
§9.6 closes the value set and refutes the training level from data. Not used in code yet.

### 1.9 Clip changes: the display blend and phase (CONFIRMED rule; gait-blend, 2026-10-08; gait-blend2, 2026-10-09)
User report: in battle the walk -> run and run -> walk change snapped from one clip to the other. Traced statically:
- **Two layers.** The sim entity update (`0x0066CE00`, once per 0.1 s tick) steps a frame counter (`+0x1C0`) through a
  clip sampled at 10 frames/s (the clip object's `+0x50` = round(anim duration x 10), `0x00E4F4D0`) and moves the man
  by the clip's root motion times `+0x448` (`0x0066EEB0`; default 1.0 from the constructor `0x0061CB90`). **Correction:**
  MIDDLEWARE_VERIFY's "blend 1.0f at +0x448" (`0x006666E0`) is this root-motion scale, not a blend. The pose that is
  drawn is the per-soldier display object's (built by `0x007179B0`, which reads `ENTITY_DISPLAY_SHARED_POSE_TABLE`),
  updated every rendered frame by `0x007725D0` (11,856 bytes).
- **The clip object** (`0x00E4F4D0`, one per fragment line): `+0x1C` the slot kind (§1.1), `+0x20` the line's
  `blend_in_time` (1.0 s when the line has none, parser `0x00E62760`), `+0x30` / `+0x34` a transition clip's from / to
  slots, `+0x44` the anim, `+0xB4` / `+0xB8` a list of "gear windows" `{0, start, length}` built from the anim's
  `L_FOOT_GEAR_UP_START`/`_END` and `R_FOOT_GEAR_UP_START`/`_END` events (length wraps over the clip end, `0x00E5FC40`).
- **On a clip change** (`0x007725D0`, new clip != current): the pose blend time D = the new clip's `blend_in_time`;
  when the old clip has gear windows `0x00E6DB90` replaces it (old time inside a window -> the rest of that window; a
  window starting within the next frame -> the wait plus the window). The root blend time Dr = 0.5 s (`0x01318038`)
  when D is the clip's own `blend_in_time`, else D. Elapsed e = 0. Vanilla data (`real_install::gait_blend_survey`):
  **no clip under `animations/` has gear markers**, so a vanilla gait change blends over the new clip's
  `blend_in_time`: `WALK_n` / `RUN_n` / `*_TRAINED_n` lines say 0.5 s (most) or 0.25 s, 1.0 s when absent.
- **Start time of the new clip:** when the new anim loops (anim `+0x44` mask 0x2, the flag that also exempts a clip from
  the end-of-clip test) and the old clip's root speed (anim `+0x60`) is above 0.1 m/s (`0x0131A7B0`): new time =
  (old time mod old duration) / old duration x new duration (CRT fmod `0x01285DD0`), so the step phase carries over.
  Otherwise 0 (deaths and transition kinds have their own cases). The loop flag is set by slot kind when the anim
  is bound (`0x00E4F4D0`, CONFIRMED polish16 2026-10-14): mask 0x2 for kinds 1, 2, 4, 6, 7, 8, 0xA, 0xE, 0x18 and
  0x1C, so a stand (kind 0) or a transition (kind 3) starts at 0. Ours: `view::start_time` with
  `battle_animation::kind_loops` (the kinds in `SLOT_KINDS`); PROVISIONAL: a figure's first clip, a looping clip
  the phase does not carry into and the sim-clocked kinds (below) start at the figure's own offset (`skin::phase`).
- **Every frame:** w = e / D. If D > 0, w < 1 and a previous output pose exists, the drawn pose is the per-bone blend
  (`0x010CDB20`: translations lerped, rotations nlerp or slerp by a mode) of the **previous frame's drawn pose** and
  the new clip's pose at weight w; else the new clip alone. The result is kept as the next frame's previous pose
  (pose cache `+0x18`), then e += frame time (ms x 0.001, `0x01318030`). The display root (position, heading) is lerped
  the same way at e / Dr. So the pose at the switch keeps weight prod(1 - e_k / D) over the frames: the first frame
  still shows it whole, and it is gone once e >= D. Frame-rate dependent in the original too.
- **Playback rate:** a locomotion clip (kind 2) advances by frame time x speed / its root speed (anim `+0x60`) x the
  display's playback scale (`+0xB0`, below), with no clamp or zero guard (other kinds: "Clip advance by slot kind", below); a transition clip (kind 3, `WALK_TO_RUN` ...) by speed over the lerp of its
  from / to clips' root speeds (clamped by `0x01318048` / `0x01318060`, not read), and at its end the to-slot clip
  follows with no blend.
- **Per-display playback scale `+0xB0` (CONFIRMED, gait-blend3 2026-10-11).** The display constructor `0x007179B0`
  makes two draws from the display manager's LCG seed (manager `+0x8`, MSVC constants `0x343FD` / `0x269EC3`): first
  (seed >> 16) x 1.5259022e-05 into the display input's `+0xD0`, then `DrawDisplayPlaybackScale` (`0x0078B8A0`, call
  at `0x00717E46`): (seed >> 16) x 1.5259022e-06 + 0.95, a scale in [0.95, 1.05], stored at `+0xB0`. The update
  `0x007725D0` reads it once (`0x00772E3D`), for clip kind 2 only, and multiplies it into that advance. Kind 2 is the
  moving gait loops (slot table `0x013AEBE4`, kind at record `+4`: `WALK_1` 10, `TROT` 28, `CANTER` 33, `GALLOP` 38,
  `CHARGE` 125); stands (`STAND` 1, kind 0), action loops (`COMBAT_READY` 88, `AIM` 126: kind 0; the combat loops
  `0xF` / `0x11` on the soldier's clip clock; the rider's `0x17` takes the mount's clip time) and transitions (kind 3) do
  not take it. So the men and horses of one unit on one
  loop drift apart by up to 10 % of the clip rate, which is what keeps a cavalry unit out of lockstep (user check
  2026-10-10: "every horse steps at the same time"). The seed's origin is UNKNOWN (no write to manager `+0x8` found in
  the traced code). Ours (`view::playback_scale`, `display_scale`, one per figure in spawn order: men, then the
  bearer): one discarded draw for `+0xD0`, then the scale, multiplied only into a moving gait loop's advance
  (`loop_advance` with `LoopKind::Gait`, keyed on the kind of the clip shown: `on_levels`, `on_ladder`'s loop rung
  and wrap test, the bearer's fallback loop), never while an action loop is shown over the gait, nor into stands or transitions; the
  seed is ours (`display_seed`, a stream of the battle seed). The rider copies the mount display's clip time, so a
  pair keeps one scale. Still not as the exe: the exe's per-soldier speeds and start times (each man's sim speed, his
  own STAND_TO_WALK start) are one unit speed in ours, so a unit setting off from standing takes its first step
  together; the drift then grows with the scale.
- **Which slot the display picks: the locomotion graph (CONFIRMED, gait-blend2 2026-10-09; §1.10).** The graph on the
  display (`+0xCC` graph, `+0xD0` node) is static data; §1.10 has the nodes, edge rules and slot tables. A **transition
  clip** (kind 3) is entered in one of two ways: out of the graph's node-0 slot (STAND) at once, from 0 with the line's
  blend (store `0x00773D18`); from a loop on the frame the loop's cycle wraps (its time mod its length after the
  frame's advance is at most the one before it; `[ESP + 0x118]` is the time before the advance, `0x00772CC0` /
  `0x00773CCF`), at the phase past the wrap, drawn at once (store `0x00773D5C`, blend time 0); until then the loop goes
  on. The wait holds only while the clip shown is the from-loop's (CONFIRMED, round 2 of the gait-blend2 review,
  2026-10-10): at `0x00773C83` the transition node's from-slot `+0x30` is compared with the shown node's
  (`[display +0xD4]`) slot `+0x4`; on a mismatch the display takes the generic clip change at once (`0x00773D85`:
  the same node skips to `0x00774041`, else `+0xDC` / `+0xE0` are zeroed and the clip changes with the usual phase
  carry and blend). The display keeps one clip node and one clip time; there is no separate graph clock. Ours
  (`view::on_ladder`): the wait only when the figure's last clip is the from-loop's, else a usual change (the frame a
  Ready / Melee action loop ends); while an action loop is drawn the ladder skips its transitions and steps between
  loops (PROVISIONAL, ours: in the exe the graph's clip is the one shown, so the case does not arise there). Its rate is speed over the lerp of its from-
  and to-slots' root speeds by the fraction played (at least 0.1 m/s), held to 1..10 (the constants `0x01318048` /
  `0x01318060` the old notes left unread). At its end its to-slot loop follows at the time past the end, drawn at once
  (store `0x007740A9`, blend 0). Store `0x00773FF7` only ever stored stand and idle clips in the sitting. **Debugger
  sitting 2026-10-09** (original, custom battle, the user ordering infantry then cavalry walk -> run -> walk; stores
  A `0x00773D18`, B `0x00773D5C`, C `0x00773FF7`, D `0x007740A9`): horses climbed STAND -> STAND_TO_WALK (A) -> WALK
  (D) -> WALK_TO_TROT (B) -> TROT (D) -> TROT_TO_CANTER (B) -> CANTER (D) -> CANTER_TO_GALLOP (B) -> GALLOP (D), the
  node `+0xD0` the same within each pair (static `0x0150CB50`, `0x0150BFBC`, `0x0150CB98`, `0x0150C258` = nodes 1, 4, 8,
  12). "Some went gallop -> canter_to_gallop again on slowing": the horse table has **no step-down clips** (nodes 7, 11,
  15 map to no slot), so a slowing horse goes loop to loop with a blend; entering CANTER_TO_GALLOP again needs the
  speed back above the canter / gallop midpoint (7.72 m/s), e.g. a rider catching up his place; a transition never
  plays backwards (its rate is at least 1). No infantry or rider locomotion clip reached the four stores: for riders
  the static trace agrees (a rider display's graphs, `0x007A7120` -> slot tables `0x0133D4E0` / `0x0133D5A8`, have only
  stand, step and turn slots; what moves a rider's legs with his horse is not traced, ours pairs `RIDER_<mount slot>`);
  for men it does not (the man display's graphs, `0x007A7190`, carry `WALK(_TRAINED)_1..5` / `RUN(_TRAINED)_1..5` and go
  through the same function and stores). Next sitting: break on `0x00773D18` and `0x00773D5C` with the display's vtable
  `0x01341E20` (a man) and log clip `+0x04` / `+0x1C`, during an infantry walk -> run order.
- Ours (`battle/view.rs` `ClipBlend`, `skin::Fade`): every figure (man, mount pair, standard bearer) keeps its own
  loop clip and clip time, as each exe display does. A loop clip change starts a blend over the new line's blend-in
  time (`FragmentClip::blend_in`, the one place the 1.0 s default lives) from the frame the figure drew last frame,
  weight prod(1 - e_k / D) as above; a moving -> looping change keeps that figure's normalised phase, tested on the old
  clip's own root speed (`Anim::root_speed` > 0.1 m/s; a rider pair carries when either clip moves); else the figure
  restarts at its own phase offset. A figure not drawn the frame before (it or its unit skipped; frame-numbered,
  `SkinState::frame`), or D = 0, snaps, and a skip ends a running cross-fade (the skipped frame drew the slot without
  it). The rider and the mount blend **each on its own** (`PartFade` per part, each over its own line's blend time,
  the mount's from the mount fragment): a mount clip changing alone (in melee the man keeps his idle clip while the
  horse follows the gait) cross-fades the horse only. The pair keeps one clip time (the paired clips have the same
  frame count). A ladder step (§1.10) can start the clips at a given time, drawn at once or cross-faded
  (`LoopClip::start`), as the exe's stores do.
  PROVISIONAL (ours): (1) the GPU keeps at most **four** poses frozen at clip changes per part (`skin::FROZEN`),
  with their weights, not the previous frame's blended pose, so the new clip's earlier frames do not linger as in the
  recursive blend. A change is never delayed: it freezes the pose drawn last at the old clip's weight (dropped when 0,
  a change on the frame after a change) and keeps the older poses; past four, the lightest pose's weight goes to its
  time-neighbour (the next newer pose, or the next older one when it is the newest), so the sum, and so the live clip's
  weight, does not move; all are gone one blend time after the last change. (An earlier "third change waits" rule
  stuck for ever after back-to-back changes: round-2 review.) **The run -> walk hiccup (user check 2026-10-10,
  gait-blend3):** a run -> walk order enters the walk graph on its fastest level and the sim speed then falls by the
  deceleration per 0.1 s tick (infantry 5 m/s², 0.5 m/s a tick), so the display steps down a level about every tick:
  four or five clip changes inside one 0.5 s blend. The old two-pose limit dropped the lightest pose (about a quarter
  of the weight) on each third change and scaled the others up: a visible jump. Four poses hold a whole run -> walk
  slow-down (test `a_run_to_walk_keeps_every_frozen_pose`), and the merge leaves no jump when a longer one exceeds
  them.
  (2) Bone matrices are lerped in model space (no quaternion slerp). Neither showed in the logged runs below as a
  visible step. (3) The exe also lerps each display's root at e / Dr (Dr 0.5 s, `0x01318038`). Ours has no display
  root of its own to lerp: a figure stands at its formation place, which moves with the unit's drawn pose
  (`DrawnPose`); loop clips have their root motion removed, so a clip change moves no root; the root bone's own offset
  is part of the pose and fades over D (equal to Dr for the 0.5 s vanilla gait lines). (4) One-shots (fire, reload,
  melee, deaths) are not blended, and a one-shot drawn ends a running cross-fade. (5) A standard bearer playing the
  kit's per-gait fallback clip (no levels) snaps. Not done
  (Polish/BACKLOG): the gear-window override `0x00E6DB90` for mod clips with `*_FOOT_GEAR_UP_*` events.
  Cost (hot path): the per-frame figure upload stays at 4 words per slot; only fading figures get a 64-byte fade entry
  (four poses; 32 bytes before gait-blend3),
  written by the render world into the start of a fade buffer (`skin::FadeUpload`, partial `write_buffer`), so a frame
  without a cross-fade uploads nothing extra. Release bench of the upload path (serialise + render-world copy +
  staging copy, best of 5 x 3000, this machine): 13,000 slots (a 20 v 20 unit battle at 160 men, man + mount slots)
  4 words 0.010 ms vs 0.018 ms for the earlier 8-word slots; 52,000 slots 0.065 ms vs 0.447 ms; the fade entries for
  every slot fading at once 0.011 ms (13,000) / 0.083 ms (52,000), for a tenth of them 0.001 / 0.005 ms (measured at
  32-byte entries; not re-measured at 64). The graph
  step per figure (§1.10) allocates nothing: a few compares on the level speeds.
  Tests: `battle::view` (weights 1, 0.75, 0.375, 0.09375, 0 for D 0.5 s at 8 frames/s; 1.0 s fallback;
  per-alternative blend time; a figure whose clip did not change keeps its time; a change mid-fade keeps the older
  pose; back-to-back and 3-4 quick changes end on the newest clip within one blend time; a pose past the limit gives
  its weight to its neighbour; a run -> walk slow-down keeps every frozen pose; the playback scale follows the exe's
  draw and figures on one loop drift apart by it; the ground speed lerped between ticks; the mount clip followed while the man's stays; the mount blends its own changes; a mount change alone
  updates the pair's speed; the phase carries by the old clip's own root speed; a figure not drawn last frame snaps; a
  skip mid-fade ends the fade; one-shots; fade numbering and the pole bone posed as the shader does; a horse climbs
  its ladder through the transition clips; a man changes graph with the order and level with hysteresis) and
  `unit_animation::level_clips_carry_the_lines_blend_in_time`.
- **Cavalry hitch (gait-jitter, 2026-10-11).** The user saw cavalry hitch and surge after gait-blend3. An
  env-gated per-figure probe (since removed; 42,450 frame lines, 2 cavalry units, walk → run → walk) found three
  causes, all fixed:
  1. *Loops over n intervals (CONFIRMED, `0x010DA030` GetAnimFramePairAtTime).* The exe's frame pair on a looping
     anim (flag `+0x44` mask 0x2) is floor(t·rate) mod (n − 1) and the next key, over the n − 1 intervals of the
     anim's length `+0x4C` = (n − 1)/rate. Ours ran over n, blending the last key back into key 0: a closed gait
     loop keys the same pose at both ends, so every cycle held that pose for one frame time (50 ms) and ran
     n/(n − 1) slow (horse walk 1.15 s for 1.10, trot 0.85 for 0.80, canter 0.65 for 0.60, gallop 0.55 for 0.50,
     up to 10%): the legs lagged the ground and caught up each cycle. Ours now matches (`ClipSlot::figure`,
     `ClipSlot::duration`, `Anim::world_matrices_at`); after the fix the walk's first key of a pair never reaches
     22 (probe: max 21 of 23 keys). Test `skin::a_loop_never_blends_its_last_frame_into_its_first`.
  2. *A transition drawn one frame past its end (CONFIRMED, `0x0077404C`..`0x007740A9`).* The exe advances the
     clip time first, then compares it with the anim's length (`0x0077404C`), subtracts the length
     (`0x0077408E`) and switches to the loop node (`0x007740A9`) in the same frame. Ours tested the time before the
     advance, so the transition drew one frame past its end, wrapped to its key 0 (probe: STAND_TO_WALK at 3.062 s
     and WALK_TO_TROT at 2.751 s drew key 0); transitions are open clips (last key vs key 0 differ by 1.15–1.46
     rad at the root), a one-frame pop. Ours now leaves the transition on the frame its advanced time reaches the
     end, the loop at the time past it (probe: the last transition key drawn is 59 of 61, then loop key 0). Test
     `view::a_horse_climbs_its_ladder_through_the_transition_clips`.
  3. *The phase never carried (ours).* `carry_phase` tested the root speed of the clips drawn, which have their
     drift removed (`soldiers::in_place`), so 0 (probe: `cs=0.000` on every line): each step down restarted the
     stride at the figure's own offset (Loop1 → Loop0 at phase 0.891 → 0.268; Loop2 → Loop1 0.376 → 0.269,
     0.925 → 0.276). It now tests the level's root speed as read (`carry_speed`, `KitLevel::speed`; probe after:
     Loop1 → Loop0 0.156 → 0.165, Loop2 → Loop1 0.655 → 0.666). Test
     `view::a_level_change_carries_the_stride_by_the_levels_own_root_speed`.
  4. *Whose root speed: the figure's own alternative (CONFIRMED, gait-trace 2026-10-13).* A slot holds up to 5
     alternatives; `0x00E5F760` (the fragment slot's clip getter) returns alternative `selection % count` (a 0x18-byte
     row: 5 clip pointers, the count at `+0x14 + slot × 0x18`; an empty slot falls back to slot 0's alternatives,
     which ours skips, §1.2). Every clip lookup of the update `0x007725D0` passes the display's own selection, and the
     graph context's `+0xC` is the same value, so the rate (`0x00772E2B`), the carry at a clip change (`0x00773E2E`,
     `0x00773E85`..`0x00773E94`), the graph's up / down edges (`0x007CEAA0` / `0x007CE840`) and its entry test
     (`0x00703120`) all read the anim `+0x60` of the figure's own alternative, and the transition rate lerps its
     from / to slots' clips through the same getter (`+0x30` / `+0x34`, same selection). A rider copies his mount
     display's clip time (`0x0079BB10`), so the mount's alternative rules a mounted pair. The moving deaths
     (`0x006611E0`) measure each slot's clip through `0x00E5F760` with the entity's `+0x1E0` selection too. Ours took
     the first clip's speed for the whole level (`KitLevel::speed`, the last one's for actions); now every
     alternative keeps its own (`KitLevel::speeds` / `mount_speeds`) and `KitLevel::speed_of(sel)` feeds the rate,
     carry, climb / entry edges, the transition rate (`on_ladder`) and the moving-death pick (`actions::death`).
     Not changed: our levels are ordered by their first clip's speed, the exe's graph by its node order (the shipped
     tables agree). Test `view::each_figure_goes_by_its_own_alternatives_root_speed`.
  5. *A transition left early (CONFIRMED, gait-trace 2026-10-13): no rule of its own.* When the graph leaves a
     kind-3 transition before its end (a down edge `0x007CE840` or a new order), the display takes the generic
     change at `0x00773D85`: `+0xDC` / `+0xE0` zeroed; if the old clip's anim `+0x60` > 0.1 (`0x0131A7B0`) and the new
     clip loops (anim `+0x44` mask 0x2, `0x00773E73`), the new time is fmod(t, old length) / old length × new length
     (`0x00773EB4`..`0x00773EF3`), else 0 (`0x00773F17`; kinds 0x14..0x16 aside). The two holds before it do not
     apply to a transition: `0x00773C12`..`0x00773C63` keeps the shown clip to its end only while the display's
     stop flag `+0x114` is set (last frame's `[ESP+0x1F]`, written at `0x007743A5`, from `0x007946D0`, the
     stop / arrive clip pick: moving within 6.5 m of the target, 1.0 for slot 0x12 or the clip's `+0x74`, ×0.01 when
     above 50; the new clip then starts at `[ESP+0x14]`, `0x00773EA1`), and `0x00773BAA`..`0x00773C0E` only when the
     shown clip is kind 0xC / 0xD. So the stride carries out of a transition by the transition clip's own root speed
     and length, which ours already does (`ClipBlend::step` through `carry_speed` of the level shown, now the
     figure's alternative, item 4). The stop clip pick (`0x007946D0`) is not modelled (no stop / arrive slots in
     ours yet).
  Checked and not a cause: rider and mount share one clip time and one advance, so the playback scale
  (`0x0078B8A0`) is applied to both alike. The once-a-second
  surge on slowed ground is in the movement, not the animation (§1.10, "Measured surge").
- **Clip advance by slot kind (CONFIRMED, gait-infantry 2026-10-12).** The update `0x007725D0` switches on the
  shown slot's kind (record `+4`, slot table `0x013AEBE4`) before it advances the clip time:
  - kind 2 (`0x00772E19`..`0x00772E65`): t += speed / anim `+0x60` x display `+0xB0` x frame time, with no clamp
    and no zero guard (a standing man on a moving graph gets rate 0);
  - 0xC / 0xD (the turns into a walk) through `0x007730BC`: the same forward speed (display input `+0x0`,
    `[ESP+0xA0]`) / root speed, without the scale;
  - kind 4 (`PUSH_ENGINE_*_BACKWARDS`, `ENGINE_LIMBERED_BACKWARDS`) at `0x00772E6A`: a different speed / root
    speed, without the scale (corrected polish16-fix 2026-10-14; it was grouped with 0xC / 0xD). XMM1 there still
    holds display input `+0x8` (`[ESP+0xA8]`, loaded at `0x00772C71` after `0x00752FF0`, a `__thiscall` that pops
    its two arguments; nothing on the path from there writes XMM1). The snapshot `0x0079FF50` stores in that slot
    (ring `+0x34`, read back as ring `+0x18`) the length of the entity's `(+0x8C, +0x94)`, whose atan2 it keeps as a
    heading (ring `+0x3C`): a horizontal speed whatever the facing, which stays positive for these backward moves
    where the forward speed `+0x14C` would not. `(+0x8C, +0x90, +0x94)` is the entity's measured velocity
    (CONFIRMED polish17 2026-10-14): the motion integrator `0x007F08F0` adds the tick's displacement `+0xD4..+0xDC`
    to the position `+0x48..+0x50` and then stores that displacement x 10 there (`0x007F12AD`..`0x007F12F2`, the
    0.1 s tick), and the placing move `0x0080BAC0` stores (new position - old) x 10 (`0x0080BAEC`..`0x0080BB30`).
    A tick without a displacement skips the add: `BL` is set only when the squared length of `+0xD4..+0xDC` is
    above 0 (`0x007F0DF3`..`0x007F0DFB`), and `0x007F0E26` jumps on `BL != 1` past the add and the move tests to
    the bounds check at `0x007F102C`, which still reaches the store (`0x007F1083` -> `0x007F129B` -> `0x007F12AD`):
    the velocity is then that zero displacement x 10, 0, the same as the add path would give (CONFIRMED polish18
    2026-10-14). Ours: a man that did not move over the tick has ground speed 0.
    So kind 4 divides the distance the man moved over the last tick, per second. Ours feeds the same quantity, the
    ground speed (distance moved over a tick, [`GroundSpeed`] in `view.rs`, each man's own through `MenDrawn`), to
    both kinds (CONFIRMED for kind 4; no kit shows a kind-4 clip, so there is nothing to look at in game);
  - kind 3: the transition rule above;
  - kinds 0, 1, 0xE and 0x20 (`0x00773086`): t += frame time (rate 1, no scale);
  - every other kind (the melee idles 0xF, 0x11, 0x17 ...): no advance in this switch (`0x00773080` jumps to
    `0x00773237`). Kinds 0xF..0x13 (CONFIRMED polish16 2026-10-14) take their time from the soldier's clip
    clock instead, set each frame at `0x0077430C`: the entity's clip frame count (`+0x1C0`, copied by the
    snapshot `0x0079FF50` into the display input, `0x00752FF0`) x 0.1 s plus the in-tick time (`+0x28230` ms x
    0.001, `0x005F5530`), and 0.1 s more for kind 0xF. So they play at rate 1 (no speed, no playback scale) from
    the start the sim sets. The same second switch (`0x00774274`, byte table `0x00775430`, jump table
    `0x00775420`: kinds 0xF..0x13 to the clock `0x0077430C`, 0x16..0x1A to `0x00774282`, 0x1D to `0x007742C0`,
    0x14 / 0x15 / 0x1B / 0x1C to the exit `0x00774398`, where the out-of-range kinds go too) sends the rider's
    kinds 0x16..0x1A (his refusals, gait transitions, steps, combat idles, stand idles, deaths, mount and dismount
    = 0x16, `RIDER_COMBAT_n` / `RIDER_MOUNT_ATTACK_n` = 0x17, gait loops and jumps = 0x18, turns = 0x19 / 0x1A) to
    `0x00774282`, which copies the mount display's (`[ESI+0xCC]` -> `+0x14`) clip time (`+0xD8`) and flag `+0x115`
    (CONFIRMED polish16-fix 2026-10-14): so kind 0x17 plays the mount's clip time. Kind 0x1D (the rider's jump
    and chevaux-de-frise deaths) goes to `0x007742C0`, which copies the same clip time and `+0x115` and also the
    mount's `+0x11C` (`[ESI+0xCC]` -> `+0x11C`, at `0x007742E1`..`0x007742ED`) (CONFIRMED polish17 2026-10-14).
    Ours already shares one clip time between rider and mount and keys a mounted level on the mount's slot
    (`soldiers::level_kind`).
    Checks before that switch (`0x00774228`..`0x0077425D`, CONFIRMED polish17 2026-10-14), on the entity's state
    (`+0x1B8`, the snapshot ring's `+0x24` via `0x0079FF50` / `0x00752A20`, `[ESP+0x5C]`) and requested slot
    (`+0x1C4`, ring `+0x30`, `[ESP+0x68]`): state 0x15 or 0x16 sets the clip time to the shown clip's end (the clip
    object's `+0x44` -> anim length `+0x4C`, `0x00774386`..`0x00774392`) and skips the switch; else a slot in
    0x2FA..0x301 (`DEATH_JUMP_1`..`_8`, kind 0x1F) skips it with the time untouched; else state 0x2A, 0x4B or 0x17
    goes to `0x007742A8` instead of the switch: kinds 0x16..0x1A copy the mount's clip time without the `+0x115`
    flag (`0x007742F8`, stored at `0x00774392`), kind 0x1D goes to `0x007742C0`, and every other kind (kind 2's
    gait loops too) takes the clip clock `0x0077430C`. Ours: `battle_animation::display_time` holds the whole
    rule, these checks and the kind switch after them (`JUMP_DEATH_SLOTS` = 0x2FA..0x301; byte table `0x00775430` =
    0xF..0x13 clock, 0x14 / 0x15 exit, 0x16..0x1A mount time and flag, 0x1B / 0x1C exit, 0x1D mount time, flag and
    `+0x11C`; clock constants 0.1 at `0x0131A7B0`, 0.001 at `0x01318030`; CONFIRMED polish18 2026-10-14, test
    `display_time_follows_the_pre_switch_checks_then_the_kind_switch`). The battle view has no entity anim state
    (`+0x1B8`), so it does not call it yet. What the states stand for is open: 0x17 (and 0x18) are written by
    `UpdateSoldierSpeedMultiplierAndState` (`0x00654518`..`0x00654590`, when `+0x118` == 1: 0x17 with `+0x114` == 1,
    slot `+0x44C`, frame `+0x450` + 1), 0x2A / 0x4B by `0x00653A10` (`0x00653F0B`..`0x00653F39`, slot 0x360 = none,
    by the thresholds at `0x014521C8` / `0x014521CC`), 0x2A also by `0x0066CE00`, `0x00679300` and `0x00679F50`;
    0x15 / 0x16 come only from data, through `+0x630` (`0x00662E90`, copied to the state by `0x0067A050`; callers
    `0x00672580`, `0x006735E0`, `0x00672B80`, `0x00672F40`), and no instruction stores or compares them at `+0x1B8`
    directly (byte search, polish18).
  The men's graph slot tables name the stand rung's clip: `0x0133C608` (walk graph) and `0x0133D030` (charge graph)
  both map node 0 to `STAND` (slot 1, kind 0); the charge graph's node 2 is `CHARGE` (slot 125, kind 2). So a
  moving graph that has dropped to its stand rung plays the stand clip at rate 1, not the walk level slowed down.
  Ours: `view::LoopKind` (`Gait` = kind 2, `Paced` = kinds 4 / 0xC / 0xD, `Plain` = the rest) and
  `view::loop_advance` / `advance_by`, the one helper every loop calls (`on_levels`, `on_ladder`'s loop rung and wrap
  test, `bearer_loop`'s fallback), keyed on the kind of the clip shown (`KitLevel::kind` / `GaitAnims::kind` from
  `battle_animation::SLOT_KINDS`, the mount's slot for a mounted level): an action loop shown over the gait advances
  by its own kind, and a moving graph's stand rung plays `STAND` (kind 0) at rate 1. The rate is `loop_rate` =
  speed / the level's read root speed, no clamp (1 only for a level with no read root speed). The melee idles
  (`COMBAT_IDLE_n`, kind 0x11) play at rate 1 (CONFIRMED, `0x0077430C` above); their start is the figure's own
  offset (PROVISIONAL: ours keeps no per-soldier clip clock).
- **Infantry hitch (gait-infantry, 2026-10-12).** The user (2026-10-11) still saw a slight hitch on infantry after
  gait-jitter (cavalry passed). Probe: release `--battle --ai off`, two line infantry units ordered to walk, run
  (10 s), walk (16 s) and stop (26 s); one log line per figure per frame (clip, level, phase, rate, speed), ~28k
  frames.
  - *Unit 1, flat ground: smooth.* Rate 1.13-1.14 on the 1.27 m/s level, the phase carried across every level
    change (gait-jitter's carry holds for men), no wrap pop beyond the clip's own data.
  - *Unit 2, uneven ground: the hitch.* The unit speed steps at the 11-tick re-plans (§1.10) and the ground
    multiplier steps: 17.2 s 1.406 → 1.519 m/s; 19.3-19.4 s 1.463 → 1.588 → 1.665; 20.6 s 1.593 → 1.388;
    22.0 s 1.383 → 1.212 (multiplier step). Each step changes all 160 men's cadence in one frame, and the larger
    ones cross the 1.27 ↔ 1.73 level midpoints, so the whole unit swaps level at once, each swap with the line's
    0.5 s blend (the pose at the switch fading out). That is the hitch seen. The exe moves each soldier by his own
    distance and his own ground multiplier (`+0x1A4`), so its speed steps and level swaps fall on different frames
    per man; ours moved the unit as one. Fixed in the model: each man's own timed move (§1.10, "Each man's timed
    move").
  - *Walk → run:* the jog plays at about 0.46 rate while the unit accelerates; the exe does the same (kind 2, speed
    over root speed). Not a cause.
  - *Loop closure:* some infantry loops' last key differs from key 0 by 0.06-0.34 rad (bones not identified). The
    exe plays loops over n − 1 intervals (`0x010DA030`) as ours does, so it shows the same pop at the wrap: data,
    1:1, left as is.
  - *Fixed (fidelity, not the hitch):* (1) a moving graph's stand rung played the walk level at its slowed rate;
    it now plays the kit's stand level at rate 1 (`0x0133C608` / `0x0133D030`, `0x00773086`). (2) `loop_rate`
    clamped to 0.25..4, which the exe does not (`0x00772E22`); removed (it bit on 6 of the ~28k probe frames).
    (3) An action loop shown over the gait advanced at the gait rate; it now advances
    at rate 1 (kind 0). (4) One helper, `loop_advance`, replaces the copies of the kind-2 rule
    (`gait_scale`, `gait_clip_rate`). Tests `view::a_moving_graphs_stand_rung_plays_the_stand_and_levels_are_unclamped`,
    `view::only_moving_gait_loops_take_the_playback_scale`, `view::the_bearers_gait_clip_plays_by_its_read_root_speed`.
    Before / after: the movement is untouched, so unit 2's speed steps and the level swaps they cause stay as
    measured above; the per-man fix is in §1.10, "Each man's timed move".

### 1.10 Locomotion: speed changes and the locomotion graph (CONFIRMED; gait-blend2, 2026-10-09)
User report after §1.9: infantry smoother, **cavalry still abrupt**. Cause found: ours changed a unit's speed in one tick
(a heavy horse 2.6 -> 10 m/s) and picked the closest level, so a horse went from its walk straight to the gallop; the
exe gathers speed at the entity's acceleration and climbs a ladder of gaits through transition clips.
- **Speed change (`0x00819770`, the locomotion step, once per 0.1 s tick).** Current speed = the speed measured last tick
  (`+0x194`, from the move, `0x007F0260`) clamped to 0..max (`+0x10C`); the wanted speed is `+0x148` (set by the
  locomotive's virtual `+0x84`) times the speed multiplier `+0x1A4` (fatigue and ground column from `0x006543D0`,
  then the slope factor). Rising, the speed changes by at most `+0x158` x `+0x1A4` x 0.1; falling, by at most `+0x100`
  x 0.1. The step is that speed x 0.1 along the heading (`+0xD4`, integrated by `0x007F08F0`, which also adds the
  collision pushes `+0x70` / `+0x74` of `0x0081A790`). `+0x158` / `+0x100` come from the battle entity record
  `+0x24` / `+0x28` (soldier set-up `0x0061CB90` -> `0x007E4FB0`; the decel through the sub-object `0x007E5130`), which
  the record builder `0x00E52F40` fills from `battle_entities` **column 5 (acceleration) and 6 (deceleration)**
  (columns 3..11 in order at `+0x1C..+0x3C`; `+0x18` is an enum of column 2, `+0x4C` column 12, the radius: this makes
  §44 of BATTLE_FIDELITY.md's garrison radius CONFIRMED, while its "+0x18 is the class" is column 2, the skeleton
  type). Data: infantry 2.4 / 5, light infantry 3 / 6, heavy horse 2.5 / 6, medium 3 / 8, light 3.5 / 10 m/s².
  Code: `ntw_sim::battle::model::step_speed`, `LandUnit::acceleration` / `deceleration` / `speed` (set from the
  entity in `battle::setup`); a unit built without entity data changes speed at once. PROVISIONAL (ours): a unit
  stops dead when its move ends (how the wanted speed falls at the destination is the untraced `+0x84`); the exe
  clamps to `+0x10C` (ours has no max apart from the run speed). Logged release run (`--battle --ai off`, side 0 ordered
  to run at 5 s, to walk at 11 s, temporary prints): heavy horse 2.87 -> 4.6 (0.6 s) -> 7.95 (1.8 s) -> 9.8 m/s
  (2.5 s); infantry 1.6 -> 3.6 m/s in about 0.6 s; slowing horse 6.3 -> 1.8 m/s in about 0.6 s.
- **The locomotion graph (static, built by CRT initialisers `0x00413D80..0x00415200`).** One shared graph of 25 nodes
  (list `0x0150D010`; node constructor `0x00721060`: index at `+4`, which is also its priority, edges at `+0xC..+0x14`;
  edge constructor `0x007126B0`: from, to, predicate vtable). Node numbers: 0 stand; loops 2, 5, 9, 13, 17; 1 stand ->
  loop 2, 4 / 8 / 12 / 16 up into loops 5 / 9 / 13 / 17, 7 / 11 / 15 / 19 down into loops 2 / 5 / 9 / 13, 3 / 6 / 10 /
  14 / 18 loop -> stop; 20..24 steps and turns. A graph instance (`0x00717930`) pairs the node list with a per-entity
  **slot table** (node -> slot, 8-byte entries): horse `0x0133D738` (STAND, STAND_TO_WALK, WALK_1, WALK_TO_TROT, TROT,
  TROT_TO_CANTER, CANTER, CANTER_TO_GALLOP, GALLOP; no stop or step-down slots), men's walk / run / walk-trained /
  run-trained `0x0133C608` / `0x0133C6D0` / `0x0133C798` / `0x0133C860` (`WALK_1..5` ... `RUN_TRAINED_1..5` on the
  loops, no transitions), artillery horse teams `0x0133D800` (WALK_1, RUN_1, WALK_TO_RUN, RUN_TO_WALK, stops). The
  display picks the graph by its class (`+0x40`): men by the entity state (`0x007A7190`: walk, run and trained
  variants), animals one graph (`0x007A72F0` -> `0x00794A70` for horses), riders stand / step only (`0x007A7120`).
- **Edges (predicate vtables `0x0133EFC8..0x0133EFE8`).** Up (`0x007CEAA0`): into a transition node only if the slot
  table has a clip for it (else the direct loop edge applies); from the stand when the speed is above 0.01 m/s; from a
  loop when the speed is above the **midpoint** of its loop's and the next loop's root speeds (anim `+0x60`). Down
  (`0x007CE840`): to the lower loop when the speed is at most the lower loop's root speed **+ a quarter** of the gap;
  to the stand at 0.01 m/s (or through a stop clip that can finish within its length). Done (`0x00703100`): a
  transition node moves to its loop once the clip shown is that loop's. Both up and down also need the heading change
  under `DAT_0150D054` and a context value above 0.01 (`+0x40` of the display input, not traced); turning figures take
  the turn nodes (not modelled). With no node (a new graph) the display keeps the highest-numbered node whose entry
  test passes: a loop whose clip is no faster than the speed (`0x00703120`); transition and stop nodes never pass. The
  graph is walked until no edge fires. Helper tables: `0x007CEC60` (a transition's lower loop), `0x007CECD0` (its
  upper loop), `0x00797F40` (is a transition), `0x007947E0` (node has a clip).
- **Display input.** The display reads the entity through two tick snapshots lerped by the elapsed fraction of the
  tick (`0x00752A20` position / heading, `0x00752FF0` speeds and orientation): the speed the edges test is the
  snapshot's `+0x10`. That the exe draws men between ticks this way is a lead for `DrawnPose`'s UNKNOWN (BATTLE_FIDELITY
  §59): not traced further here. Ours (gait-blend3, 2026-10-11): `GroundSpeed::at` lerps the unit's ground speed
  between the last two ticks by `Time<Fixed>::overstep_fraction`, and the clip rate and level tests read that, so a
  speed change no longer steps the playback rate once a tick (the second half of the run -> walk hiccup).
- **Ours.** `unit_animation::climb` / `enter` / `transition_rate` / `MOUNT_LADDER`; `soldiers::LadderRung` (each
  mounted kit's rungs from the horse table, rider on `RIDER_<slot>`); `view::on_ladder` (mounted: one graph, the
  transition starts and ends as §1.9) and `view::on_levels` (on foot: the walk or run graph of the gait, entered on
  its fastest level no faster than the man). `GroundSpeed::gait`: stand when still, run when the move runs (the run
  option or a rout), else walk; a charging man uses the charge graph (`CHARGE`). CONFIRMED: `0x006543D0` sets the
  state `+0x1B8` from the stance `+0x1D8` and the move kind `+0x220` (0 walk, 1 or 3 run, 2 charge): 8, 0xB, 0xD; the
  trained stance 9, 0xC; `0x007A7190` maps 1 and 9 to WALK_TRAINED, 0xB to RUN, 0xC to RUN_TRAINED, 0xD to the charge
  graph (`0x0133D030`, node 2 `CHARGE`), others to WALK; the move kind is the order's run option (`0x0051A9A0`).
  Logged release run (as above): horses STAND -> STAND_TO_WALK -> WALK -> WALK_TO_TROT -> TROT at a walk order (the
  heavy horse walks at 2.6 m/s, past the 2.50 m/s walk / trot midpoint of its clips, 1.47 and 3.53 m/s, so a walking
  heavy horse trots by the traced rule; it drops back to the walk below 1.99 m/s on slopes), then on the run order
  TROT_TO_CANTER at 4.5 m/s, CANTER, CANTER_TO_GALLOP at 7.95, GALLOP at 9.8, and back to CANTER, TROT, WALK on the
  walk order; infantry WALK_TRAINED_2 / _3 at a walk, RUN_TRAINED_1 then _2 at a run, and on the walk order entering
  the walk graph on its fastest level (WALK_TRAINED_5 at 3.16 m/s) and stepping down one level a tick. Screenshot of
  cavalry mid-transition (`NAPOLEON_BATTLE_ZOOM=30,4`, battle time 2 s): poses whole, riders seated. To check in game:
  `cargo run -p napoleon --release -- --battle --no-intro --ai off`, order the cavalry to walk, then run, then walk,
  side by side with the original. Tests: `unit_animation` (ladder thresholds, entry, transition rate), `battle::view`
  (ladder starts, men's graphs), `ntw_sim::battle::model` (acceleration / deceleration).
- **Round 2 (gait-blend2): the coordinator's points after the first round.**
  1. *Walk-ordered horses must stay in walk* (sitting 2026-10-09: on the walk order horses went stand_to_walk <-> walk,
     walk_to_trot only after the run order), but ours walks a heavy horse at 2.6 m/s, past the 2.50 m/s walk / trot
     midpoint. The clip speeds are not the difference: the anim loader `0x010CB060` sets anim `+0x60` = root
     displacement first -> last frame / duration `+0x4C` (as `Anim::root_speed`; with anim flag `0x10000` and a clip
     over 1 s, the displacement over its first second), so WALK_1 1.47 / TROT 3.53 m/s hold. The display's speed is the
     entity's interpolated forward speed `+0x14C` (the snapshot writer `0x0079FF50` copies `+0x14C`, `+0x150`, the
     ground speed and the wanted speed `+0x148` into the display input's tick ring, read back by `0x00752FF0`; the
     edges' second context value is that wanted speed). A unit's move speed is its formation's first member's record
     walk `+0x1C` / run `+0x20` (`0x005650E0`, `this` from `0x004B4A50`, members at formation `+0x18` / `+0x1C`), times
     the group factors `+0x510`->`+0xAF0` x `+0x2780` for a group's lead unit (`0x0055C280`). The soldier set-up
     `0x0051B7D0` makes the riders with the unit's `+0x60` record and, for a unit with mounts, the mounts with the
     `+0x8C` record (`0x0059EED0`, 0x69C bytes) and seats `+0x60`-record riders on them (`0x00655B20`). Settled by the
     2026-10-10 sitting below (a cavalry unit's speed is its horses' record; a walking horse moves at ~0.8 of its order
     speed) and the static trace after it.
  2. **Arrival deceleration: done (CONFIRMED).** `+0x84` is only a turn-rate override (`0x00659220`). The wanted speed
     `+0x148` is set by the locomotive's state machine (`+0xF4`, run by `0x010FC370` from the step; states are
     static singletons `0x01454168..0x01454194`): the move state (`0x00806D40`) wants the order speed `+0x10C`
     (times the cosine of the heading error when it must turn), and its check `0x007DE1F0` -> `0x00807410` leaves for
     the stop state once the distance left `+0x144` is at most max(0.5, 0.5 / decel x speed^2) m; the stop state
     (`0x00807080`) wants 0, so the step brakes at the deceleration. Ours: `ntw_sim` `arrival_braking`, braking
     latched by a one-tick look-ahead (the exe stays in its stop state); the unit ends up to 0.5 m short, as the
     soldier does. Test `a_unit_brakes_to_its_destination`.
  3. **Rider legs: done (CONFIRMED).** A rider display with a mount display (`+0xCC` of its input, `param_4[0x33]`)
     skips its own graph: its slot is the rider table's (`+0xE4` -> `+0x60`, an array by mount slot, `0x0079BB10`)
     entry for the mount display's current slot, RIDER_STAND (427) when the table has none, and for the rider
     kinds it copies the mount display's clip time (`+0x14` -> `+0xD8`) every frame. Ours already plays
     `RIDER_<mount slot>` (`unit_animation::rider_slots`) at the shared clip time; fixed now: a mounted action loop
     without a mount clip (the horse on its gait) no longer leaves the rider on his idle clip, he plays the gait
     level's rider clip as the map gives.
  4. **The two PROVISIONALs: done (CONFIRMED)**, above: the men's graph by state (and the charge graph), and the
     ladder start on the loop's wrap.
  5. **Infantry clips at the four stores.** Statically a man's walk / run levels are loop nodes of his graph, changed
     through store C `0x00773FF7` (a loop -> loop change; A, B and D belong to transition clips). Each soldier view
     holds two display states (`+0x80` + `+0x164` x 0x4C, double-buffered) and `0x007911E0` / `0x00789BD0` update
     them, so the stores are the same for men. CONFIRMED by the 2026-10-10 sitting below (point 3).
- **Debugger sitting 2026-10-10 (original, custom desert battle: general, a light cavalry unit, line and light
  infantry; module base `0x00E90000`).** Settles points 1 and 5 above:
  1. *A cavalry unit's speed comes from the horse record (CONFIRMED).* `0x005650E0` on a 24-man cavalry unit: first
     member's record `+0x18` = 1 (horse), walk `+0x1C` 2.7, run `+0x20` 11.0 m/s; that unit always queried it with the run
     flag, and its horses' `+0x10C` read 11.0 standing and 11.0..15.95 moving (the catch-up factor), wanted 9..12.8 m/s.
     A line infantry unit (80 men): record `+0x18` 0, walk 1.55, run 4.05 m/s. A walk order to the general or the light
     cavalry did not call `0x005650E0` in the logged window.
  2. *A walking horse moves well under its record walk speed (CONFIRMED).* A walk-ordered light cavalry horse (record
     walk 2.8, run 12.0, `+0x158` acceleration 3.5, `+0x100` deceleration 10.0), three samples over ~6 s on flat sand:
     `+0x10C` 2.811..2.856, wanted `+0x148` = speed `+0x194` = forward speed `+0x14C` **2.09, 2.27, 2.28 m/s**,
     multiplier `+0x1A4` 1.0. So the wanted speed was ~75-81 % of the order speed while walking straight: below the
     2.50 m/s walk / trot midpoint, which is why the original's walk-ordered horses stay in walk and ours (moving at the
     full record walk) trot. Traced statically below ("Static trace 2026-10-10"): the read can't tell which of two
     factors did it, so a second sitting read is written there.
  3. *Men's walk / run level changes go through store C (CONFIRMED).* `0x00773FF7` with clip kind `+0x1C` = 2 and the man
     display vtable `0x01341E20` logged `mus_irregular_locomotion/mus_walk_100 / _127 / _173 (+ _alt*)`,
     `mus_jog_313`, `mus_run_407` and the flag bearer's `fla_walk_* / fla_jog_* / fla_run_407` during the infantry walk
     -> run -> walk orders. The earlier sitting's silence was the filter, not the path.
- **Static trace 2026-10-10 (gait-blend2 round 3): every factor between the order speed and the wanted speed.**
  The locomotive state machine is the object at entity `+0xF4` (vtable, `+4` current state, `+8` iteration cap;
  `0x010FC370` runs `0x010FBBD0` until the state stops changing: state vtable `[0]` check -> next state or null,
  `[1]` enter, `[2]` update, `[3]` exit, `[4]` id). State fields are entity offsets minus `0xF4` (state `+0x18` =
  `+0x10C`, `+0x30` = `+0x124`, `+0x54` = `+0x148`). The twelve singletons `0x01454168..0x01454194` have vtables
  `0x01346994 + 0x14 n`; only two updates want a speed: the move state (`0x00806D40`) and state 7 (`0x00806F70`,
  wants `+0x10C` with no cosine, 0 within 0.01 m); the others (`0x00806C20`, `0x008069C0`, `0x008070A0`, the stop
  state `0x00807080`) want 0 and only turn. So, per 0.1 s tick, CONFIRMED:
  1. *Order speed `+0x10C`* is set by `SetLocomotiveMoveTarget` `0x0080B6E0` (target `+0x160..+0x170`, speed
     `+0x10C`, flags `+0x110`), called each tick by the soldier's move-order update `0x00659600` (first thing in
     `0x006543D0`). The speed is the order block's `+0x40C`, except in move states 4 / 5 of `+0x390` with byte
     `+0x414` = 1 (a timed move): then `0x0063AF40`(v = `+0x40C`, vmax = record run `+0x20` x 1.45 (`0x0063F040`),
     d = distance to the order's destination `+0x3F4`, t = `+0x410` − elapsed `+0x43C` (+0.1 a tick), keep-v flag =
     `+0x438` bit 0 clear) = v if t ≤ 0, else min(v + (d − v t) / t, vmax), with d − v t floored at 0 under the flag.
     With bit 0 set (`+0x3A4` or `+0x408` clear) it is recomputed every tick, so it is plainly d / t: a soldier that
     falls behind its schedule gets a slowly rising order speed. **That is the 1.004..1.02 of the sitting** (2.811
     rising to 2.856 over ~6 s fits a horse moving at ~0.8 of d / t with ~80 s left). Not traced: the writer of the
     order block `+0x3F4..+0x420` (a struct copy; `+0x410` presumably the move's planned duration).
  2. *Move state `0x00806D40`* wants `+0x10C`, times max(0, cos(heading error `+0x124`)) unless `+0x134` > 0 (the
     error is the bearing to the step's target minus the heading `+0x64`, set by the step at `0x00819974`; a turn
     snaps when the error is ≤ 1° (`0x0081BEF0`, `0x0145438C` = 0.01745 rad), else turns by at most the turn rate
     `+0x104` (from `+0x15C`, 2π in states 0x19 / 0x1A, `0x00659220`) x 0.1). None of this happens in strafe mode
     (`0x00814AA0`: tweak `LOCOMOTIVE_FORCE_STRAFE` at `0x0150FB48`, flag `+0x110 & 0x40`, or byte `+0x116`): then the
     wanted speed is `+0x10C` and the entity moves straight at its target.
  3. *The step `0x00819770`* multiplies the state's wanted speed by `+0x1A4` and stores the product back in `+0x148`
     (`0x00819D57`). `+0x1A4` is reset to 1.0 by `0x007F0260` after each move, multiplied by the ground column
     (and the fatigue factor on runs only, `+0x220` > 0) in `0x006543D0` **before** the step, and by the slope factor
     (gradient `+0x1A0` from `0x005574A0` at the step ahead vs `+0x4C`, the height `0x007F08F0` sets from the same
     ground) **inside** the step at `0x00819D17`. `+0x1A0` is zeroed on entry. The step target is `+0x160` through
     `0x0080E9D0` (a moving frame when `+0x170` is set, `0x007F4950`); the path corner pick `0x0080BF70` puts the next
     visible corner there, so on open ground it is the destination. Nothing else writes `+0x148` on this path
     (`0x0080B080`, the networked / puppet move of `0x0067A050`, sets it to the tick's displacement and skips the step).
  So the sitting's 0.75..0.81 is cos(heading error) or the slope factor (or a ground column the `+0x1A4` read could
  not show, if it was not sand): **the entry read can't separate them**, because at `0x00819770` entry `+0x148` is
  last tick's product, `+0x1A4` holds ground x fatigue without the slope, and `+0x1A0` is not yet set. Sand's mounted
  column is 0.9 (`unit_movement_modifiers`), so the `+0x1A4` of 1.0 read also says the horse's cell was not sand (or
  the mount did not pass the mounted class test `0x0055ABF0`). Ours has no per-soldier heading and no timed move:
  `placeholder_movement` moves the unit at its record walk x ground x slope, which matches the exe only if the 0.8
  was the slope. **Not ported** (no guess): resume with the read below.
  **Debugger read that settles it** (same set-up as the 2026-10-10 sitting, one walk order to a cavalry unit across
  open, level ground; dynamic = static − 0x400000 + module base; `ghidra_trace_sync_disable()` first): breakpoint at
  `0x00819D37` (in the step, after the multiplier is final and before it multiplies the wanted speed), condition
  `poi(poi(@ebx+0x1E8)+0x18) == 1` (a horse), ~20 hits, logging `df @ebx+0x10C L1` (order speed), `df @ebx+0x148 L1`
  (the state's wanted, before the multiplier), `df @ebx+0x1A4 L1` (multiplier incl. slope), `df @ebx+0x1A0 L1`
  (gradient), `dw @ebx+0x124 L1` (heading error, 65536 = 2π), `df @ebx+0x134 L1`, `dd @ebx+0x19C L1` (ground index),
  `dd @ebx+0xF8 L1` (current state: the move state singleton holds vtable `0x013469A8`), `dd @ebx+0x110 L1` (flags),
  and the timed-move block `df @ebx+0x40C L2`, `db @ebx+0x414 L1`, `df @ebx+0x43C L1`. Reading: `+0x148` ≈ 0.8 x
  `+0x10C` with `+0x124` ≈ ±6700 means the heading cosine (then the next question is why the heading lags: log
  `+0x64`, `+0x15C` and the target `+0x11C` / `+0x120` too); `+0x1A4` ≈ 0.8 with `+0x1A0` ≈ 0.08 means the slope
  (then ours already has the rule, and the walk / trot difference was the map); both ≈ 1 with a slow move means the
  measurement, not the rule (re-read `+0x194` against `+0x148` at `0x00819E3B`).
  **Result of that read (user sitting 2026-10-10, CONFIRMED): the ground column, not the heading.** Write watchpoint
  on one walk-ordered light cavalry horse's `+0x148` over 20 ticks. Per tick: the step zeroes it (`0x008199B7`), the
  move state writes the wanted = order speed `+0x10C` (`0x00806D57`, `0x00806EE8`; heading error `+0x124` 0 or -1, so
  the cosine is 1), then the step stores the product (`0x00819D5F`). At the product `+0x1A4` = **0.373** (ground 0.40 x
  slope 0.93, gradient `+0x1A0` 0.024..0.045) on ground index `+0x19C` = 0xE, so the wanted was 2.68..3.07 x 0.37 =
  1.0..1.1 m/s; before the step `+0x1A4` read 0.40 (ground column x fatigue, no slope). The order speed rose ~0.035 a
  tick (the timed move's d / t catch-up) and fell back to 2.69 when the move was re-planned. Polled reads on ground 3
  showed the wanted at the full order speed (2.81..2.84), on ground 6 ~0.8. So the walk is slowed by the mounted
  column of `unit_movement_modifiers` per ground index (and the slope). Ours already has ground x slope, so the
  difference is the ground index a cell gets and the column value per index. Next: map ground indices 0xE and 6 to
  their `unit_movement_modifiers` rows, compare with our ground lookup for the same cells, and port the timed-move
  order speed (`0x0063AF40`).
  **Ground index → row → cell, traced (gait-blend2, 2026-10-10, CONFIRMED unless marked).**
  - *Index → row.* The name table `0x01452520` holds 25 names (field_ploughed .. none, our `GROUND_TYPE_NAMES`
    order), then end_marker and invalid_ground_type. The Ground State Grid build `0x0061E430`
    (BuildBattleGroundStateGrid) looks each name up in `unit_movement_modifiers` in table order, so the ground index
    is the name's position; `0x006543D0` uses that record for an index < 25 and 1.0 otherwise. 0xE =
    vegetation_dense_forest, 6 = road.
  - *Pixel → index: by palette colour, not palette index.* `LoadBattleMapGroundTypeImage` `0x00EC8560` loads the
    ground TGA (`0x010D2170`), then `0x00E8B550` converts each pixel: `0x00EE5E00` takes its palette colour
    (`0x010DBDD0`) and compares the 4 bytes exactly (`0x00EC1FA0`) with the 26-entry colour table `0x0145BF60`
    (B,G,R,A=0xFF; entry 25 a duplicate black that never matches); the first match is the index, none gives 0x1A
    (factor 1.0). Ours read the raw palette index. A probe of the 57 shipped presets with a TGA (all type 1,
    bottom-up, 24-bit palette from entry 0): 38 have the 25-entry identity palette, 16 a 24-entry palette without
    field_forest (every index ≥ 2 is one ground lower than its colour: caribbean, empty_flat, hb_naval, hb_nile,
    hb_trafalgar, indian/ottoman artillery_fort and great_fortress, the seven nap_mp_*_[sa] maps,
    nap_mp_great_plains), and 3 a 256-entry palette (nap_mp_grassy_flatlands, welly_map_c, western_artillery_fort:
    85 is rock, 255 road). No shipped colour is unmatched. So ours gave the wrong ground (and movement column) on 19
    maps; now fixed: `GroundTypeMap::from_palette_indices` converts through `GROUND_TYPE_COLOURS` /
    `ground_index_of_colour` (`ntw_formats::battle_terrain`); an index past the palette end gives 26 (what
    `0x010DBDD0` returns there is UNKNOWN; no shipped map has one).
  - *Cell.* The grid is a fixed 512 x 512, cell = world size / 512 (ours: the TGA size, 512 on every shipped map).
    `0x0081AC80` (UpdateSoldierGroundTypeIndex) sets `+0x19C`: a standing-on object (`+0x10`) gives its `+0x5C`;
    else `+0x18` gives 0x15 (wood); else the cell at 256 + floor(pos x 512 / size) per axis, each clamped ≤ 511 as
    unsigned. Row 0 is the file's first row = the picture bottom (bottom-up files), which matches ours. Not ported:
    the object and `+0x18` paths (they do feed `+0x19C` and so the multiplier, but our terrain has no standing-on
    objects or woods yet; BACKLOG line), and the cell exactly on a border (whether the row axis is z or −z decides
    the tie; UNKNOWN). *The unsigned clamp, decided: not an ORIGINAL BUG.* A negative index wraps and clamps to 511
    (the far edge) where ours clamps to 0; every point on the map has both indices in 0..511 and gets the same cell
    from both, so only an off-map point differs and nothing in the exe's data or intent is contradicted on the map.
    Comment at `GroundTypeGrid::at`.
  - *Timed move `0x0063AF40` (ComputeTimedMoveOrderSpeed), ported as `ntw_sim::battle::model::timed_move_speed`.*
    With time left t = planned `+0x410` − elapsed `+0x43C` (`+0x43C` += 0.1 a tick) > 0: speed v + (d − v·t) / t,
    the shortfall floored at 0 when the order keeps its speed (`+0x438` bit 0 clear; the bit is set when `+0x3A4` or
    `+0x408` is clear), capped at the run speed (record `+0x1E8` → `+0x20`) x 1.45 (`0x0063F040`); t ≤ 0 gives v.
    d is the ground distance from the soldier (`+0x48`, `+0x50`) to the destination `+0x3F4` (`0x0080E9D0`). Called
    by `0x00659600` (UpdateSoldierMoveOrderTarget) in move states 4/5 of `+0x390` with the timed byte `+0x414` = 1:
    with keep-speed only on the first moving tick (`+0x440`), else every tick. The order: `0x00659520`
    (AssignSoldierMoveOrder) copies the destination to `+0x3F4..+0x404` and the speed block to `+0x408..+0x420`
    (`+0x40C` speed, `+0x410` planned time, `+0x414` timed), sets state 1 and zeroes `+0x43C` / `+0x440`. The timed
    path is `0x00816900` (vtable `0x01348B44` slot 1; speed / time from the order object's `+0x3C` / `+0x40`, set
    from its descriptor words 8 / 9 by `0x007E2C80`), reached through `0x00650DC0` (soldier `+0x228`) from
    `0x005DD450`, a virtual loop over the unit's soldiers (vtable `0x0132BB8C`).
  - *Where the speed and planned time come from (traced statically 2026-10-10, corrected gait-jitter 2026-10-11).*
    Two order sources reach `0x006533E0` (InstallSoldierTimedMoveOrder, the descriptor → `0x007E2C80` →
    `0x00659520` step). **(a) The unit move order drives a unit's walk and run (CONFIRMED).** Its per-tick update
    (`0x00581AE0` UpdateUnitMoveOrderReissue, a vtable method in the table at `0x01325474`) re-issues every soldier's order when the battle
    tick count × 0.1f (`0x0131A7B0`) is strictly above the last issue time (order `+0x2C`) + 1.0f (`0x01318048`):
    `COMISS`/`JBE` at `0x00581B73`/`0x00581B76`, store and call `0x00584E10` at `0x00581B81`; the tick count is the u32
    at order `+0x14` → `+8` → `+0x58`. In f32, 10 ticks × 0.1 almost always rounds to exactly 1.0 (not above it), so
    the re-issue comes every **11 ticks** (an f32 run from 0 over 200 000 ticks: 18 178 periods of 11, 4 of 10, at
    ticks 21, 42, 1284, 20 489). **The counter is traced (gait-trace 2026-10-12, CONFIRMED):** order `+0x14` → `+8` is the Battle Model (ctor `0x00506C00`, "Battle Model" profiling strings), whose `+0x58` the ctor zeroes at `0x00506C85`; `0x0057DF40` (StepBattleModelIfTickDue) runs one model tick and then increments it at `0x0057E0BC`. Ours ports the rule itself (`timed_move_issue_time` / `timed_move_reissue_due` in `ntw_sim::battle::model`, order `issued` = `+0x2C`), so from tick 0 the 10-tick periods fall at issue ticks 11, 32, 1274 and 20 479. Left PROVISIONAL on `Battle::tick`: the model also ticks during deployment (AI deployment phase `0x00583820` runs from tick 8), so the exe's count at the conflict's start is the deployment's ticks, while ours starts at 0 at the conflict; the f32 schedule (and any rule keyed on the count) depends on that base until our model counts deployment ticks too. `0x00584E10` (IssueUnitMoveOrderToSoldiers) fills each soldier's descriptor
    (`+0xA4`): words 0–4 the soldier's own slot point, word 5 the heading, word 7 order `+0x24`, word 8 v = the gait
    record's speed (`0x00565030` GetUnitGaitSpeed, no ground or slope factor), word 9 t from `0x0054C650`
    (ComputeUnitMovePlanDistanceAndTime), word 10 low byte = `0x007FF1C0` && order `+0x28` (clear for a walk), and
    installs it at `0x00585602`. `0x0054C650`: D = n·k + 2v, t = D / v, n = (unit `+0x510`)`+0x27C`, k = (unit
    `+0x510`)`+0x13C` (read at `0x0054C691`); n = 0 gives t = 2.0 (`0x40000000`). **n and k named statically (gait-trace
    2026-10-13, CONFIRMED):** unit `+0x510` is the unit body (0xE38 bytes, ctor `0x0051B7D0`, allocated by `0x00531320`
    from `0x00505B00` ← `0x005055E0` ← `0x00544A70` ← `0x00515340`; `0x00565030` GetUnitGaitSpeed reads its `+0xAF0`
    speed multiplier and `+0x384` record through the same pointer). n = the body's living-soldier count: `0x005307E0`
    (called once from the body ctor) creates one soldier object per `+0x48` entry and pushes it into the `+0x278` /
    `+0x27C` / `+0x280` array (count++ at `0x00530AE1`); `0x00574E90` swap-removes one (count-- at `0x005751DF`).
    k = `unit_stats_land` column 45 `spacing_rank_close` (the rank spacing of the close order, m): the body ctor
    copies the holder's LAND_UNIT_RECORD into `body+0x20` (`0x0051B841` → `0x00564790` → `0x00514FB0`, member-wise;
    record `+0x11C` copied at `0x00515169`), so `body+0x13C` = record `+0x11C`, and the record builder `0x00E8CFF0`
    fills `+0x114..+0x130` from the `unit_stats_land` row `+0x1C0..+0x1DC` (`+0x11C` ← row `+0x1C8`; the repo schema
    `ntw_data::schemas` puts #43 default_ranks at 0x1C0, #44 spacing_file_close 0x1C4, #45 spacing_rank_close 0x1C8;
    `+0x134` ← row `+0x1E0` is_artillery confirms the alignment). Earlier class notes here were wrong: `0x00515340`
    (0x2DF8 bytes, "Manager", battle model `+0xB0`) is a battle-level manager, `0x00518230` copies a different layout
    (the recruitment-panel holder), and `0x005181E0` / `0x00518DC0` build a 0x514-byte class, none of them the unit.
    **Open contradiction:** with n living men and k the rank spacing, a full unit (160 × ~1.5 m) plans t = n·k/v + 2 of
    tens to hundreds of seconds, while the 2026-10-10 sitting's slope (order speed rising ~0.035 a tick from 2.69 under
    `0x00659600`'s d / (t − elapsed)) needs t ≈ 5 s (n·k ≈ 3v). Not resolvable statically (the array `+0x27C` may be
    counted differently at order time, or `0x00659600` may not use t as read): `TIMED_MOVE_PLANNED_SECONDS` stays
    INFERRED 5 s until the debugger read at `0x0054C691` (below) logs n, k and t, and `0x00659600`'s use of word 9 is
    re-read against the logged values.
    **(b) `0x006DC700` (IssueArtilleryCrewFollowMove) is the artillery crew following its gun, not a walk order (CONFIRMED).** Its caller chain is
    `0x006D7720` → `0x006E0410` → `0x006D9A40` → `0x006DC700`, and `0x006D7720` is slot 105 of the six artillery-piece
    vtables (ctor `0x0068F610` picks the grape / chain / shot_rifled / improved_grape variant). The follow record it
    reads is kept by `0x006F48F0` (UpdateArtilleryCrewFollowRecord, called per follow entry by `0x006FEE40` / `0x006F47C0` / `0x006A7E60`): speed
    `+0x38` = the slot's move since the last update (3D distance `0x007FE690`) × 10 when above 0.01, target `+0x24` =
    slot + 5 × speed along the heading (tables `0x0176CFF8` / `0x0176CFFC`, negated in state 0x1D), counter `+0x3C`
    += 1 (`0x006F4FA5`); every 10th it sends speed `+0x38`, t = 5.0 (`0x40A00000`) and word 10 = 0. The 2026-10-10
    note that the walk follows (b) (from the t = 5 arithmetic alone) was wrong.
    **Only these two sources exist (static, sitting-sheet prep 2026-10-15, CONFIRMED by xrefs):** `0x006533E0` has
    exactly two callers, `0x00585602` (in `0x00584E10`, source (a)) and `0x006DC7AF` (in `0x006DC700`, source (b)), so
    the planned debugger sitting at `0x006533E0` is not needed. `0x006DC700` itself has a second, uncatalogued
    caller: `0x006D9A22` in `0x006D9A10` (ECX = this + 0x20, argument from `0x006C2F60([[this+0x14]+4])`, the same
    shape as the `0x006D9A40` call at `0x006D9BA5`), reached through `0x006CB6F0` (and the thunk right after it) from two
    adapter stubs with no Ghidra function, `0x0067BCB0` and `0x0067B2F0` (each subtracts 0x98 from its argument, a
    base-class adjust), whose addresses sit in a listener table at `0x01336A18`, `0x01336CBC`, `0x01336CE0` and
    `0x01336CE4`. The owning class is UNKNOWN; it still only issues crew-follow moves (the body of `0x006DC700`), so
    the walk's source is unchanged.
    Ghidra (gait-jitter 2026-10-11): the record is typed `ArtilleryCrewFollowRecord` (0x68 bytes known; both functions
    are its members), the direction table is `g_aHeadingDirectionTable` (65536 x {x, y}, BSS, component order INFERRED),
    `0x01325C60` = `g_flHeadingToRadians` (pi / 32768), `0x01508EF0` = the "cannon_crew_validity" debug log channel.
    Every named local has its storage type; V5 completeness `0x006F48F0` 52.9, `0x006DC700` 71.8. The rest is
    accepted: register-only SSA temporaries, raw offsets into the engine and soldier (no structs for them yet), and the
    log channel's untyped object.
  - *The surge is the original's (CONFIRMED, gait-jitter 2026-10-11).* Statically: the order speed is the unslowed
    gait speed (word 8); `0x00659600` recomputes d / t every tick through `0x0063AF40` (word 10 clear sets `+0x438`
    bit 0, so no keep-speed floor) and neither holds nor smooths it; the ground column × slope multiplier `+0x1A4`
    applies only after it, to the wanted speed in the step `0x00819770` (`0x00819D5F`), whose limits (acceleration
    3.5 × multiplier, deceleration 10 m/s² for that horse) pass a +0.035 rise and the drop back at once. On slowed
    ground the soldier falls behind its plan, d / t rises, and the re-issue at `0x00581B81` restarts it at v: a
    sawtooth. At runtime, the 2026-10-10 sitting (original under the debugger: a write watchpoint on a walk-ordered
    light cavalry horse's `+0x148` for 20 ticks, ground index 0xE, `+0x1A4` = 0.373 = column 0.40 × slope 0.93,
    heading error 0 / −1) logged the order speed `+0x10C` starting at 2.69, rising ~0.035 a tick to 3.07, and
    falling back to 2.69 at the re-plan, with the wanted speed 1.0..1.1 m/s at `0x00819D5F`. So the speed-up /
    slow-down once a second on slowed ground is 1:1 and kept.
  - *Wired, `ntw_sim::battle::model::TimedMove`.* Each moving unit carries a timed order: planned at its gait speed
    with d = speed × 5 s (`TIMED_MOVE_PLANNED_SECONDS`, INFERRED, above), re-planned on the exe's f32 rule
    (`timed_move_reissue_due`, CONFIRMED `0x00581B73`: mostly every 11 ticks, 10 at the f32 edges) or when the speed changes, cleared
    on arrival or stop and on every order install, also one re-issued to the same point (`LandUnit::set_destination`,
    the one path every order source writes the destination by; `0x00659520` zeroes `+0x43C` on each install). Each
    tick the wanted speed is `timed_move_speed(v, run × 1.45, d, 5 − elapsed, keep = false)` × the ground/slope
    multiplier; d shrinks by the distance covered and elapsed grows 0.1 after the speed is computed, as
    `0x00659600`. Test `walking_cavalry_in_dense_forest_walks_on_its_timed_order` (ground 0xE, mounted 0.4: every
    11-tick plan starts at 2.69, rises 0.03–0.05 a tick, peaks below the sitting's 3.07 + 0.05, no re-plan at tick
    10, below the 2.50 m/s walk/trot switch). The unit's order is its reference soldier's; each man's own d is in
    "Each man's timed move" below. `TimedMove` keeps the INFERRED t.
  - *Debugger read left (for `docs/FOR_USER.md` only if the night debugger can't reach it).* Break at `0x0054C691`
    for a walk-ordered horse and infantry and log n = (unit `+0x510`)`+0x27C`, k = `+0x13C` and the returned t
    (the static read above says n = living men, k = rank spacing, which gives t far above the sitting's ≈ 5 s: the
    log settles which is used). Sitting plan: launch the original under Ghidra's debugger (never attach to a running
    game), walk-order one cavalry and one infantry unit; at `0x0054C691` log the unit body pointer, n and k, and on
    return D and t; then watch `0x00659600` read the move descriptor's word 9 (`+0xA4 + 0x24`) to see which t the
    timed move uses.
  - *Gait-trace state (2026-10-13, final).* Done: BACKLOG 223 (tick counter, re-plan rule); 224's static part (n, k
    above, CONFIRMED); 225 and 226 (§1.9 items 4 and 5); the Ghidra batch (`0x00506C00` ConstructBattleModel, V5
    write-up, globals `g_flBattleMapCentreX/Y`, `g_szWindScopeName`; `0x0054C650` plate fixed to n = living men /
    k = `unit_stats_land` #45). Left: the debugger read above (224's `TIMED_MOVE_PLANNED_SECONDS`, INFERRED), and
    `Battle::tick` counting the deployment ticks (PROVISIONAL, its own BACKLOG line); an empty slot's fallback to
    slot 0 (§1.2), not modelled. Also, for ground 6 ~0.8: on a road cell, log the soldier's `+0x19C`, `+0x10` and
    `+0x1A4` at `0x0081AC80`'s return and at `0x00819D5F`, to tell an object ground from the road row value.
  - *Measured surge in ours (gait-jitter, 2026-10-11; env-gated per-tick probe, since removed, `hb_austerlitz`, AI
    off, our 2 cavalry walk → run → walk).* On open flat ground the unit's speed is steady (2.44–2.47 m/s walk,
    ±1%). On slowed ground (multiplier ~0.65) the re-plan gave a 1 s sawtooth: 1.58 → 1.71 m/s (+8%) over 10 ticks,
    then back at once; the gait loop's cadence follows it (`loop_rate`), so the horses read as speeding up and
    slowing down once a second: the original's shape (above), now on its 11-tick period. Terrain also swings it
    (walk 2.44 → 3.0 → 1.6, run up to 6.1, down to 2.7, up to 8.5) through the ground/slope multipliers and the
    6 m/s² braking, as traced in §1.4.
- **Each man's timed move (CONFIRMED mechanism; gait-infantry, 2026-10-12).** How the exe gives each soldier of a
  moving unit his own speed and start, traced from the unit move order:
  - *Cursor.* Every tick the order's path cursor (`+0x34` node, `+0x38` fraction) is set to the reference soldier's
    projection on the path (`0x00531A70` → `0x007F05F0`: a forward search over the segments, never back on the
    same segment).
  - *Issue* (when the order is built, `0x0051A8FD` with argument 1, and every 11 ticks, `0x00581B81` with 0; these
    are `0x00584E10`'s only callers): v = the gait speed (`0x00565030`); (D, t) from `0x0054C650` (D = n·k + 2v,
    t = D / v; D = 2v, t = 2 when n = 0; D = v·t always); a copy of the cursor is carried D along the path
    (`0x007F0790`); the formation layout (engine `+0x628`, vfunc `+0x24`) lays the unit out there, one 0x24-byte
    slot per man, and names the reference soldier (order `+0x30`; `0x00572B00` raises it to (unit
    `+0x510`)`+0x27C` and clamps it to count − 1); then each issued man gets `0x006533E0` with his slot, the
    heading, v and t.
  - *Clamp at the path end.* `0x007F0790` past the last node extrapolates beyond the end unless its flag is set;
    `0x00584E10` sets it (`0x00584E54..0x00584E7B`) only when the order's `+0x20` (from its descriptor's `+8`, the
    destination layout) is 0 and its `+0x28` byte (descriptor `+0x10`) is set. The plain move descriptor
    `0x0051A970` (`+8` = 0, `+0x10` = 1; also built at `0x00565576`, `0x005810FC`) is what `0x0065D740` builds
    for a command without a heading (`+0x40` clear), so it clamps; with a heading it builds `0x0051A910` (`+8` =
    the layout from `0x00649B30`), which extrapolates. `0x007FF1C0` on the carried cursor (last segment,
    fraction ≥ 1) then sets order `+0x3C` (at the end), and descriptor word 10 = `+0x3C` and `+0x28`: the
    soldier's `+0x408`, which makes `0x00659980..0x00659A00` leave `+0x438` bit 0 clear (keep speed: d / t
    computed once on the first moving tick, `+0x440`, floored at v) when his byte `+0x3A4` is set too.
  - *Who is issued.* Argument 1: every man. Argument 0: a man for whom `0x0063E290` is false (his timed move still
    runs), unless he started an action this tick (`+0x224`) of a kind table `0x00811890` maps to true (1, 2, 13,
    14, 17..20), and a finished man only when `0x00658C20` holds (his slot's order `+0x1C` = 1, written by
    `0x008056D0` from his locomotive's result `+0x18` at the move's end) and order `+0x3C` is 0. An issued man
    gets the timed move only when `0x005516D0` on the cursor copy is false and his slot allows it (else
    `0x0057AC10` → soldier vfunc `+0xB8`, or `0x006527B0`).
  - *The stop.* Nothing is issued at the reference soldier's stop. The order's done flag `+0xC` is `0x00531B70`,
    true once `0x0063E290` holds for every soldier; until then the 11-tick re-issues go on for the men still
    walking. The issue point reaches the end at least D − 1.1 v before the stop, so every man's point at the stop
    is already his place at the end: d = what the reference soldier has left + his lag.
  - *No spread.* No random generator is reachable from `0x00584E10` (to depth 5), and the issued men get it in
    the same loop, with no delay. A man's order speed is his own d / (t − e) (`0x0063AF40`, capped at 1.45 × run),
    his speed that times his own multiplier `+0x1A4` (ground under him, slope ahead, fatigue), stepped by
    acceleration × multiplier and by deceleration. So men differ only by where they stand: a man on slower
    ground falls behind, the next issue's d = D + his lag speeds him up, and his speed (and so his gait level)
    changes on his own frames, not the unit's.
  - *Cursor step.* After the reference soldier's install, while `+0x3C` = 0, the cursor is carried v further
    (`0x005856F2`) when `0x00658C20` holds or his distance² is under (0.25·D)². Not modelled (see below).
  - *t.* Still INFERRED ≈ 5 s (`TIMED_MOVE_PLANNED_SECONDS`): the 2026-10-10 sitting fits t ≈ 5.26, so k is
    probably nonzero at runtime; the debugger read above settles it.
  - *Ours* (`ntw_sim::battle::march`, called from `Battle::placeholder_movement`): the unit's own move is the
    reference soldier's (`TimedMove`, its point `issue_point`: D ahead or the path's end, with `at_end`, `first`
    and the kept speed); each man (`LandUnit::march`, `ManMarch`) keeps his lag behind his place along the unit's
    facing, his speed, his d, `finished` and his kept speed. At an issue each man the gate passes (`reissued`)
    gets d = the issue point's distance + his lag. After the unit's move ends the same order goes on, re-issued
    every 11 ticks with D = 0, until every man has finished (reached his place at the end or braked to a stop for
    it). Each tick he samples the ground and slope at his own place (laid out on the unit's facing after the
    step) and steps with the unit's own rules (`TimedMove::speed_for`, `slope_multiplier`, `step_speed`,
    `brakes_for`: one function each for the unit and the men). On even ground every man repeats the unit's
    arithmetic exactly (test). The places come from one layout, `LandUnit::formation` (the grid
    `formation_grid`: files = the front rank `LandUnit::files`, the men who fire; size `width()` × `depth()`, the
    depth being the grid's ranks × the rank spacing `formation_rank_spacing`), computed once per tick for the men
    still walking, and used by the model, the view's men, its standard bearer (front rank) and its charge
    knock-downs (`man_rank` 0). The view plays a man's gait from his own speed (`view::MenDrawn`).
  - *Layout grid (review round 2).* `0x00642C90` computes the files only (`floor((W + 0.001 − a) / b) + 1`, at
    least 1); its callers cap them at the men (`0x0061CAB0` with its men argument, `0x00646B00` with its fifth,
    `0x0065A410` with its argument's `+0x20`, CONFIRMED by a read in review round 3). The ranks are
    ceil(men / files) (`0x00619E60` steps the slot counter through each rank of `files` slots, CONFIRMED). So the
    depth and the places come from one grid: a battle file's `num_soldiers` without a width gets ranks from its
    own men (was the stats' default ranks, so its ranks were spaced wrongly), and a front rank wider than the
    men is capped (was off-centre). The cap lives once, in `LandUnit::files`, and `LandUnit::width` narrows the
    width with it (`formation_width × files / formation_files`, round 3), so men of a capped block keep their file
    pitch everywhere, not only at setup (test `the_depth_and_the_places_come_from_one_grid`). The drawn block
    (`setup::drawn_size`: outline box, deployment spacing) keeps the old 2 m least depth, PROVISIONAL (ours, no
    such floor is known in the exe); the model's depth (the men's rank pitch) has none. `UnitInfo.ranks` (unused) is gone. The
    row placement `0x00619EB0` and the rank pitch p4 are decoded in *The shape layouts (formation5)* below (ours
    now places every row the exe's way); the anchor along the path stays ours.
  - *Which men the files cap reads (polish15, CONFIRMED static).* Every caller of the line shape's files
    (`0x0061CAB0` its third argument, `0x00646B00` / `0x00646C70` / `0x00646D40` their fifth, `0x0065A410` /
    `0x0065A6D0` their argument's `+0x20`) caps at the count it is passed. The fire step's `0x005694D0` passes the
    unit's soldier count `+0x18` (with the width of the owner's `+0x510` → `+0x384` entry `+0x24`) and caps at it;
    `+0x18` is the live count: `0x00575580` / `0x005755D0` take a dying soldier out of the vector at `+0x1C` and
    lower it, and the reform `0x00581200` lays `+0x18` men out over the slots. So the files and the ranks follow
    the living men, not the starting men. Ours had `files()` and the grid's ranks on `max_men` and the
    no-width fallback `width()` on `men`; now all three read `men` (`files()` caps at it, `formation_grid` takes
    ceil(men / files), the fallback width is those files × the file spacing), so the depth and width shrink as
    the unit loses men (test `the_files_the_width_and_the_grid_follow_the_living_men`). (The view now re-places the living men whenever
    the model lays the formation out again; see "The shape kinds" below.)
  - *When the men are re-laid (polish15-fix, CONFIRMED static).* The review found the grid reflows every tick in
    ours, but `0x00581200` re-lays only behind a gate. Traced: `0x00581200` is vfunc `+4` (the
    per-tick update) of the fire-order base, vtable `0x013239B0` (drill vtables `0x013239F0` volley, `0x01323A30`,
    `0x01323A70`, `0x01323AB0`, `0x01323AF0` share it), and is also called once from `0x0066EC20` (first tick,
    guard `+0x24`, states with vtables `0x013339E0` / `0x01333A20` / `0x01333A60`). The base ctor `0x0051A230`
    sets `+0x18` = a slot object (`0x007F2680`, 0x3C bytes, freed in `0x00521ED0`), `+0x1C` = the width (owner
    `+0x510` → `+0x384` → `+0x24`), `+0x20` = 0. The update: when `0x00569410` (unit `+0x1F4` → `+0x10`) is 0 and
    men (`+0x18`) × 0.5 < `+0x20`, it zeroes `+0x20`, calls the shape (`+0x628`) vfunc `+0x24` (out: 0x24-byte
    slots; the slot object, the width, the men), and sends each soldier `i` farther than `0x0057E1F0` + 1.5 m
    from slot `i` there (soldier vfunc `+0xB8`); then vfunc `+0x38` true → done `+0xC` = 1, else tail-call the
    fire step `+0x3C`. `+0x20`'s only writer found: `0x00539910` (vfunc `+0x34`, the candidate gather) adds 1
    per call per candidate `i < 0x005694D0` that is ready (`0x0063E290`) and whose current behaviour (soldier
    `+0x2D4`, present flag `+0x35B`, `0x007FF390`) has vfunc `+0x34` true and result `+0x1C` == 0
    (`0x008056D0` copies the motion task's result `+0x18` once the task is done, `+8`: 0 or 1). The gather runs
    every fire-order tick: the volley step `0x0055EB30` (tail-called from the update's end) calls vfunc `+0x34`
    into the global vector `0x01450DD0` each call and waits for max(1, round(men × 0.025)) candidates; no other
    writer of `+0x20` was found. The result: `+0x18` = 1 is written when the entity's motion state (`+0x390`)
    reaches 7 (`0x00818834`, `0x0081936B`; `0x008185C3` also when the entity's `+0x13C` is 1); the task ends with
    it left 0 when it finishes by the state-6 path (`0x00818810`, via `0x0062DF10`; `0x00819310` with `+0x44`),
    i.e. a move that ended short of its goal, so the counter counts, per tick, the front-rank men standing on
    an ended-short move, and the re-lay sends the ones far from their slots back. The counter re-lay does not
    re-grid: the slots it lays are the shape's over the living men, and only men farther than `0x0057E1F0()` +
    1.5 m from slot `i` move (`0x0057E1F0`: 1.5 if `0x0055C9A0(4)`, else the row float at `0x00F88300` + `+0x194`
    → `+0xC`). The update's periodic block (tick % 10 == unit id % 10, `DAT_014A35D0` = 10 from `0x00407740`)
    only stamps `+0x2784` (the in-formation tick) through `0x00557BC0` / `0x00557C20` / `0x00557C80`.
    *The re-slot after losses is a different mechanism:* `0x005755D0` (dying soldier removal) sets unit `+0x278C`
    = 1 and `+0x2788` = 20 (when soldier `+0xEC` is clear and shape vfunc `+0x30` is true), pulls the men behind
    forward along the shape's vfunc `+0x34` chain and compacts the soldier vector; `SQUAD::update` `0x00580A30`
    runs the state update (`0x00580990`, under `+0x278D` / `+0x278E` clear) and then decrements `+0x2788` once per
    tick; each order update found checks `+0x278C && +0x2788 == 0` (`0x00581200`, `UpdateUnitMoveOrderReissue`
    `0x00581AE0`, `0x00583150` / `0x005732B0` / `0x005735A0` through the this-adjust thunks `0x004F9DF0` /
    `0x004F7C60` / `0x004F8790` (`this − 0x18`, null-safe; the states they serve: see "The thunks" below), and `0x0066E9B0`)
    and calls `0x0057AFC0(slot, width, 1, -1)` → `0x00555CA0`: shape vfunc `+0x24` layout with the living men,
    nearest-slot assignment `0x005373F0` (`0x00502770`, threshold 16.0 or FLT_MAX), the soldier vector rebuilt in
    slot order, `+0x278C` cleared, then the in-place check `0x00557C80` with tolerance 4.0 (`+0x2784` = tick when
    the mean distance is under it, else 0). The unit ctor `0x00518DC0` starts with `+0x2784` = 0, `+0x2788` = 0,
    `+0x278C` = 1 (a re-slot at the first update). Other `+0x278C` setters: `0x00521F80`, `0x00531680`,
    `0x005380A0`, `0x0057B0E0`, `0x005819B0` (not traced; not losses). A move's issue or re-issue
    (`IssueUnitMoveOrderToSoldiers` `0x00584E10`) assigns no slots (review round 3, CONFIRMED static): its shape
    `+0x628` vfunc `+0x24` with `+0x18` only computes the slot points the men are issued to; the one assignment
    inside it, `0x0057AC10` (when `0x005516D0` holds for a man), runs once per order (order byte `+0x44`).
    `0x00576B30` has two callers only: `BuildUnitMoveOrder` (`0x0051A8E3`, an order behind the front) and
    `0x00581AE0` (below).
    *The move order lays out every tick short of the end (review round 2, CONFIRMED static).*
    `UpdateUnitMoveOrderReissue` `0x00581AE0` calls `LayOutUnitMoveOrderSlots` `0x00576B30(due)` (due = `+0x278C` set and `+0x2788` = 0)
    every tick while the order's at-end flag `+0x3C` is clear, and only when due once it is set. `0x00576B30`
    takes the slot buffer (`0x007F2D70` on order `+0x34`); at the order's first update (byte `+0x7C` `bFirstUpdateDone` clear, then
    set; `BuildUnitMoveOrder` `0x0051A530` makes that first call for an order behind the front, else the
    first update does, short of the end or when due) it runs `0x0057AFC0(slots, width, due, -1)`;
    later only while `0x00532360` holds (no man on a building structure, squad `+0x734`) with the anchor `+0x30`.
    `0x0057AFC0` lays the shape out over the living men; with `due` set it is the full re-slot (`0x00555CA0`,
    clearing `+0x278C`), so the pending flag is cleared only by a due call. A moving unit short of its end
    therefore re-lays at the tick after a loss; at the end it waits the 20 ticks.
    *Which removals start the countdown (CONFIRMED static).* `RemoveSoldierFromSquadSlots` `0x005755D0` (squad vtable `0x013211D8` slot `+4`)
    always sets `+0x278C`; it sets `+0x2788` = 20 and pulls the men behind forward along the shape's vfunc `+0x34`
    chain (args: index, the formation width at battle unit `+0x384` → `+0x24` as a float, the men; the line
    shape's `+0x34` is `0x00646F40` in vtable `0x01333AA8`, which passes the width and its spacings `+0x08` /
    `+0x10` to `0x00646D40`; the earlier Ghidra plate's "file count" was wrong) only when the soldier's
    byte `+0xEC` is clear and the shape's vfunc `+0x30` is true; otherwise it swap-removes the soldier with the
    last. Its only caller is `0x00575560` (clears soldier `+0x388`, the squad, then the squad's `+4`), reached
    from a unit's remove-soldier `0x005756D0` (`0x00574E90` calls it after taking the soldier off the unit's
    lists) and `0x0057E260` (move a soldier to another squad, 22 callers: re-squadding). A death is the soldier's
    vfunc `+0x104` `0x00653A10` (every soldier vtable, e.g. `0x01333714`; `0x006D5C40` is the slot in the
    `0x0132B7D8` family): it clears `+0xEC`, then removes the soldier from his unit (`0x00563040` →
    `0x00574E90`), so every death restarts the countdown. `+0xEC` is set only by the soldier's reset
    `0x00660AB0` (vfunc `+0xF8`, which also clears `+0x388` and leaves the battle's live lists). The shape:
    kind from the unit's active special ability (see "The shape kinds" below; `0x00579780` stores the shape
    parameters at unit `+0x2F0`..`+0x2FC` and re-creates each squad's shape; the earlier note here that unit
    `+0x2F8` is the kind was wrong: `0x00531630` passes the kind and those four parameters), built by the
    factory `0x00531680` (via `0x00531630`): 0 the line (`0x00620ED0`, vtable `0x01333AA8`; layout
    `+0x24` `0x0062FD60` → `0x00620850` → `0x0065A6D0`, ranks by `0x00619E60`; files `+0x10` `0x00642D00`),
    1 (`0x00620F20`, `0x01333AEC`), 2 (`0x00621120`, `0x01333BEC`), 3 (`0x00621140`, `0x01333C28`), 4
    (`0x006211A0`, `0x01333C64`), 5 (`0x006211D0`, `0x01333B30`), 6 (`0x006210D0`, `0x01333BB0`), plus
    `0x00620F70` (`0x01333B6C`, from `0x005196F0`). Vfunc `+0x30` is `0x00447880` (true) for all but kinds 1 and
    2 (`0x00462CB0`, false). 
    *The shape kinds (formation worker, CONFIRMED static).* `0x005498D0` picks the kind from the unit's special
    abilities: it asks `0x0055C9A0(ability)` in the order 6, 0, 1, 2, 3, 4 and the first active one gives kind 3,
    2, 4, 5, 1, 6 respectively; none gives 0, the line. The ability switch `0x00578860` (through `0x005315A0`)
    and `0x00579780` re-create the shape when an ability starts or stops. So kinds 1 and 2 (vfunc `+0x30` false)
    are the shapes of abilities 3 and 0: a loss there makes the re-slot pending (`+0x278C`) without restarting
    the countdown, which then falls due at the first tick it is out. Ours: `march::SHAPE_KINDS`,
    `LandUnit::shape_kind` and `reslot_tick` (test `a_loss_in_shape_kind_1_or_2_keeps_the_countdown`).
    Every kind's layout is traced (formation5 and formation6 below). PROVISIONAL: ours never clears an active
    ability.
    *The shape parameters (formation4, CONFIRMED static).* Every shape stores the squad's `+4` at `[1]`; the line
    ctor `0x00620ED0(a, p1, p2, p3, p4)` (through `0x00620EB0`) stores p1..p4 at `[2]`..`[5]`. Kind 6
    `0x006210D0(a, p1, p2)` is the line with p3 = 3.5 and p4 = 2.0 fixed; kind 3 `0x00621140` is the line with
    p3 = p4 = 1.0 plus p1, p2 again at `[6]`, `[7]`. `0x00579780` stores the unit's four (p1 → `+0x2F8`, p2 →
    `+0x2FC`, p3 → `+0x2F0`, p4 → `+0x2F4`); its callers pass p1 = p2 = an entity's `+0x4C` (or twice it), p3 the
    unit's file spacing (`+0x138` / `+0x148`), p4 `+0x13C` / `+0x14C`; `fire_and_advance` (ability 7, in
    `0x00578860`) passes p3 = `0x00557570()` while on, `+0x138` when off. The line (vtable `0x01333AA8`): files
    `+0x10` (`0x00642D00` → `0x00642C90`) = floor((width + 0.001 − p1) / p3) + 1, capped; width `+0x14`
    (`0x00642B50`) = p1 + p3 × (files − 1); `+0x18` (`0x00642A60`) the width of ceil(men / files) files. Kind 6
    (vtable `0x01333BB0`, light infantry): files `+0x10` (`0x00642E10`) = min(`0x00642D90`, cap) with `0x00642D90`
    = floor((width + 0.001 − p3/2 − p1) / p3) + 1; width `+0x14` (`0x00642BA0`) = p1 + p3 × (files − 1) + p3/2;
    layout `+0x24` `0x006302E0`. The line generator (`0x00620850`, vtable `0x01333AA0`) holds width, p1, p2, p4,
    the slots, `[1]` and p3; its first rank (`0x0065A410`) takes files from (min(row length, width), p1, p3),
    capped at the row's slot cap, placed by `0x00619EB0` alternating sides by parity at spacing p3; every row
    is laid this way, not only the first (corrected in formation5: `0x0065A6D0` / `0x00619E60` are the line's
    counting pass, which only advances the slot counter, not its later ranks). Kind 6's generator (`0x00620920`, vtable
    `0x01333BA8`; `0x0065A4F0` / `0x0065A790`) uses `0x00642D90` and shifts a rank's start by half a pitch: plus
    when the generator's `+0x18` bit 0 is set, minus when the previous rank's files were odd and this rank's even
    (the stagger). `0x00659E70` walks the anchor's rank objects (`0x00555DF0`; list `+0x14`, count `+0x10`;
    row start `+8`/`+0xC`, direction `+0x10`/`+0x14`, length `+0x18`, slot cap `+0x20`). Turning light infantry
    on (`0x00578860` case 4 → `0x0052ED80`, task `0x00515210`, vtable `0x013247C8`) keeps the files (old shape
    `+0x10` of the width), takes kind 6's width of them and re-forms at the mean of old and new widths
    (`0x00537FA0`); `fire_and_advance` keeps the files and re-forms at the new shape's width (`0x00537E40`).
    Ours: `march::SHAPE_KINDS` records these; the layouts are in formation5 and formation6 below. Still
    PROVISIONAL: the width task's later updates (which set kind 6's files).
    *The shape layouts (formation5, CONFIRMED static).* The factory `0x00531680` gives kinds 0, 1 and 5 the
    unit's p1..p4; p3 is the spacing between the men of a row, p4 the pitch between rows (a straight anchor
    piece `0x00659F00` lays a row every p4 metres along its length). Each layout vfunc `+0x24` builds its
    shape's generator, then the anchor walk `0x00659E70` hands each rank object the men left; a row asks the
    generator how many it holds (capped at the row's slot cap, the men left) and places them across itself.
    - *Row placement `0x00619EB0`.* A row of n slots whose first is the layout's running slot count c: the
      first slot sits on the row centre when n is odd; when n is even, half a spacing to the left if c is odd,
      to the right if c is even. Each later slot steps one spacing past the last one on its side: an odd slot
      index to the left, an even one to the right. So every row is centred. The row direction is (dirY, −dirX)
      of the anchor's direction, the unit's right hand (`0x007F2680`, `0x0063A1D0`). The placement stops a row
      early when a slot's rank (slot / files for the line) differs from the first slot's; with every row but
      the last full this never happens.
    - *Open-ground anchor `0x007F2680`.* One straight piece from the front centre straight back along the
      facing, its length the shape's measured depth: `0x007FC920` calls the shape's measure (vtable `+0xC`),
      which is each kind's own (formation6, CONFIRMED static, this settles the old "kinds 1/5 anchor sizing"
      line): the line (kinds 0, 6) `0x0063A1D0` → `0x0063A9D0`, depth (ceil(men / files) − 1) × p4 + p2, centre
      at front − facing × (ranks − 1) × p4 / 2; the diamond `0x0063A2F0`, centre (A − 1) × p4 behind the front
      (A = ceil √men); the wedge `0x0063A750`, centre (T(men) + 1) × p4 / 2 behind (T the triangle row of the
      last man); kinds 2 and 3 `0x0063A590` → `0x00643B50`, a k × k box with k = floor √men (see below), centre
      k/2 behind; kind 4 `0x0063A660`, the template's n × n plus the shape's p1 (`+8`) wide and p2 (`+0xC`) deep,
      centre n/2 behind (the box depth halved less p2 halved). Ours: `Formation::front`, the distance
      from the measured centre to the front, per kind (tests in `march.rs`). Ours lays every order this way;
      the exe lays a moving order's rows along its planned path from the path cursor, wheeling on its arcs
      (`0x007F2DA0`): PROVISIONAL in ours. Missing: what the path piece classes' vfuncs `+0x14` / `+0x18`
      return, the path builder reached from `0x00815F80`, and how an arc piece lays its rows.
    - *Kind 1, the diamond.* Layout `0x0062FDC0` builds generator `0x006208B0` (vtable `0x01333AE4`), whose
      `+0x20..+0x28` come from `0x006209F0(men)`: A = `0x0063C010` = the single-precision square root rounded
      to nearest plus one when the root is above it, i.e. ceil(√men); B = A/2 + A²/2 rounded half to even
      (truncated); C = A² − B; `+0x2C` the shape's `+4`, `+0x30` p3, rows p4 apart. Its row count
      `0x0065A490` → `0x00660D50(A, r, cap)` = min(r′ + 1, cap) with r′ = r for r ≤ A − 1, else 2(A − 1) − r:
      rows of 1, 2, …, A, …, 2, 1. Placement `0x0061A260` is `0x00619EB0` with the rank read from the
      triangle table, mirrored past B (rank = 2A − T[A² − 1 − c] − 2). B is one short for odd A (the banker's
      rounding), harmless: T[A(A − 1)/2] = A − 1 either way. Counting pass `0x0061A1C0`.
    - *Kind 5, the wedge.* Generator ctor `0x00620990` (vtable `0x01333B28`; its row function `0x0065A690` is
      not a defined function in the database): `+0x20` the shape's `+4`, `+0x24` p3; row r holds
      clamp(r + 1, 1, cap). Placement `0x00619B60` is `0x00619EB0` with the rank from the triangle table.
      Counting pass `0x00619B10`.
    - *The triangle table* `0x014D05B4`, filled by `0x00622780` (called at `0x00410B8A`) with 0, 1, 1, 2, 2,
      2, …: slot c's row in a triangle of rows of 1, 2, 3, … slots. It holds 400 entries (`0x190`), so a wedge
      or diamond past 400 men reads past it (an engine limit; ours computes the row, `triangle_row`).
    - *Kind 6, light infantry.* Ctor `0x006210D0` fixes p3 = 3.5, p4 = 2.0; layout `0x006302E0` builds
      generator `0x00620920` from the shape's getters (`0x00647050` p1, `0x00646BC0` p2, `0x0064CA30` p3,
      `0x00679B00` p4). Its row `0x0065A4F0` stagger: an odd row index shifts the row's start half a spacing
      to the right; otherwise, when the previous row held an odd count and this one an even, half a spacing
      to the left. Its files are `0x00642D90` over the layout width, which the width task `0x00515210` sets
      (not traced, BACKLOG "Battle formation left"): ours takes the unit's files, PROVISIONAL.
    *The squares and the pike wall (formation6, CONFIRMED static).* Kinds 2, 3 and 4 have no generator: their
    layout vfunc `+0x24` places every man itself from the order point P (the front centre) along the unit's
    heading vectors (heading h → θ = h·π/32768, front F = (sin θ, cos θ), right R = (F.y, −F.x)), at 1 m
    whatever the unit's spacing. Each slot record (0x18 bytes, set up by `0x00620A90`) gets a place, a heading
    short at `+0xC` (65536 to a turn, positive to the right, added to the unit's) and pose flags. The turn
    constants are `0x014D0844` (0x2000), `0x014D05AC` (0x4000), `0x014D05A8` (0x6000), `0x014D0650` (0x8000).
    The exe's floor √men (in `0x00630390`, `0x006311B0`, `0x00643B50`) is the float root rounded to nearest,
    less one when the root is below that. The earlier note that kind 3's layout is `0x0062FE20` was wrong: its
    vtable `0x01333C28` slot `+0x24` is `0x006311B0`.
    - *Kind 2, the pike square `0x00630390`.* k = floor √men. A block of k × k, row by row from the front left,
      1 m apart (row 0 on P's line). A block man faces by his sector from the block's middle: on a diagonal
      (|x| = |y|, not the middle) out along it (±0x2000 in front, ±0x6000 behind, + on the right); else left of
      |y| −0x4000, right of it +0x4000, behind |x| 0x8000, otherwise ahead. The men past k² stand on a ring
      half = (k + 1)/2 out from the block's middle across and one metre outside it front and back: first its
      four corners, (−half, 1 ahead) −0x2000, (half, 1 ahead) 0x2000, (half, k behind) 0x6000, (−half, k
      behind) −0x6000; then eight walks in turn from the corners, step t = q/8 + 1 metres along each: from the
      front left back (−0x4000) and right (0), from the front right left (0) and back (0x4000), from the back
      right forward (0x4000) and left (0x8000), from the back left right (0x8000) and forward (−0x4000). Its
      box (`0x0063A590` → `0x00643B50`) is k wide and k deep. The ring's corners are laid by the remainder
      r = men − k² (kept at `[EBX+0x14]`): the first whenever r ≥ 0 (`CMP 0` / `JL` at `0x006308D4`, always
      true), then after each corner `SUB 1` / `JS` (from `0x006309F0`) stops when r − 1 goes negative, and past
      the four r − 4 > 0 runs the walks. So with r = 0 to 3 the layout lays men + 1 slots, one corner more
      (r = 0: corner 0 alone; r = 3: all four). That slot, index men, is never held (CONFIRMED static):
      `AssignSoldiersToNearestSlots` `0x005373F0` makes one pair per soldier, the k-th with slot k, so only
      slots 0 to men − 1 are paired, and `AssignSlotsTwoOpt` `0x00502770` swaps soldiers between those pairs;
      `LayOutSlotsAndAssignSoldiers` `0x00555CA0` rebuilds the soldier vector from the pairs and
      `0x00557C80` checks the soldier count's slots only. The square `0x00631560` lays exactly its men (its
      loop runs to nMen). Ours: `Formation::laid_slots` (march.rs) counts the extra corner and puts it at slot
      men; only the slots past it (a man added after the lay-out, ours) stand behind.
    - *Kind 3, the pike wall `0x006311B0`.* Full rows of `files` from the left, 1 m apart, the remainder row
      starting at (files − rem) >> 1 slots in; every heading 0. files = `0x00642EE0` = min(`0x00642C90`(width,
      p1, 1.0), men) = min(floor(width + 0.001 − p1) + 1, men). Its box is the pike square's k × k (the shared
      measure `0x0063A590`), not its own rows. Ours takes the width as the unit's fresh width and p1 as two man
      radii: PROVISIONAL. Missing: the re-form width the ability switch `0x00578860` gives it, and which
      entity's `+0x4C` reaches p1 through `0x00579780` (shape `+0x18`).
    - *Kind 4, the square `0x00631560`.* The template table `0x014D0690` (0x20-byte entries: side `+8`, slot
      array `+0x1C` of 0x18-byte records), built once by `0x00621F00`, holds templates of side n = 7, 8, … ,
      51, adding sides until the last holds 400 slots or more. `0x00661690` picks the template whose 8n − 8 to
      8n − 1 men fit, else the first (7): an engine limit of 407 men (ours lifts it; n = max(7, men/8 + 1),
      the table's pick for 1 to 407). Template n (`0x00621D10`): four sides front, right, back, left
      (`0x006361C0`, directions (1, 0), (0, −1), (−1, 0), (0, 1)), each an outer row of n men one metre out from
      an inner row of n − 2, the outer starting half a metre before the inner corner; every man faces out
      from his side (0, 0x4000, 0x8000, −0x4000), the first of each inner row an eighth turn left of that;
      then the centre's 7 (`0x00629E40`): (0, 0), (−1, 0), (1, 0), (0, 1), (−1, 1), (1, 1), (0, −1), facing
      ahead. `0x0066AE60` turns a template slot into the unit's frame from the template's middle, which
      `0x0063A660` (an n × n box) puts n/2 behind P. ORIGINAL BUG (`0x00621D10`): the template starts the
      front side at the box's corner (−n/2, n/2) and `0x006361C0` takes it as the inner row's, so both rings
      sit 1 m left of and 1 m ahead of the template's middle while the box and the centre block are centred
      on it: the front row stands 1 m ahead of the order point and the centre block off the rings' middle.
      Ours starts at the inner corner (−(n/2 − 1), n/2 − 1): rings centred, the outer n wide and deep.
    - *Not modelled.* The slot headings are recorded (`Formation::slot_heading`) but ours does not turn the
      men to them nor weigh them in the slot cost (PROVISIONAL, see `slot_cost`); the slot pose word (FormationSlot
      `+0x14`) is not modelled: the pike wall `0x006311B0` writes 5 for its first row, 4 for its second, 3 for the
      rest and its remainder row (CONFIRMED static); what the men do with it is untraced. A slot past a square's laid men (more slots than the shape places) gets a place of its own
      behind the shape (ours).
    Ours: `march::Formation` (`row`, `row_first`, `place`, `slot_heading`, `front`), `row_offset`,
    `triangle_row`, `diamond_half`, `floor_sqrt`, `pike_square_block_heading`, `pike_square_extra`,
    `square_slot`, `SQUARE_SPACING_M`, `SQUARE_MIN_SIDE`, `LIGHT_INFANTRY_SPACING_M` / `LIGHT_INFANTRY_PITCH_M`;
    tests `places_and_frame_match_the_drawn_block`, `line_rows_alternate_by_slot_parity_and_stay_centred`,
    `the_diamond_rows_grow_to_a_then_shrink`, `the_wedge_adds_a_man_per_row`, `light_infantry_rows_stagger`,
    `the_pike_square_faces_out_from_its_block_and_ring`, `the_pike_wall_fills_rows_from_the_left`,
    `the_square_lays_its_template_rings`. The diamond's and the wedge's measured centres moved to the exe's
    (formation6). Ghidra names (V5): `CalcCeilSqrtInt` `0x0063C010`, `InitDiamondDims` `0x006209F0`,
    `CountDiamondRowSlots` `0x00660D50`, `PlaceDiamondRow` `0x0065A490`, `InitTriangleRowTable` `0x00622780`;
    formation6 (each analyze_function_completeness ≥ 70): `BuildSquareTemplateTable` `0x00621F00`,
    `BuildSquareTemplateForSide` `0x00621D10`, `AddSquareTemplateSide` `0x006361C0`, `AddSquareTemplateCentre`
    `0x00629E40`, `FindSquareTemplateForMen` `0x00661690`, `GetSquareTemplateSide` `0x00642F20`,
    `PlaceSquareTemplateSlot` `0x0066AE60`, `LayOutSquareFormation` `0x00631560`, `InitFormationSlotRecord`
    `0x00620A90`, `CalcSqrtShapeFiles` `0x00642E70`, `CalcPikeWallFiles` `0x00642EE0`, `MeasureSqrtShapeBox`
    `0x0063A590`; structs `SquareTemplateTable`, `SquareTemplate`, `SquareSlotRecord`, `FormationBox` (the
    measure's 0x24-byte box), `FormationLayoutResult`, `FormationAnchor`, and `FormationShape` grown to
    0x18 (`+4` the squad's `+4`, p1..p4 at `+8..+0x14`); globals `g_SquareTemplateTable`,
    `g_wHeadingEighthTurn` / `QuarterTurn` / `ThreeEighthsTurn` / `HalfTurn`. Names in these notes only:
    `MeasureSquareShapeBox` `0x0063A660` (prototype and plate applied; 65.9, held under 70 by register-only
    locals and shared globals), `LayOutPikeWallFormation` `0x006311B0` and `LayOutPikeSquareFormation`
    `0x00630390` (not yet given a V5 pass).
    *The backward order `+0x8C` (CONFIRMED static).* The path request builder `0x008102F0` sets the request's
    `+0x8C` (at `0x00810D3F`) when the goal minus the summary's front centre (`+0x9C`) has a negative dot product
    with the summary's front (`+0x34`), i.e. the goal lies behind the men's front. `BuildUnitMoveOrder`
    `0x0051A530` then lays out with the re-slot argument (`0x00576B30(1)`), a full re-slot that clears the
    pending `+0x278C`. The test runs only when the request's `+0x28` sub-object's first dword is 0; its meaning
    is untraced, and ours takes it as 0 for every order (PROVISIONAL). Ours: the issue in `march_men` (test
    `an_order_behind_the_front_clears_the_pending_reslot`).
    *The thunks (CONFIRMED static).* `0x004F9DF0` → `0x00583150` is the attack state's update; `0x004F7C60` →
    `0x005732B0` the reform state's entry for the fire-at-target task group; `0x004F8790` → `0x005735A0` the reform
    state's entry for the idle group (the idle state `0x01450BDC`, reload, reform, ready), which sets the task's
    `+0x2C` to 1. An idle unit enters the reform state only when `0x00573770` holds: never when the squad's
    vfunc `+0xB4` check fails or `0x0057ABD0` holds; always when the task's `+0x2C` is 0; otherwise only when
    the men were not found in place (`0x00557BA0`: the stamp `+0x2784` older than 20 ticks). The idle update
    `0x005577C0` runs the in-place check `0x00557BC0` → `0x00557C20` (tolerance `0x0057E1F0()` + 2.5 m when
    `0x0055ABF0` holds, else 1 m), which stamps `+0x2784`, every `[0x014A35D0]` ticks (10), staggered by unit.
    *The idle reform gate (formation4, CONFIRMED static).* The squad tick `0x00580A30` pushes a new idle task when
    the task stack is empty (`0x0051A3A0`: `+0x2C` = 0, `+0x28` = the width) and the state machine's pass
    `0x010FBBD0` runs the idle state's check (`0x00573770`) before its update, so a unit whose order ended reforms
    in its next tick. `0x00557BA0` (disassembled) is `stamp ≠ 0 && stamp + 20 > tick`, unsigned; a stamp of 0
    reads as none (ours: a stamp at our tick 0 reforms again at tick 1; the exe's first tick is not traced). The stamp
    `+0x2784` is cleared by the ctor `0x00518DC0`, written by `0x00557C80`, and set to the tick by `0x00557D80`,
    called from the reform state's update `0x00581CA0` (slot 1 of vtable `0x01325428`, the object built by
    `0x0051B350` for `0x005735A0`, `0x005732B0`, `0x005733C0`, `0x005734C0` and `0x0066E9B0`) when
    `ProcessLocomotiveStateMachine` reports every move done (it then sets done `+0xC`), and from `0x00580DD0`
    (vtable `0x0132509C`, built by `0x00519C20` from `0x0053FD30`; it stamps while `+0x1C` is set and `+0x10` <
    0x2D; its task is not traced). So the reform's end stamps the men in place and the gate stays shut for 20
    ticks. The idle update `0x005577C0` runs the check when tick % `[0x014A35D0]` (10) equals the battle unit's
    vfunc `+0x10` (its serial) % 10, with the anchor `0x00555DF0()` and the task's width `+0x28`.
    `0x00557C20` lays the shape out over the living men; `0x00557C80` takes the mean of `sqrt` distances from
    each counted soldier (`+0x1A8` clear; `+0x48`/`+0x50`) to his slot k: with a count > 0 and tolerance ≤ mean
    the stamp is 0, else the tick. The tolerance is `0x0057E1F0()` + 2.5 m when `0x0055ABF0` (mounted: category
    0 or 5, or 3 without ability 15 `dismount` active; `0x0055C2A0` is that with `dismount` inactive) holds, else
    1 m. `0x0057E1F0` is 1.5 while ability 4 (`light_infantry_behaviour`) is active, else the float `+0xC` of the
    `entity_training_levels` row of the unit's training level (unit `+0x194`, `0x00EED960`), through
    `0x00F88300` (`this + 0x5C + level × 4`, an array of six row pointers; level 6, an unknown key, reads past
    it). Unit `+0x194` is the stat block's `+0x3C` (CONFIRMED static: the record builder `0x00E8F1D0` stores
    `0x00EED960`'s result there at `0x00E8F3CC`, and the battle unit's stat block at `+0x158` is the record's copy),
    so the morale rules that read `+0x194` (front-attack adjust, fighting-cavalry scaling, fatigue concern; ours
    `MoraleInputs::training_level`, formerly `formation_class` and always 0) now get the unit's real training level.
    ORIGINAL LIMIT, lifted: the exe's six levels are hardcoded (`0x00EED960`) and their rows are read by that enum,
    so a mod-added level has none. Ours keeps the enum for the morale rules (a mod-added key is 6, as in the exe)
    but reads the tolerance from the key's own row in the merged database, so a mod-added level keeps its row
    (`setup.rs` `training_of`, test `a_mod_added_training_level_keeps_its_row`); a key with no row gets no tolerance
    and is logged once per key. The shipped tolerances: mob 1.7, rabble 0.5,
    poorly_trained 0.38, trained 0.2, well_trained 0.1, elite 0.13 (real-install test
    `entity_training_levels_give_the_drill_tolerances`). The full re-slot's own in-place check (tolerance 4.0)
    is left out: every path ours models (the reform's end, the idle check) overwrites its stamp first.
    Ours (`march.rs`): `LandUnit::idle_gate`, `in_place_check`, `drill_tolerance`, `MenMarch::in_place_at`
    and `idle_reformed`; `reslot_tick` runs the reform entry for a standing unit when the gate holds (laying
    out in full when a re-slot is due, else keeping each man's slot) and the check otherwise; `march_men`
    stamps the men at the reform's end. Tests: `the_idle_gate_holds_for_a_new_task_or_an_old_stamp`,
    `a_unit_reforms_once_after_its_orders_end`, `the_in_place_check_measures_the_mean_offset`,
    `the_drill_tolerance_follows_the_training_level`, `a_due_reslot_walks_a_standing_units_men_to_their_slots`,
    `mounted_follows_the_category_and_dismount`, `training_levels_follow_the_exes_order`.
    PROVISIONAL (ours): every standing unit is idle (the fire-at-target group's reform entry `0x005732B0` and
    its gate are not traced); no controller veto (vfunc `+0xB4` of the controller stack `+0xADC`) and no
    stored-move veto (`0x0057ABD0`, soldier `+0x619`: ours issues the reform's moves at once); the check counts
    every living man (soldier `+0x1A8` not traced); the serial is the unit's id (the exe's is drawn from the
    battle model's counter `+0xBC`, which other battle objects may share; a debugger sitting).
    Ghidra names (formation4, V5 scores): `UpdateIdleStateInPlaceCheck` `0x005577C0` (78.5),
    `CheckIdleSquadMenInPlace` `0x00557BC0` (70.7), `LayOutShapeAndStampMenInPlace` `0x00557C20` (79.0),
    `StampSquadMenInPlaceByMeanOffset` `0x00557C80` (82.5),
    `IsSquadInPlaceStampRecent` `0x00557BA0` (89.4), `StampSquadMenInPlaceNow` `0x00557D80` (89.4),
    `UpdateReformStateUntilMenStand` `0x00581CA0` (83.6), `GetTrainingLevelRow` `0x00F88300` (89.4),
    `GetUnitDrillTolerance` `0x0057E1F0` (84.7), `IsUnitMounted` `0x0055ABF0` (88.6), `IsUnitRidingAndNotDismounted`
    `0x0055C2A0` (91.4), `ParseTrainingLevelKey` `0x00EED960` (93.3); the period global
    `g_dwInPlaceCheckPeriod` `0x014A35D0` (10, set from 10.0 by `0x00407740`).
    *The soldier action flags `+0x35B` / `+0x35C` (soldierflags, CONFIRMED static).* Every soldier holds one action
    in place in his action slot (the slot object at `+0x2CC`; the action at its `+8`, soldier `+0x2D4`, 0x87
    bytes; the action's done flag `+0x10` is soldier `+0x2E4`, its tick count `+0x14`). Slot `+0x8F` (soldier
    `+0x35B`) is *present*: the base action ctor `0x007E3A50` sets it (and numbers the action from the battle
    model's counter, ctx `+0x28` → `+8` → `+0x5C`). Slot `+0x90` (soldier `+0x35C`) is *started*: the slot update
    `0x0066E780` (from the soldier update `0x00658C50` at `0x00658E1F` / `0x00658E3D` / `0x00658E7A`, and from
    `0x00670EB0`), for a present action not done and not started, runs `0x0063CDD0` when slot `+0x91` is set,
    zeroes `+0x14`, runs the entry vfunc `+4` and sets started; then, while not done, the update vfunc `+8` and
    `+0x14`++ (so entry and first update fall in one call); when done and slot `+0x92` is set it destroys the
    action and clears both flags; it returns present. Every install (`0x006533E0` timed move, `0x00653560` plain
    move, the family `0x00651F30`–`0x00653430`) destroys a present action (vfunc 0 with 1; the timed move's
    `0x007EDF70` frees only when its flags & 5 == 5), clears both flags and builds the new one in place. Readers
    test busy = present and not done (`+0x35B` == 1 and `+0x2E4` == 0): `0x0063E290` (ready: not present, or done,
    or `0x008118F0` when `+0x224` ≠ 0) and some 20 sites (`0x00531BE0`, `0x00539840`, `0x0055D470`,
    `0x0055F600`, `0x0055F710`, `0x0057FB70`, `0x00583390`, `0x00583980`, `0x005F3040`, `0x005FCFB0`,
    `0x00646BE4`, `0x0064DE80`, `0x006718F0`, `0x006B29F0`, `0x006D9A40`, `0x006DC700`, `0x006F9FFB`,
    `0x006FC7C0`); `0x007FF390` returns the action when present, done or not, for the fire state's gather
    (`0x00539910`, `0x0062F750`): the one reader for which a present done action differs from none.
    The idle update `0x005577C0` first, for each living man, destroys a present action and clears `+0x35B`, and
    always clears `+0x35C`, then runs the in-place check: an idle unit's men lose their actions on every idle tick
    without a transition. Ours (`march.rs` `LandUnit::idle_update`, test `the_idle_update_ends_every_mans_action`):
    each living man's walk to his slot ends (`ManMarch::walk` None) and his move is ended (`finished`, which
    stands for `0x0063E290`); his kept speed (`+0x440`) is outside the action and stays. Ours has no started flag
    (a move starts in its issue tick, as entry and first update share a call) and no present-but-done state (no
    fire gather). Other clears, for states ours does not build (BACKLOG): unit-level `0x0051A230` (fire-order
    base ctor, every man), `0x0051AC80`, `0x00520110` (from `0x0051F150`), `0x00522080`, `0x0052F770`,
    `0x00563420` (thunk `0x004F951D`), `0x00553770` (thunk `0x004F9C2D`), `0x00574090`, `0x00580C20` (vtable
    `0x01325018`), and `SQUAD::update` `0x00580A30` at `0x00580B49` (every man, when `+0x515` is set, `+0x508`
    ≠ 1 or `+0x498` set, `+0x460` == 0 and `0x00586440(0)` fails; those fields are not traced); soldier-level
    `0x005BE5D0`, `0x005EFFE0`, `0x005DD2A0` / `0x005DD500`, `0x0064A520` (from `0x008057F0`), `0x0069B5F0`,
    `0x006C2880`, `0x006CCE10`, the death vfunc `+0x104` `0x00653A10`, the resets `0x00660AB0` and `0x00623960`;
    `+0x35C` alone (with `+0x364`) at `0x00658E24` in the soldier update and in `0x0066B080`.
    *The slot assignment and the reform walk (formation3, CONFIRMED static).* `LayOutSlotsAndAssignSoldiers` `0x00555CA0` (the full re-slot,
    also the partial lay-out with its re-slot argument clear) lays the shape out (vfunc `+0x24`), then
    `AssignSoldiersToNearestSlots` `0x005373F0` pairs soldiers and slots and the soldier vector is rebuilt in
    pair order, then the in-place check `0x00557C80` (tolerance 4.0). The assignment starts from the identity
    (the k-th soldier in the vector to the k-th slot; soldiers whose vfunc `+0xF4` object has `+0xA8` set, and a
    soldier the layout names, are paired in fixed slots and left out of the swaps: first, or after the swaps in the
    last slots when `0x0054C8B0` on the squad's `+0x510` returns 1), each pair costed by
    `ComputeSoldierSlotAssignmentCost` `0x005281D0`: the squared distance c from the soldier (`+0x48`/`+0x50`) to the slot, plus, when c > 0,
    (1 − min(f · d̂, 1)) × 0.5 with d̂ the unit direction to the slot and f the soldier's facing (`dir16` of
    `+0x64`), so a slot behind him costs up to 1 more than one as far ahead. The 2-opt `AssignSlotsTwoOpt` `0x00502770` then runs
    over a list of eligible pairs with a cost limit: FLT_MAX and no round limit for a re-slot, 16.0 and one
    round otherwise. Each round takes the eligible pair of largest cost below the limit (strictly larger
    replaces, so the first wins a tie); none left ends it. Unlimited: the first other eligible pair in list order
    with its cost below the limit whose two swapped costs sum strictly below the present sum is swapped with it
    (`SwapSlotPairSoldiersAndRecost` `0x0057CBC0` swaps the soldiers and recomputes both costs); without one the pair leaves the list
    (swap-remove). Limited: of the other eligible pairs below the limit, the one whose swap lowers the sum most
    (strictly below 0, the first on a tie) is swapped. The costs' defaults are 0 for a pair without a soldier
    and FLT_MAX for one without a slot (the caller's stack pair at `0x0053741C`, passed as the 2-opt's `this`). The slots are laid at the order's path cursor
    (`LayOutUnitMoveOrderSlots` `0x00576B30` takes the point from `0x007F2D70` → `0x0080AEA0`, a lerp along a path
    segment by the cursor's fraction). The reform state's entry `EnterReformStateMoveMenToSlots` `0x005736B0` runs
    the full re-slot (`0x0057B000`) when its byte `+0x35` is set, then for each soldier `MoveSoldierToSlotIfOutOfPlace` `0x0055F160(slot pos,
    slot heading, 0.0625, 910)`: a soldier strictly farther than 0.0625 m² from his slot, or turned more than
    910/65536 of a turn (the float 65536/72, 5 degrees, rounded) from its heading, gets a plain move there (`0x00653560`). In battle `0x00653560` stores the target at soldier `+0x5FC` and the
    soldier's update `0x00658C50` issues it once (`0x007E2040`), at the run speed (the entity record's `+0x20`)
    and the run kind when the distance² is strictly above 25 (the float at `0x01325D54`, compared at
    `0x00658D07`), else at the walk speed (`+0x1C`) and the walk kind. In deployment (`0x0055AD60`: the battle
    state `+0x14` at most 1) `0x00653560` instead builds a move descriptor from the slot and hands it to the soldier's
    vfunc `+0xB8`; INFERRED: that vfunc puts the soldier
    at his slot at once (it is not traced). The plain move runs the same locomotive as the timed move, so it
    brakes for its point (`arrival_braking`): a soldier within the 0.5 m minimum braking distance stops at once.
    Ours (`march.rs`): `slot_cost`, `assign_slots` (`PARTIAL_ASSIGN_MAX_COST`), `LandUnit::relay` (the men stay
    where they stand: each man's offset from his slot is `ManMarch::side` and `lag`, kept in slot order with his
    `figure` number), `LandUnit::send_men_to_slots` (`REFORM_MOVE_SQ`, `REFORM_RUN_SQ`, `ManMarch::walk` and
    `kind`), and `march_men` walks each man straight to his slot in two dimensions. A re-slot (and the first
    lay-out, the exe's pending one from the constructor) assigns in full; the every-tick lay-out short of the end is
    partial; a new order's first issue lays out (in full for an order behind the front, partially for one short of
    its end; none for one at its end) only when the tick's lay-out step did not, or did so partially for an order
    behind the front, so the men are assigned once per tick as the order's update does (`0x00581AE0` →
    `0x00576B30`; test `a_reissue_tick_assigns_the_men_once`); a re-issue lays out nothing (test
    `an_at_end_reissue_does_not_reassign`). INFERRED: a new order built in a tick whose lay-out step already laid
    out (the previous order short of its end) adds no partial round; the order of `BuildUnitMoveOrder` and the
    squad update within one tick is not traced. A standing unit sends its men through the idle reform (see
    "The idle reform gate" above). Tests: `a_slot_behind_costs_more_than_one_as_far_ahead`,
    `the_full_assignment_untangles_and_the_partial_one_swaps_once`, `a_reslot_keeps_every_man_where_he_stands`,
    `the_reform_walks_near_men_and_runs_far_ones`, `a_due_reslot_walks_a_standing_units_men_to_their_slots`.
    PROVISIONAL: the lay-out point (ours: the unit's position, INFERRED to be where the cursor puts the
    reference soldier's slot); no special soldiers; every man faces the unit's facing, so the heading test of
    `0x0055F160` never holds and the cost's facing term uses the unit's facing.
    *Width and depth: the exe measures the men (CONFIRMED static).* Every reader takes the squad summary at squad
    `+0x62C`: width `+0x650` (summary `+0x24`), depth `+0x654` (`+0x28`), radius `+0x670` (`+0x44`). `0x0053C180`
    refreshes it on alternate ticks: `0x00519150` builds one 0x1C-byte record per soldier (soldier, x `+0x48`,
    z `+0x50`, heading `+0x64`, radius `+0x620`, surface `+0x10`, `pGroundProvider` `+0x18`); `0x0053A390` the centroid (`+0x0C`/`+0x10`),
    a 7-sample moving average of its change (`+0x4C`/`+0x50`) and the speed `+0x94` (|velocity| × 10 / ticks
    elapsed); `0x0053A1D0` the dominant heading from a 256-bin histogram of the soldiers' headings; `0x0053D1A0`
    the box: one soldier gives width = depth = his radius; otherwise the principal axis from the 2×2 covariance,
    the extents along both axes over the soldiers with 0 < distance < mean + 2σ, each plus twice the largest
    soldier radius, the box centre (`+0x18`/`+0x1C`), and the width/depth naming by the axis nearer the
    previous heading (`+0x20`) with a 65% majority test over four heading buckets; debug lines when battle
    `+0xB0` → `+0x2B44` is set. `0x0053C3D0` then writes `+0x44` = the distance from the box centre to the
    farthest soldier and the nearest soldiers to the box's corners. So the width and the depth are one measured
    box, not a layout. Detail (formation worker): the axis is the eigenvector (−Sxy, Sxx − λ) of the larger
    eigenvalue λ of the covariance sums centred on the mean, normalised, falling back to +x when its length is at
    most 1e-4 or both components at most 0.001; the spans start at 0 (so they hold the mean); the four candidate
    headings are ±A, ±B as 16-bit angles (atan2 × 10430.378, rounded to even); each man votes for the nearest
    (a later candidate only when strictly nearer), the most votes win (the first on a tie) unless they are fewer
    than round(0.65 n), when the candidate nearest the previous heading wins; facing ±A the width is the B extent.
    The radius `+0x44` (squad `+0x670`) is the farthest man from the box centre, without his radius; the front
    centre `+0x9C` is the centre plus the front × depth / 2, and `+0xC4` / `+0xD8` the front corners. `+0xB0` is
    a copy of the centre (the unit-list range tests `0x00701310` / `0x006C9E40` read it through `0x0053FBE0`).
    Ours: `march::measure_squad_box`, measured at the start of each battle unit tick by `LandUnit::refresh_summary`
    (before `reslot_tick`) from the men where they stand (each at his slot plus his `side` and `lag`), with
    `LandUnit::man_radius` (column 12 of the man's `battle_entities` record, set in setup.rs); `width()`,
    `depth()` and `radius()` read it, and `morale::near_units` uses its centre and radius. *The refresh's
    schedule (formation3, CONFIRMED static):* `SQUAD::update` `0x00580A30` runs `0x0053C180` at `0x00580A7C` when
    the squad has men; in battle (`0x0055AD60` false: the battle state `+0xB0` → `+0x14` past 1) it measures only on
    the ticks whose parity is the summary stamp's (`+8`), every tick in deployment. Ours measures on the ticks whose
    parity is the unit id's (test `the_box_is_measured_on_alternate_ticks`). PROVISIONAL: the stamp is the unit
    body's serial (`0x00518E3C` reads it through `0x0057E910`, numbered by the battle's counter at `0x0051B816`),
    ours the unit id, and the exe tick's parity against ours is not traced; the radius enlargement for attached
    members (`0x00607540`, below) is not modelled. `evade_room` (`0x0057A6F0`) takes
    max(radius, speed × 3 s) as its strip's length and starts it at the front centre `+0x9C` (`0x0053FBD0` at
    `0x0057A753`; test `the_evade_strip_runs_the_squad_radius_from_the_front_centre`); the skirmish strips start at
    the front centres too and test the boxes with the oriented-box overlap `0x0055A1B0` (BATTLE_FIDELITY.md
    "Skirmish"). The box is also measured when a unit joins the battle (`Battle::add_unit`) and when it is put down
    without a tick (deployment, reinforcement entry, garrison entry: `LandUnit::place_at`). In deployment that is the
    exe's every-tick refresh; placements also force one (`0x0054FDD0`, called from the best-heading placement
    `0x0056DEB0` and the group merge `0x0057B0E0`, CONFIRMED static). PROVISIONAL: whether the building and
    reinforcement entries refresh at once is not traced. The first "previous heading" for the width/depth naming
    is 0: the constructor `0x00518FA0` copies the summary's `+0x20` from the zero word `0x014A3498` (set by
    `0x00407A10`; CONFIRMED static); later refreshes take the last box's heading. The leave/arrive test
    `0x005857A0` reads each soldier's circle, so ours tests the men (`men_inside`), not the box.
    Release cost (formation3, 20 units of 160 men, `march_cost`): `measure_box` 0.061 ms per tick, `march_men`
    with the two-dimensional walks 0.136 ms per tick.
    Ghidra names (V5 write-ups): `UpdateSquadSummary` `0x0053C180`, `MeasureSquadSummaryBox` `0x0053D1A0`,
    `ComputeSquadSummaryRadiusAndCorners` `0x0053C3D0`, `GetSquadSummaryCentre` `0x0053FBE0`,
    `FindEnemyUnitsWithinRange` `0x00701310`, `GetFormationShapeKindFromAbilities` `0x005498D0`,
    `IsSpecialAbilityActive` `0x0055C9A0`, `CreateSquadFormationShape` `0x00531630`,
    `SetUnitFormationShapeParams` `0x00579780`, `CheckIdleUnitCanEnterReform` `0x00573770`; globals
    `g_dwSquadSummaryManRecordsHeader` `0x014A3ED8` (the man-record vector), `g_pSquadSummaryDistances`
    `0x014A3ED0`, `g_wHeadingHalfTurn` `0x014A3A1C` (0x8000, set at start-up by `0x004079D0`).
    Ghidra types (polish38, polish42): the three functions are methods of class `SquadSummary` (0x120 bytes:
    `0x0053E500`, called last by `0x0053C3D0` with this summary, the records and the man nearest the front
    point, writes `+0x98`, `+0xEC` and `+0xF4`..`+0x11C`; their meaning is UNKNOWN), with the fields above
    plus `FrontPoint` `+0x9C`, `CentrePoint` `+0xB0`, `FrontCornerA` `+0xC4`, `FrontCornerB` `+0xD8`, each a
    20-byte `SquadSummaryGroundPoint` built by `0x007FD070(.., 3, 0)` on battle `+0xB4` from the hint point
    `GetSoldierGroundPoint` `0x00659480` gives for the nearest man (ECX = that man). The ground point (CONFIRMED
    static): `flX` `+0x00`, `flY` `+0x04` (height), `flZ` `+0x08`, `pSurface` `+0x0C` (the surface it stands on;
    `0x0057E870` takes the height from its plane at `+0x30`..`+0x3C`, y = −(a·x + c·z + d)/b, or from the terrain
    `0x0066BB40` when it is null), `pFrame` `+0x10` (the surface's moving frame, `pSurface`→vfunc `+0x10`→`+0x80`,
    or null): `InitializeGroundPointOnSurface` `0x007EAD80` stores xyz and the surface and, with a frame, converts xyz
    to the frame's local space (`0x007F66F0`); `GetGroundPointWorldPosition` `0x0080E9D0` (477 callers) turns it
    back to world space. `GetSoldierGroundPoint` copies the soldier's stored point (`+0x4F8`) when his byte
    `+0xB4` is 1, else builds one at his x/z on his surface `+0x10` (`0x005473B0`) or, when `+0x18` is set, calls
    slot `+0x10` of the object at provider `+0x1C` with the output point. `0x007FD070` searches the surfaces
    outward from the hint's and copies the hint (setting its fifth argument) when none is found; its own steps
    are UNKNOWN. The record is `SquadSummaryManRecord` (0x1C:
    `pMan`, `flX`, `flZ`, `wHeading`, `flCoverRadius` (the man's `+0x620`), `pManSurface` (the man's `+0x10`), `pManGroundProvider`
    (the man's `+0x18`, what it points to is UNKNOWN)), built by `InitializeSquadSummaryManRecord` `0x00519150`,
    and the vector `SquadSummaryManRecordVector` (count `+0x08`, data `+0x0C`;
    `g_pSquadSummaryManRecordsData` `0x014A3EE4` typed as a record pointer). The four `.y` reads of
    `g_aHeadingDirectionTable` in `0x0053D1A0` carry user references to the table base `0x0176CFF8` instead of
    the interior `0x0176CFFC`. Completeness (raw): `0x0053D1A0` 76, `0x0053C3D0` 82, `0x0053C180` 61 (effective 79; after the
    polish42 fix round typed its record pointer and named its build buffer; the rest is register-only splits and the untyped battle pointer), `0x00607540` 86, `0x00519150` 92, `0x00659480` 74, `0x007EAD80` 79, `0x0080E9D0` 85; the rest is
    register temps the decompiler splits and the untyped surface pointers. New `BattleSoldier` fields (polish42):
    `pGroundSurface` `+0x10`, `pGroundProvider` `+0x18`, `wHeading` `+0x64`, `bUseStoredGroundPoint` `+0xB4`,
    `StoredGroundPoint` `+0x4F8`, `flCoverRadius` `+0x620`. A plain soldier is 0x660 bytes (CONFIRMED static: the
    base constructor `0x0061CB90`, vtable `0x01333714`, and the plain-soldier allocations push 0x660 at
    `0x0051C05E` and `0x0069477E`); the `BattleSoldier` type holds those 0x660 bytes. Larger objects that run
    soldier code exist (0x69C and 0x6C8 bytes, below), so 0x660 is the plain soldier's size, not every
    soldier's. The base constructor writes `+0x620` as a copy of the radius `+0x58` at `0x0061D2D8`.
    *The radius grown over attached entities (polish42 fix round, CONFIRMED static):*
    `UpdateSoldierCoverRadiusOverAttachedEntities` `0x00607540` (ECX = the soldier) sets `+0x620` = `+0x58`,
    then walks a linked list at `+0x674` (next `+0x04`); each node holds a pointer array (count `+0x2C`, data
    `+0x30`) whose entries point (at their `+0x20`) to an entity, and for each non-null one `+0x620` becomes
    max(`+0x620`, the x/z distance from the soldier (`+0x48`/`+0x50`) to the entity's `+0x48`/`+0x50` plus the
    entity's `+0x58`). So `+0x620` is the radius of a circle round the soldier that also covers his attached
    entities; it equals `+0x58` only while the list is empty. Its two callers both read fields past 0x660:
    `UpdateGunTrainAttachNodes` `0x005ED4B0` first moves each list node's point (`+0x0C`/`+0x10`) to the soldier's position plus the
    offset (`+0x68C`, `+0x690` − node `+0x18`) rotated by his heading `+0x64`, then calls `0x0060CAC0`,
    `0x005ED450` and `0x00607540`; it is reached from vtable slots `+0x58` / `+0x54` (`0x005ED030` / `0x005ED060`, which
    first call the base `0x00659D20` / `0x00659D40`) of the vtables `0x0132BE3C`, `0x0132C0C8`, `0x0132C2F0`
    and `0x0132C528`, and from `0x005BD200` / `0x005BD310` (slot `+0x1C0` of `0x0132C2F0` / `0x0132C528`). A plain
    soldier's vtable `0x01333714` holds `0x00659D20` in that slot. The other caller is the code at `0x005FCF83`
    (in `0x005FCEB0`, slot `+0x0C` of the gun-train vtables; see below), which also reads `+0x69D`..`+0x6A8`. `0x0132C2F0` and `0x0132C528` are
    written by `0x005A1EB0` and `0x005A38C0`, the constructors of the 0x6C8-byte objects that `0x00544E60`
    allocates once per entry of its `+0x1F4` → `+0x24` list when it has a record at `+0xB4` (`0x005A1EB0` when
    that record's `+0x18` is 6, else `0x005A38C0`), each followed by 0x69C-byte objects built by `0x0059EED0`
    (vtable `0x0132BA04`, whose slot `+0x58` is `0x005ED1F0`, which does not call `0x00607540`). So in a plain
    soldier and a 0x69C object `+0x620` stays the base constructor's copy of `+0x58` as far as these writers go;
    in a 0x6C8 object it is grown. The 0x6C8 constructors run the base chain `0x0059EAB0` (vtable `0x0132C0C8`)
    → `0x0058EF40` (vtable `0x0132BE3C`), so those two are base vtables of the 0x6C8 objects. `0x0059EED0` is
    also called from `0x0051B7D0` (at `0x0051C2B4`; the function that pushes the unit `+0x60` record at
    `0x0051C09B`), besides `0x00544E60`'s five calls.
    *Write scan (polish42 fix round 2, read-only `run_script_inline` over every instruction):* 27 direct stores
    to `[reg+0x620]` (MOV/MOVSS/FST*; the scan finds only direct 4-byte stores and misses wide or
    overlapping ones: memcpy, rep movs, struct copies); 2 in `0x00607540`, 1 at `0x0061D2D8`, and 24 others: `0x0070A254`
    (`0x00709FB0`, an object whose `+0` is copied from an argument), `0x0071FE72` (constructor `0x0071FC80`,
    vtables `0x0132CE04` / `0x01340828`), `0x007D70E0`, `0x00875999` (vtables from `0x0135456C`),
    `0x00879BF4`, `0x0087A3B0` / `0x0087AC73` / `0x0087AC7D` / `0x0087AD0B`, `0x0087C189`, `0x00A3D992` /
    `0x00A3DEB8` / `0x00A3EF81`, `0x00A3FB1D` (vtables from `0x013779AC`), `0x00DF2BBA`, `0x00E0F0F2`,
    `0x00E83836` / `0x00E83DA2` / `0x00E840DA` (`0x00E83E40` runs on a stack object of `0x00545C20`),
    `0x01123519`, `0x01130C36`, plus stack locals at `0x00788B68`, `0x00A42791` and `0x0100EAB4`.
    *Classified (attachradius, 2026-10-22, CONFIRMED static):* none of the 24 is in a soldier.
    `0x0070A254` / `0x007D70E0`: the alliance AI object (constructor `0x00709FB0`, update `0x007B25A0` reached
    through `0x00583380`; analysis/ai/AI_RESEARCH.md), whose `+0x620` is its previous mode, compared with
    `+0x61C`; it has no vtable (`+0` is copied from an argument). `0x0071FE72`: constructor `0x0071FC80`
    (installs `0x0132CE04` then `0x01340828`, no soldier base constructor; caller `0x0071C740` ← `0x007101D0`).
    `0x00875999` .. `0x0087C189` and `0x00A3D992` .. `0x00A3FB1D`: campaign objects (vtables `0x0135456C`,
    `0x01354488` with the registered-campaign-object vtable `0x01321294`, `0x013779AC`, installed on the
    same base register) built by the campaign model load `0x00872550` ("**CAMPAIGN MODEL LOAD**") /
    `0x008742C0`, from `0x00987070` (startpos / save open). `0x00DF2BBA` / `0x00E0F0F2`: the DB manager
    (`0x00E0EFB0` loads "ancillaries_table" into `+0x620` on first use; set-up `0x00DF20D0`, caller
    `0x004809A0`). `0x00E83836` / `0x00E83DA2` / `0x00E840DA`: the 0x66C-byte battle map definition (vtable
    `0x013BEFE4`, constructors `0x00E83550` / `0x00E83B40` / `0x00E83E40`, a heap object that
    `0x00545C20` `create_battle_map_definition` builds; the earlier "stack object" was wrong). `0x01123519` /
    `0x01130C36`: the renderer ("custom_sprite_land_radar_t0", "Custom_Sprite.fx"; caller chain through
    `0x01127580`, "Rigid.fx" / "ShadowMap View"). `0x00788B68`, `0x00A42791`, `0x0100EAB4`: ESP-relative
    locals. The field's address is taken (`LEA [reg+0x620]`) only at `0x00A8F019` (`0x00A8EFF0`, a container
    passed to `0x00A86F80`, no refs) and `0x0123A9E4` / `0x0123AA55` (`0x0123A9D0`, engine code, data ref
    `0x014291F8`). The wide stores the scan misses include three 16-byte `MOVUPS [reg+0x618]` at `0x0115056D` /
    `0x0115272B` / `0x011555AD`, which overlap `+0x620`: each is the last row of a 4x4 matrix stored at
    `+0x5F8`..`+0x637` from terms like (a+b)/(a−b), a renderer projection matrix, not a soldier. No copy is soldier-sized: 0x660 / 0x69C / 0x6C8 appear only as allocation sizes
    (`0x0051B7D0`, `0x00544E60`, `0x00691C10`, `0x006BA560`, `0x006BAE10`, `0x005AD510`) and in the deleting
    destructors `0x00624D40` / `0x005AD470`. None of the soldier constructors that call the set-up `0x0061CB90`
    (`0x0051B7D0`, `0x0059EC50`, `0x0068C750`, `0x00691C10`, `0x006BA560`, `0x006BAE10`, `0x007EB3B0`) holds a
    store. So a man's `+0x620` is written only at `0x0061D2D8` and by `0x00607540`; the plain (`0x01333714`)
    and intermediate (`0x0132B7D8`) soldier vtables hold `0x00659D20` at `+0x58` and never reach it, and the
    other caller `0x005FCF83` lies in `0x005FCEB0`, slot `+0x0C` of the same four gun-train vtables
    (`0x0132BE3C`, `0x0132C0C8`, `0x0132C2F0`, `0x0132C528`). The call always runs: the checks before it (`+0x69D` == 1, `+0x6A8` set, the
    record `+0x1E8` → `+0x18` == 5, the `+0x6A8` object's `+0xEC` == 1) gate only the copy of that object's byte
    `+0x118` into this one's at `0x005FCF5F`; the five calls at `0x005FCF67`..`0x005FCF83`, ending in
    `0x00607540`, run unconditionally.
    *What the 0x6C8 objects are (CONFIRMED static):* gun trains, the limbered team of an artillery unit.
    `0x00544E60` builds one from the unit's `+0xB4` gun-train `battle_entities` record, whose `+0x18` is the
    skeleton kind (man 0, horse 1, camel 2, elephant 3, artillery 4, gun_train_2 5, gun_train_6 6, ammo_caisson
    7; table `0x013B3DD0`, setup.rs `garrison_radius`): kind 6 takes `0x005A1EB0` (vtable `0x0132C2F0`), any
    other `0x005A38C0` (vtable `0x0132C528`, 5 nodes); base chain `0x0059EAB0` (`0x0132C0C8`) → `0x0058EF40`
    (`0x0132BE3C`, sets `+0x674` = 0); teardown `0x005A8140`, deleting destructor `0x005AD470`. `0x005A1EB0`
    passes the pivot {0, 6.0} to `0x0059EAB0` (`+0x68C` / `+0x690`) and adds its nodes with `0x005B1F60`
    (offset, width) = (0, 2), (2, 2), (4, 2), (8, 2), (12, 2), (14.5, 2), (16.5, 1), each at the gun's position
    plus the pivot rotated by `+0x64` (x = `+0x68C`, z = `+0x690` − offset). The node (Ghidra `GunTrainAttachNode`, 0x34 bytes from `0x005B1F60`, constructor `0x0058F210`,
    vtable `0x0132BE38`) holds its position `+0x0C`/`+0x10`, width `+0x14`, offset `+0x18`, heading `+0x1C` and
    its slot vector at `+0x20` (count `+0x2C`, data `+0x30`), linked from the gun's `+0x674` (head) / `+0x678`
    (tail). Slots (0x2C bytes, constructor `0x0058F180`; `0x005B1E40` makes the offset relative to the node,
    `0x005B1EF0` allocates and pushes it): six horse slots, kinds 0/1, at (∓0.75, −2), (∓0.75, −6), (∓0.75,
    −10) on nodes 1–3, also kept in the gun's vector `+0x6AC` (count `+0x6B8`, data `+0x6BC`); `+0x6C0` at
    (0, −14) on node 4 and `+0x6C4` at (0, −16.5) on node 5. `0x005CD7C0`, last in the constructor, records the
    node chain at `+0x680` / `+0x684` / `+0x688` and `+0x694` / `+0x698` from the tail node.
    *Attaching (CONFIRMED static):* `AttachSoldierToGroup` `0x00655B20` (member, group) calls the group's slots `+0x1B0` and `+0x1B8`,
    takes the member out of the unit's man list `+0x2CC` (`0x00564890`) when its `+0x1F0` is set, and sets
    member `+0x1EC` = group. The gun train's `+0x1B0` (`0x005F0080`, not a defined function) first runs the
    base `0x005EFFE0`, then puts a member whose slot `+0xEC` is true (`0x005DB820` on the 0x69C vtable
    `0x0132BA04`: its `+0x67C` is set) in `+0x6C4`; else one whose `+0xF0` is true (`0x00447880`, always true on
    `0x0132BA04`) and of kind 1 (horse) in the nearest free horse slot; else a kind-7 (ammo_caisson) one in
    `+0x6C0`. `0x005C38C0` fills the slot (`+0x20` = member). Men are never slotted: the plain and intermediate
    soldier vtables hold `0x00462CB0` (returns false) in `+0xEC` and `+0xF0`. So the attached entities are the
    team's horses, the caisson and the `+0x6C4` member (0x69C objects from `0x0059EED0`, whose own `+0x674` is a
    byte flag, not a list).
    *Squad man list (CONFIRMED static):* the squad partition `0x0057B0E0` takes men from the unit's `+0x2CC`
    (entries with `+0x388` == 0) and adds them with `0x0052F770`. The gun train stays in `+0x2CC` (only the
    attached members leave it), so it is a squad man and the squad summary's man record (`0x00519150`, through
    `0x0053C180`) reads its grown `+0x620`: a limbered artillery unit's squad box covers its whole team.
    *Ours:* `cover_radius_over_attached` (ntw_sim battle/model.rs) is the rule of `0x00607540`; ntw_sim builds
    no gun train, so no unit calls it yet and every modelled man keeps `cover_radius` == `man_radius` (exact:
    only gun trains grow it). The growth comes with the gun-train entity (BACKLOG §3 limbering and
    unlimbering).
    Ours (`march.rs`): `MenMarch::laid` holds the formation; `LandUnit::reslot_tick` (every active unit's tick,
    `placeholder_movement`) restarts a 20-tick countdown at each loss, lays out every tick while the unit's own
    order (`TimedMove`) is short of its end, re-lays when the countdown runs out and clears the pending re-slot
    only then (a death in tick T re-slots in T + 20, the exe's order with the death before the squad update);
    `march_men` re-lays at a new order's first issue (behind the front, or short of its end), never at a
    re-issue; the first tick lays out. The structure gate is always open in ours (a
    garrison leaves its building on any move). The counter re-lay is not modelled: ours has no collision pushes
    and no move that ends short, so no man is ever that far from his slot (see `LandUnit::formation`). Tests:
    `a_move_short_of_its_end_re_lays_every_tick`, `a_moving_unit_re_lays_after_a_loss_at_once`,
    `the_men_keep_their_slots_until_the_reslot`. The view (`view.rs`, `MenDrawn`) draws each figure from
    his own model entry (the entry whose `ManMarch::figure` is his; a re-slot moves him to another entry, so he
    walks from where he stands to his new slot, as the exe's nearest-slot assignment `0x005373F0` does; test
    `each_figure_follows_his_own_man_through_a_reslot`).
  - *A moved target (review round 2).* A move that follows a unit (the advance, the fire-target approach) runs a
    follow state (`0x0054D9F0`, entered at `0x0054D900`, left at `0x0054D9A0`; callback table
    `0x0131EAB4..0x0131EAD0`) that each tick re-reads the target (object `+0x28`, vfunc `+4` with `+0x24`, `+0x2C`)
    and compares it with an int it keeps at `+0x38`; on a change it builds a new move order (descriptor
    `0x0051A940`, `BuildUnitMoveOrder` `0x0051A530`), i.e. a first issue `0x00584E10(1)` to every man. The first
    build waits for its path request (`+0x30`) to reach state 3 with byte `+0x18` set. Ours: `TimedMove::next`
    starts a new order (first) when the followed position changed, so men finished under an order at the end are
    issued again. The compared int is not traced: PROVISIONAL, how far the target must move (ours: any change).
  - *The follow state's order frames (review round 3, CONFIRMED by disassembly).* The three state callbacks
    are reached through thunks `0x004F9AE0` / `0x004F9B00` / `0x004F9B20` (each `this − 0x18`, null-safe) in the
    secondary table at `0x0131EAB4` (the sub-object at state `+0x18`). State `+8` is the unit's state stack
    (`UnitStateStack`, `0x45C` bytes: a `0x400`-byte arena, 8 frames `{state, bytes}` at `+0x400`, count `+0x440`,
    used `+0x444`, free `+0x448`, pending `+0x44C`, pop-allowed `+0x450`, popped `+0x451`, active index `+0x454`).
    `PushUnitStateStackFrame` `0x00522D70` (cdecl) records a frame at arena + used and returns its memory (no
    bounds check, never null); `0x80` is the byte size of the move-order state `BuildUnitMoveOrder` constructs in
    it, and the null test after the push is the compiler's placement-new check, not a permission flag (an earlier
    Ghidra comment said so; corrected). `InitializeMoveDescriptor` `0x0051A940` (thiscall, `RET 0xC`) fills the
    20-byte descriptor: request `+0`, `g_wDefaultZeroWord` `+4` (a static initializer stores 0), 0 at `+8`, the
    target key `+0xC`, the key source's vfunc `+0xC` flag `+0x10`. Before the first build the frames above the
    active one are popped (`PopStatesAboveActiveFrame` `0x005359A0`, ECX = the state, bare `RET`); on a key
    change, when pop-allowed is set and the active index is below count − 1, it sets popped and calls
    `TruncateUnitStateStack` `0x0056D990(index + 1)` (thiscall, `RET 4`): with the guard set it clears it, pops
    frames while count > the kept count (each state's vfunc 0 with 1 destroys it; used −= bytes, free += bytes),
    then sets the guard again; so the old child order is replaced, not stacked. `BuildUnitMoveOrder` (V5 plate,
    polish15) copies the descriptor's `+8` to order `+0x20`, and its layout block (resize the `+0x5C` slot vector
    to the unit's `+0x510` → `+0xBF4`, shape vfunc `+0x24` with the living men `+0x18`, front width from vfunc
    `+0x08`, the `+0x40` projection) runs only when that word is non-zero: the follow path's descriptor leaves it
    0, so a follow order skips it (CONFIRMED static). The entry `0x0054D900` fills a
    `0x54`-byte path query through the path source `+0x24` (vfunc `+4`, the owner's `+0x510` → `+0x384` entry's
    `+0x24` float and twice `+0x2EC`) and submits it to the path finder (`0x008020B0`, ECX = state `+0x14` → `+8`
    → `+0xB4`), keeping the request at `+0x30`. Ours has no state stack: a new order replaces the unit's
    `TimedMove`, the same result.
  - *The settling man (review round 2).* While a soldier's locomotive moves (`+0x118` = 1 and flags `+0x110` bit 0
    or `+0x117` = 0), `0x006543D0` sets his entity state `+0x1B8` from his stance `+0x1D8` and move kind `+0x220`
    (plain stances 0..3: walk 8, run 0xB, charge 0xD; stance 4: 9 / 0xC; stance 5: 0xA; `+0x114` → 0x17,
    `+0x115` → 0x18), and the display picks his locomotion graph by that state (`0x007A7190`). `+0x220` is written
    by `StartSoldierTimedMoveOrder` `0x00816900` from the order's `+0x38` through `0x00662E50` (other writers
    `0x00665910` (2), `0x00665D40`, `0x00816080`, `0x008163C0`, `0x00805AC0` (0), `0x00806070` (0)). So a man
    still moving to his place plays the locomotion of the move he was issued (walk, run or charge), not his unit's
    mode loop: CONFIRMED; ours keeps the issued kind (`MenMarch::kind`, `march::MoveKind`) and the view plays it.
    Whether a moving soldier starts a melee attack is not traced (the gate's action kinds, table `0x00811890`,
    show some actions start while moving): ours starts none, PROVISIONAL.
  - *The `+0x3A4` lead (review round 2).* `0x0080DA94` (in `0x0080BF70`) writes the locomotive's `+0x10`, not
    soldier `+0x3A4`; no byte write of `+0x3A4` exists in `0x00500000..0x00830000`, only reads (`0x0065965D`,
    `0x006597A5`, `0x0065998D`, all in `0x00659600`). It may be a field of the order object at soldier `+0x2CC`
    (its `+0xD8`); not traced. The tag stays.
  - *PROVISIONAL (ours):* the reference soldier is the block's centre and the rows are laid straight back from
    it (the places within a row are the exe's, `0x00619EB0`, formation5; the anchor along the path,
    `0x007F2DA0`, is not); the lag is along the facing only; which of our order
    sources maps to which descriptor (all ours clamp) and `0x0051A940`'s `+0x10` (from `0x0054D9F0`); the int
    `0x0054D9F0` compares (how far a followed target must move); a moving man's melee attack; the `+0x3A4` byte
    (keep speed) has no traced writer, ours takes it as set; the 0.25·D cursor step (`0x005856F2`/`0x00658C20`) is not
    modelled; the gate's action-kind skip (ours has no per-man actions), the locomotive result behind
    `0x00658C20` (ours: 1, which only matters short of the end, where no man of ours finishes), and the
    `0x005516D0` / `0x006527B0` alternatives (ours always gives the timed move); after a braked stop short of the
    destination (≤ 0.5 m) ours re-issues at the unit's stop, the exe's points stay at the end; a man left ahead of
    his place at a stop stays there; a march in which no man moved over a tick after the unit's move ended ends
    (the exe would wait); a gait change re-issues as a re-issue (whether `0x005600C0` rebuilds the order is not
    traced).
  - *Tests:* `march::tests::on_even_ground_every_man_keeps_his_place_and_the_unit_speed`,
    `a_man_in_the_mud_falls_behind_and_is_caught_up_by_each_reissue` (lag levels off near 2 m instead of growing
    0.4 m/s), `at_the_stop_each_mans_distance_is_his_lag`, `the_issue_point_stops_at_the_end_and_keeps_the_speed`,
    `the_reissue_gate_skips_finished_men_at_the_end`, `behind_men_walk_to_their_places_after_the_unit_stops`,
    `the_front_rank_is_the_units_files`, `a_formation_of_unknown_size_spreads_its_men`,
    `the_state_hash_covers_the_march`, `places_and_frame_match_the_drawn_block`,
    `the_depth_and_the_places_come_from_one_grid`, `a_moved_target_is_a_first_issue`,
    `finished_men_follow_an_enemy_that_moves_off`;
    `model::tests::slope_and_braking_rules`; `view::tests::men_are_drawn_where_the_model_walks_them`,
    `a_man_walks_to_his_place_after_his_unit_stands`.
  - *Cost* (release, 20 units × 160 men on a 64 × 64 height grid and a ground-type grid): `march_men` 0.16 ms per
    tick for 3,200 men (~50 ns a man), within a 0.33 ms battle tick; after the fix round (gate, kept speed, one
    layout) 0.13 ms; after round 2 (the layout once per tick, none for finished men) 0.12 ms
    (`march::tests::march_cost`, ignored: `cargo test -p ntw_sim --release -- --ignored march_cost --nocapture`).
  - *Hitch before / after.* Before: the §1.9 probe's unit-wide steps (all 160 men's cadence in one frame, level
    swaps at once). After: each man's speed and multiplier are his own, so the swaps spread over the men; the
    probe is not re-run here (in-game check in HANDOFF).
  - *Melee idles (kinds 0xF..0x13), CONFIRMED (polish16, 2026-10-14):* not in `0x007725D0`'s advance switch; their
    clip time is set each frame at `0x0077430C` from the soldier's clip clock (clip advance by slot kind, §1.9), so
    they play at rate 1. Ours plays them at rate 1 (`view::LoopKind::Plain`); the start is still the figure's own
    offset (PROVISIONAL). Kind 0x17 (rider combat) copies the mount's clip time (`0x00774282`, §1.9).
## 2. Soldier LOD (CONFIRMED values)
Tweaks in `VariantModelManager.cpp`: `variant_lod1` 5.0, `variant_lod2` 10.0, `variant_lod3` 15.0,
`override_variant_lod` true, `variant_lod_skip` 2. The chooser (`0x0125ED60`) with the override on: LOD 3 if
`lod3² ≤ d` and the mesh has 4 LODs, else LOD 2 if `lod2² ≤ d`, else LOD 1 if `lod1² ≤ d`, else LOD 0; `d` is the
squared camera distance in metres (CONFIRMED: the draw call at `0x012519C3` passes the instance's stored squared camera distance, ×1000 when the force-low-LOD tweak at `0x0183CC08` applies, i.e. the lowest LOD; the set-up calls `0x01250110`/`0x012509D0` pass 0).
With the override off it walks a per-mesh distance table instead. `variant_lod_skip` (`0x0125DC80`): with the unit
detail option 1 or 2 → skip `2 − 1`, option 3 → 0, else 2 (most detailed LODs dropped at load). Animated nodes have
`lod_distance_scaler` 1.0. Code: `unit_model::unit_lod` picks each man's LOD from the camera distance; his part entities swap to that LOD's mesh (built from the file's other LOD blocks, most detailed first) only when it changes. Equipment pieces stay at their most detailed LOD (their LODs are separate pieces). Horses switch by `warscape_animated_lod`: the first LOD whose distance is 0 or covers the camera distance × `lod_distance_scaler` (CONFIRMED loop `0x01146B20`; every horse has 10, 20, 50, 0 m; metres INFERRED); all LOD files are assembled with the same piece picks (`mount::animated_lod`, `MountModel::lower_lods`). We draw shadows with the same LOD (the exe's lowest-LOD shadows not done).

## 3. Standard bearer's flag (DRAWN in round 8; see the round 8 block at the top)
- `RigidModels/VerletItems/standard_bearer_flag.logic` (`0x00732190` loads it; also `siege_prop_flagpole*.logic`):
  text, `version 1.0`; a `rigid mast_rigid_euro_flagpoleA_01` (mass 100, radius 0.072, particles `top`, `bottom`, 1..6)
  and a `verlet_item sail_rigid_euro_flagpoleA_01` (mass 1.0, surface_area 1.0, `texture flag_`, drag 0.5, gravity 0.1;
  42 particles `p(x, y, z) t(u, v)`, 60 triangles, invisible `rope` constraints). Reader:
  `ntw_formats::verlet` (CONFIRMED on all three shipped files).
- Texture (`0x01227BD0`): `"flag_" + <key> + ".tga"` looked up in `RigidModels/Flags/Textures/flags.tai` (atlas
  `flags0..2.dds`), else `flag_default.tga`. The key is INFERRED to be the faction (`flag_france.tga` ...).
- **The drawn pole and the frame (round 7, CONFIRMED).** `rigid_equip_euro_flagpole01`, one of the 134 rigid
  attachments of `unitmodels\euro_equipment.variant_weighted_mesh` (140 vertices, 504 indices, textures
  `equip_diffuse` / `equip_normal` / `equip_gloss_map`), is a **4.40 m rod along its own +x**: box
  x `-1.020 .. 3.379`, y `-0.038 .. 0.038`, z `-0.046 .. 0.040`, so its origin sits **1.02 m up the pole**.
  The verlet file's coordinates are the pole's frame translated by `(0, -0.967, -0.087)`: the mast's particles are all
  at y `-0.955 .. -0.986` and span x `1.021 .. 2.183`, inside the pole's x span; the sail keeps that y and spreads
  **+z** to 1.572 (u `0.003..0.998` runs along x, v `0.0..0.999` along z). The `rigid` block is therefore a
  **collision proxy**, not the drawn mesh.
- **The bone (round 7, CONFIRMED).** `variantmodels\equipment\mesh*.variant_part_mesh` bind
  `rigid_equip_euro_flagpole_lod1/_lod2` to **bone 3**, and the standard bearer's `.unit_variant` lists the pole under
  category `equipment_personal`, which `warscape_equipment_themes.euro_standard_bearer` fills from
  `warscape_equipment_items.euro_flagpole` -> set `euro_flagpole` (so the pole is the bearer's **personal
  equipment**). In the FLAG_BEARER skeleton (`Animations/MEN/FLAG_BEARER/FLA_STAND/FLA_StandT.anim`, 41 bones) bone 3
  is **`Weapon3`**, a parentless root whose frame-0 x axis is `(0.004, 1.000, -0.002)` -- **+x is world up** -- and whose
  origin is `(-0.001, 0.400, 0.402)`. The pole therefore runs from 0.62 m below the bone to 3.78 m above it.
- **STILL UNKNOWN:** the transform the exe gives the verlet item. Its `0.967` y offset means the item's origin is
  not the bone origin, and whether the sail hangs down the pole or streams sideways is not settled by the data.
  Nothing is drawn yet.

### 3.1 The flag atlas (`*.tai`) — CONFIRMED, round 8
`rigidmodels\flags\textures\flags.tai` is **plain text**, and its own `#` header
(`AtlasCreationTool.exe -width 2048 -height 2048 -o flags`) documents the schema: a file name, two tabs, then
`<atlas filename>, <atlas idx>, <atlas type>, <woffset>, <hoffset>, <depth offset>, <width>, <height>`, with
**A = (woffset, hoffset)** and **B = A + (width, height)** as the image's two texture coordinates in the page.
Reader `ntw_formats::texture_atlas`.

- **CONFIRMED:** all **34** shipped `.tai` files parse, every page they name is a file in the install, and every
  rectangle lies inside its page (`flag_install::every_shipped_tai_parses`). They are not only for flags: the
  grass atlases, the campaign building atlases (`eu_diffuse.tai` 362 entries), the clouds and the naval idents
  use it too, so this reader is worth having for §2 as well.
- **CONFIRMED:** `flags.tai` holds **68 `flag_<key>.tga` images over 3 pages** (`flags0/1/2.dds`), including
  `flag_default.tga`. The keys are the campaign faction keys, and **38 of the 77 faction keys with a loc screen
  name have an image of their own**; the other 39 are the dependent factions — `spa_*`, `ita_*`, `egy_*`, `tut_*`
  and every `*_rebels` — which must take a parent's flag. That parent lookup is **UNKNOWN**; we draw
  `flag_default` for them (PROVISIONAL).
- The sail's own `t(u, v)` is already the flag image's 0..1, so the drawn coordinate is `A + t (B - A)`
  (`flag.rs::spawn_flags`). The look-up `flag_<key>.tga` and the `flag_default` fallback are CONFIRMED in the exe
  at `0x01227BD0`; the *key* being the faction is INFERRED (it fits 38 of 77 exactly and the 39 misses are all
  provably dependent factions).

### 3.2 The solve (`ntw_sim::battle::cloth`, round 9) — CONFIRMED shape, PROVISIONAL constants
Everything the cloth does to its own particles comes from the file. What is invented is named:

**Where the solve lives (round 9).** It was `ntw_formats::cloth` beside its reader; it is now
`ntw_sim::battle::cloth`, because it is a stateful `f32` integrator stepped once per frame and the
determinism item's audit scope is the model crates. `ntw_formats::verlet` (the `*.logic` reader) and
`ntw_formats::cloth` (the file -> `ClothSpec` -> `ClothParts` plain arrays) stay where they are.
`ntw_sim` keeps **zero** dependencies: the data crosses as a tuple of standard-library types, because
both crates are dependency leaves (`docs/DESIGN.md` §2) and a conversion needs one to depend on the
other. The install test moved to `crates/ntw_sim/tests/flag_install.rs` (`ntw_formats` is a
**dev**-dependency) and the round-9 before/after measurements are **byte-identical**.

| term | value on the shipped flag | source | tag |
|---|---|---|---|
| the hoist | six welds at **rest length 0.0000 m** onto mast particles 1..6 | file, CONFIRMED | CONFIRMED |
| the shape | **101** `srope` distance constraints, rest **0.148 .. 0.300 m** | file, CONFIRMED | CONFIRMED |
| the mast | a collision rod of radius **0.072** about `bottom .. top` | file, CONFIRMED (that it carries a radius) | INFERRED (that it is a cloth collision radius) |
| gravity | `gravity_coefficient * 9.81` = 0.98 m/s², uniform | coefficient CONFIRMED (0.1), the combination INFERRED | INFERRED |
| wind | `drag_coefficient * (surface_area / mass) * (wind - v)` = 0.5 · 1 · (w - v) | both CONFIRMED (0.5, 1.0, 1.0), **linear** drag INFERRED (the quadratic form is not excluded by the file but at the shipped 9 m/s wind it would pin the sail rigid at 40 m/s² against 1 m/s² of gravity) | INFERRED |
| wind speed | the battle file's `weather/prevailing_wind`, read as m/s (Austerlitz `(0, 9)`; five shipped battle files `(0, 0)`, and their flags hang) | the vector is CONFIRMED present; the metre is not | PROVISIONAL — target: the exe-side wind speed the verlet item is fed |
| `SUBSTEPS` 8, `ITERATIONS` 6, `DAMPING` 0.985 | — | nothing in the data | PROVISIONAL — target: the exe-side per-frame verlet update reached from `0x00732190` |

**Measured against the shipped file** (install test `the_shipped_flag_cloth_solves`, prints all of this):
- 4 s of Austerlitz's 9 m/s wind: the sail flies out to **1.31 m** of depth with its top edge still at
  y 2.579 and its bottom at 1.416 — i.e. out and nearly level, which is what a flag in a fresh breeze does.
- 4 s of still air: it folds down beside the pole to **y 1.119**, and every rope is within **1%** of its rest
  length. It is a stiff, fully triangulated sheet, so it does not collapse to the authored 1.647 m of cloth and
  it is still folding at 4 s. The exe's own limp-flag shape is UNKNOWN; this is PROVISIONAL.
- the relaxation sweep is monotone and the worst rope stretch falls with the iteration count
  (6 passes 1.4%, 192 passes 0.2%) — that monotonicity is the evidence that the solver is converged rather than
  accidentally still.

## 4. Trees and wind

### 4.1 Tree list items (CONFIRMED, `0x00EDA990`)
`TREE_ITEM` = `translation_x`, `translation_y` (f32), `scale` (u8, only when the list says `scaled`), `source`
(0, 1 = `farm_tree`, 2 = `farm_shrub`). No rotation: a per-tree angle must come from the renderer (UNKNOWN).
`VegetationPicker.cpp` tweaks `MIN_TREE_SCALE` 0.5 and `MAX_TREE_SCALE` 1.4 are read by `0x00EC33A0` into the
picker's `+0x4C` / `+0x48`.

**Round 11 closed the rotation question from data, negatively** (`the_tree_instance_i32_is_not_a_rotation`).
The schema is **closed at three columns** — `read_tree_lists` errors on a single leftover child and every
shipped list parses — so there is no fourth column an angle could hide in. And the unmapped `I32` cannot be
it: 0 on **3,843,976** of **3,844,367** instances, 1 on 256 and 2 on 135, **all of them on `ottoman_great_fortress`**
and all in that preset's two *unscaled* outer lists. Three values over one preset is a per-list property, and
three values cannot cover 360°. So the per-tree angle, if the game has one, is generated by the renderer and
its source data is UNKNOWN to us — which is what the line above already said, now proved rather than assumed.
(The `farm_tree` / `farm_shrub` names above are round 4's RE reading of the same `I32`; round 11 does not
confirm them, and a fortress map's outer tree lists are a plausible fit for "not the map's own planting".)

**Round 7 settled what the byte is (data, install test `every_scaled_tree_byte_lands_on_its_own_scale_band`).**
Every shipped preset map: **3,844,367 instances in 101 `TREE_LIST`s**, of which **39 are scaled** (449,611
instances, all with a non-zero byte) and **62 carry no byte at all** (every byte 0 -- the flag is false, or the
list is a v2 one). The scaled bytes are exactly **`63 ..= 253`**: no value in `1..=62`, none in `254..=255`.
1,151 bytes (0.26%) ask for a scale under `MIN_TREE_SCALE` and 8,174 (1.82%) for one over `MAX_TREE_SCALE`.

- **It is a relative scale, not a size (CONFIRMED):** within one map, shrubs and trees draw from the *same*
  byte span (e.g. `hb_dresden`, lowbrush_shrub `63..177` mean 119.0 and elm_tree `63..177` mean 120.9) although
  their `.spt` boxes differ by 10x (`AmericanBoxwood_SHRUB` 1.80 m tall, `BlueSpruce_TREE` 18.2 m, from
  `data.tree_model`).
- **It is a random draw inside an artist-chosen band (CONFIRMED):** inside one species group of one map the
  bytes are spread ~uniformly over that group's own `[min, max]` (`hb_waterloo` common_alder_tree, n = 1064,
  `63..189`, 117 distinct, mode 127x56 against a flat ~8 elsewhere).
- **The decode is `scale = u8 / 128`, clamped to `MIN_TREE_SCALE .. MAX_TREE_SCALE` (INFERRED).** That
  reading puts the two clamps at byte 64 (0.500) and byte 179 (1.398), which is exactly where the shipped
  density collapses: 174 is a local peak (6,494), 175 a hole (637), 176-177 recover (3,042 / 2,519), 178-179
  fall again (649 / 658) and everything from 180 up is a thin decaying tail. Symmetrically the byte floor is
  63 -- one below the 0.5 clamp -- and 0.5 + `u8`·0.9/255 would make byte 64 (0.726) and 65 (0.729) far more
  common than 63 (0.722), which the data does not show.
- **The competing decode cannot be excluded by data:** `MIN_TREE_SCALE + u8 · (MAX_TREE_SCALE - MIN_TREE_SCALE) / 255`
  maps the same bytes onto 0.72..1.39 and also never clamps. The shipped values happen to fit both.
  Code: `TreeInstance::scale` (`ntw_formats::battle_terrain`), used by `napoleon::terrain::{trees, speedtree}`.
- **Three RE negatives (why the divisor is not an immediate).** No `1/255` (`0x3B808081`), `1/128` (`0x3A000000`),
  `255.0f` (`0x437F0000`), `256.0f` (`0x43800000`), `0.9/255` (`0x3B44D673`, `0x3B674DB4`) or `0.9/128`
  (`0x3BE66666`) immediate anywhere in `0x00E00000..0x00F00000`, and `0x437F0000` / `0x43800000` do not appear
  anywhere in the exe outside the UI. The only `0.9f` (`0x3F666666`) in the vegetation code is three
  functions, and the one at `0x00EC5E20` passes it as the width of a `0x200 x 0x200` rectangle, not a scale.
  The exe therefore computes the divisor from the two tweak floats at run time and leaves no constant.
- **The `I32` (round 7).** 0 in every scaled list and in 3,844,036 unscaled instances; **1 on 256 instances and
  2 on 135**, all in unscaled lists (round 4's "always 0 in the files checked" is narrowed, not refuted).

### 4.2 Tweaks (CONFIRMED values)
`tree_near_distance` 30, `tree_wind_response` 0.6, `tree_wind_response_limit` 1.0 (`VegetationMeshManager.cpp`),
`wind_tree_gust_low` 0.17, `wind_tree_gust_high` 2.0 (`VegetationDisplay.cpp`), `wind_maximum` 400 (`Wind.cpp`).
The vegetation display update (`0x01249F70`) scales its gust timing by the battle wind level (1 → 10, 2 → 30,
3 → 100, else 1). The battle wind vector (length 0..1) also picks the audio `wind_level_0..4` (`0x00535B30`, thresholds
0.583, 0.687, 0.820, 0.916).

## 5. Terrain

### 5.1 Heightfield (`0x00ED11F0`)
Per level `height_map_<n>_settings.xml` (or `.settings`): defaults `scale` 30, `bias` 0, `normalize` off. With
`normalize`, the loader takes the samples' min and max (parallel reductions over the decoded floats) and rescales with
`scale / (max − min)` around `bias + min`. Our `sample / 65535 · scale + bias` is the same mapping when a level uses the
full 0..65535 range.
Survey (install test `every_battle_map_parses`): every level of every preset is `normalize`d, its samples start at
0, and they end at 65535 or (22 levels, e.g. `hb_austerlitz` levels 1 and 3) 65534. So the min term is zero and the
code divides by the level's own top sample (`Heightfield::span`): at most one sample step (≤ 1.5 cm) from `/ 65535`.

### 5.2 Default deployment: `groupformations.bin` (CONFIRMED format and rules)
Loaded at battle start (`0x00506C00` stage "Group Formations Table" → `0x0068E2D0`) and kept on the battle object
(`+0xF4`). Format: see `ntw_formats::group_formation` (template: name, f32 priority, u32 purpose bits, 3 × u32 least %
of artillery / cavalry / infantry, faction list, elements; element kinds 0 block, 1 relative, 2 spanning, 3 group).
The 26 shipped templates: 1 drag-out, 13 land deployment/drag (purpose 3 or 2), 2 column templates (purpose 4), 10 naval
(0x60); three are restricted to the Middle Eastern factions.

The default deployment of an army without file positions (`0x00549F70` → `0x005B9C10` → `0x005BAB70` →
`0x005BA640`):
1. **Template** (`0x006C66A0`, purpose 2): the purpose bits must contain 2 (0x10 = any), the unit count must be within
   the template's bounds (min = Σ element minimums until the first element with minimum 0, which resets it to 0;
   max = Σ maximums, unlimited at the first unlimited one; `0x0068E890`), the faction must be listed (or the list is
   empty), and the shares (`count × 100 / units`, integer) of artillery with guns, cavalry (cavalry, camels, mounted
   dragoons) and infantry (infantry, gunless crews, dismounted dragoons) must reach the template's minimums
   (`0x006EFD70`, category tests `0x0055AB90/0x0055ABF0/0x0055C1C0`). Score = template priority × assignment score;
   a failed assignment scores −1; the first highest wins; no candidate → template 0.
2. **Assignment** (`0x0069F3A0`): a unit's weight in an element is its class's first entry in the element's list
   (`0x006C31D0`; class ids = the alphabetical class list `0x00EED3E0`). An element's bid (`0x006C3020`) is −1 when
   full, `priority / (assigned + 1) × weight`, with a missing class taking 0.001 while the element is under its
   minimum (else −1). Each round every element names its best unit with urgency `best + (best − second)` (`2 × best`
   when there is no second), +1 under its minimum (`0x006B4300`); the most urgent element takes its unit and its bid is
   added to the score. A unit nobody bids for fails the template; an element left under its minimum × 0.01. A
   template with a single element takes everything at its priority.
3. **Layout** (`0x006A7C10`): line elements put units side by side from the left, `spacing` apart, fronts level
   (`0x0067BE80`); columns stack them from the front (`0x0067BFF0`); blocks sit at `pos` (`0x006DFEF0`); relatives sit
   beside or behind their anchor with the offset as the gap, fronts or centres level (`0x006DFF40`, empty anchors
   skipped `0x006BFFD0`); groups are the union of their members (INFERRED).
4. **Placement** (`0x005DFC40`): the group faces the area's direction, its origin on the area centre, pulled back from
   1 m in 0.5 m steps until every unit is inside the area, at most the area depth minus the group depth (the area
   field read as the half depth is INFERRED); the best pull-back otherwise. Units flagged `+0x1C5` form a second group
   with the same template, placed 25 m ahead of the centre and pulled back the same way. The flag is
   `unit_stats_land` #88 (record `+0x1A5`, BATTLE_FIDELITY §5 offsets), set on the 23 Spanish guerrilla units and the
   French contra-guerrillas: guerrilla deployment (CONFIRMED). Done in `deploy_with_templates`.

Code: `ntw_formats::group_formation::{choose, assign, layout}`, `napoleon::battle::setup::deploy_with_templates`
(test armies), unit tests and the install test `a_line_army_gets_a_line_template`. For the AI (§6, `ntw_ai`): the
same templates serve group moves (purpose 1 drag-out, 4 columns); the AI would call `choose`/`assign`/`layout` with
its purpose bits.

## 6. Left for the shader work (§2, not done here)
The unit and terrain lighting model: the soldier vertex declaration / skinning shader and its normal/gloss use
(ANIM §7.1), the colour-mask combine (ANIM §7.4), horse normal/gloss maps, the battle terrain technique with its
detail/blend/cliff maps (BATTLE_TERRAIN §8), tree normal-mapped lighting.

## 9. Round 4 (taken over by the fidelity-battle worker), time-boxed leads
### 9.1 The trained-idle test (§1.8): training level enum found, the test itself still UNKNOWN
- **Training level enum (CONFIRMED).** The parser is `0x00EED960` and the printer `0x00EEC540`:
  - 0 `mob`, 1 `rabble`, 2 `poorly_trained`, 3 `trained`, 4 `well_trained`, 5 `elite`; 6 for an unknown name.
  - The unit-stats record builder stores it at record `+0x3C`, from builder `+0x1B0` (`0x00E8F3C4`).
- **Consequence.** If the `+0x1B0` test in `0x006631A0` were this level, its "0 or 1" would give the
  `STAND_TRAINED_IDLE` set to *mob and rabble*, the opposite of what the clip name says. So the test is probably not
  the training level (UNKNOWN). The getter was not found:
  - the soldier vtables' slot `+0xA8` is `0x0066B2F0` in every class checked, and that is an update function, not a
    getter;
  - no small function returning record `+0x3C` matched.

  Nothing ported.
- Unit vtable for later work: the unit constructor `0x0051B7D0` writes `0x01321298`, with sub-vtables at `+0xC`
  (`0x013212A4`) and `+0x10` (`0x013212EC`).

### 9.4 Round 5 (0-D sandbox): the `+0xA8`/`+0x1B0` chain narrowed (getter itself still UNKNOWN, no code changed)
Read-only runs, scratch `target/tmp/a_..e_*.txt` (out of git):
- **Call chain (CONFIRMED, `0x006631A0` decomp):** the function calls the virtual at `+0xA8` on `this` with
  no args, getting a parent object; then, only when its stance argument `param_1` is 0 (stance 0), it calls
  the virtual at `+0x1B0` on that parent and, when that returns 0 or 1, uses stance 4 instead. So
  `this` = the soldier, `+0xA8` = soldier-to-parent getter, `+0x1B0` = virtual on the PARENT returning 0/1
  for the trained idle set. `param_1` 0/4 index the `0x014520B0..` stance table (0 `STAND_IDLE_1..11`, 4
  `STAND_TRAINED_IDLE_1..6`); dword `0x76` of `this` (`+0x1D8`) = current stance, dword `0x144` = pending
  stance.
- **Soldier setup vtable `0x01333714` (CONFIRMED):** installed by `0x0061CB90 @0061CC67`, re-installed by
  `0x00623960 @0062396C`; its slot `+0xA8` (42) is `0x00461180` = `return 0` (decompiled). So at setup the
  parent is NULL and the trained branch is dead. Its slots 26/27 are `0x0066B2F0`/`0x0066B200`.
- **`0x0133AF38` is NOT the battle soldier vtable (CONFIRMED negative):** its slot 42 is `0x0066B2F0`
  (void update, size 552, decompiled — returns nothing usable); running `0x006631A0` on such an object would
  call `(*garbage + 0x1B0)()`. And no instruction in `0x005B0000..0x00680000` installs `0x0133AF38`
  (`scal:0x133af38`, empty). Prior "soldier vtable" assumption for `0x0133AF38` withdrawn for this path.
- **Battle unit `0x01321298` ruled out on both ends (CONFIRMED):** ctor `0x0051B7D0` writes it at `+0x0`
  (subs `0x013212A4`/`0x013212EC` at `+0xC`/`+0x10`); full 130-slot dump shows slot 42 (`+0xA8`) AND slot 108
  (`+0x1B0`) both = `0x005289E0`, decompiled as a void cleanup-like function (conditional `0x00FF7EF0`,
  always `0x0051EDB0`, conditional `0x0126E016`). The battle unit is neither `this` nor the parent here.
- **A `+0x1B0` byte FIELD on the parent (CONFIRMED, `0x00664A80`, single caller `0x006123C0` size 62):**
  walks `this[+0x28]` soldiers via `this[+0x2C]` and returns 1 if any `soldier[+0x1EC]->[+0x1B0] == 0`
  (byte). Field and vtable slot share only the number; the virtual getter's class is still UNKNOWN.
- **Next lead:** `ins:`-enumerate `CALL dword ptr [..+0xA8]` sites in battle code and decompile the 10
  remaining callers of `0x006631A0` (seen: `005be710 0061cb90 00679300 005f0170 005be5d0 00652470 00662e70
  0062a1f0` + 5) to find a `this` whose vtable holds a real `+0xA8` getter, then dump that vtable's
  `+0x1B0` slot.

### 9.5 Round 6 (0-D sandbox follow-up): real `+0xA8` getter FOUND, parent class still UNKNOWN (no code changed)
Read-only runs F..M on the `target/tmp/NR-s1-ghidra-copy` project (scratch `target/tmp/f_..m_*.txt`, out of git).
Nothing reached CONFIRMED for code; notes only.
- **Full `0x006631A0` caller list (CONFIRMED, `xref:0x006631A0`, run F):** 13 code sites:
  `0061cb90 005be5d0 005be710 005effe0 005f0170 00679300 0062a1f0 0065d940 00649480 00652470 00660ab0
  006543d0 00662e70` (the script's `callers:` header shows only the first 8 of 13; the 5 missed were
  `005effe0 0065d940 00649480 00660ab0 006543d0`). Plus 3 call sites in unanalysed thunk code at
  `00809ea0/920/9b0` (0x80 apart, `ptab` shows raw code bytes — a thunk table, not data; parked).
- **The 10 unseen callers (decompiled, run G):** `005be5d0`/`005effe0` (siblings: `(*param+0x90)()` fetch,
  palette stance 0 / 0x1D); `005f0170` (stance 0x17 path, also calls order→stance `0x00663730`);
  `0062a1f0` (stance = `0x00643900()` unless 0x49); `0065d940` (order `+0x1D8` in 0x3E..0x40 → stance 10);
  `00649480` (stance = `[+0x4FC]`); `00652470` (forwarded stance); `00660ab0` (soldier removal from the
  parent's `(+0x28 → +8 → +0xB0)` soldier lists at `+0x1D0/+0x1D4` and `+0x1BC/+0x1C0`, then reset
  `0x006631A0(0,1,1)`); `006543d0` (order-gated palette + `0x00663730`, heavy float path);
  `00662e70` (size 8 bare tail-call thunk to `0x006631A0`, 33 callers — likely a vtable slot alias).
- **`CALL [reg+0xA8]` sites in `0x005B0000..0x00680000` (CONFIRMED, `ins:`, run F):** `005d7dc0`,
  `005d85d0` ×3, `005d8e50`, `005dfa90`, 2 in no-function code (`005fb99d/53`), `00604a40` (2-arg call —
  different class/slot use, not the getter), `0064f6c0/710/790/8d0` (+1 in no-function `0064fa82`),
  `00655250`, `006631a0` itself, `0066ce00` ×4. The `005d*`/`0064*` sites are unexamined.
- **Live-soldier chain in `0x0066CE00` (CONFIRMED, `lst:`, run G):** `parent = soldier->+0xA8()` (×4:
  `0066d638/46/5f/78`); `parent->+0x1B0()` tested against **5, then 7, then 6** — any match (after the
  `[+0x1F0]→+0xADC→0x0055C2E0` check) sets `[soldier+0x61C] = 1`, else 0. Guard: `[+0x28]→+0x8→+0xB0`
  via `0x0055AC60`.
- **Second chain in `0x00655250` (CONFIRMED, `lst:`, run G):** `[EBX+0x518]` → `0x006113D0` → `EBX->+0xA8()`
  → `parent->+0x1B0()` must be nonzero AND `== 1` → AL = 1.
- **The real `+0xA8` getter (CONFIRMED, run K):** battle soldier vtable `0x0132BE3C` slot 42 (`+0xA8`) is
  `0x006A7DD0` (size 73, virtual-only — zero direct callers): returns the first non-null
  `(*sub+0xA8)()` over the sub-object pointer array at `this[+0x670]` (count `this[+0x66C]`), else 0.
  The ultimate parent (whose vtable carries the `+0x1B0` getter) is one chain hop further down — UNKNOWN.
- **Soldier vtable lifecycle (CONFIRMED, runs J..M):** setup `0x01333714` (`0x0061CB90 @0061CC67`;
  `xref` proves only two install sites exe-wide, the other being the reset below) → intermediate
  `0x0132B7D8` (`0x0059EC50 @0059EC7E`, also `0x0068C750 @0068C78B`) → battle `0x0132BE3C`
  (activator `0x0058EF40 @0058EFC0` ← `0x0059EAB0` ← `0x005A38C0`/`0x005A1EB0`; `0x0058EF40` calls
  `0x0059EC50` → `0x0061CB90`, then overwrites with the battle vtable + same sub-vtables
  `0x0132B994/0132B998` and slots `0x3D/0x6B = 0x0132C00C/0x0130BD4C`). Teardown `0x005A8140`
  (installs `0x0132BE3C` with the same slots) → reset `0x00623960` (7 callers) → back to `0x01333714`
  (+ slots `0x3D/0x6B = 0x013338C4/0x01333918`, clears `+0x5AB/+0x16B/+0x35B/+0xD7/+0xB1`).
  Intermediate slot 42 is the SAME `0x006A7DD0` (`dw:0x0132B880`, run M) — the parent chain is live
  from the intermediate state on, dead (returns 0) only under the setup/reset vtable.
  (Caveat: the run-H dump base `0x0132BE34` overshoots the true base by 2 slots — the run-C `vt:`
  walk-back artifact. True slot N = dump index N+2. The soldier's OWN slot 108 (`+0x1B0`) is
  `0x005EFFE0` — a palette-fill caller, NOT the parent getter; do not conflate.)
- **Withdrawn false leads:** `0x0132B74C`/`0x0132C270` have zero references (walk-back artifacts; their
  slot-42s `0x006386E0`/`0x006387D0` are not soldier slots); `0x0132B5D8` slot 42 is `0x005DC8D0`
  (returns constant 1 — not a parent getter); `0x00604AAF` is a 2-arg call on another class.
- **Value semantics (UNKNOWN; INFERRED against `unit_stats_land` #41):** observed `+0x1B0` tests are
  `{0,1}` (trained idle, stance 0 only), `{5,7,6}` (the `+0x61C` flag), and nonzero-then-`==1`.
  Values 6/7 exceed the round-4 training enum (0..5), and 0/1 would be mob/rabble — the opposite of
  "trained". So the getter does NOT return the #41 training level; the enum (≥8 values, or a bitmask
  with bit0 = trained-ish) is unidentified. §1.8's training-level INFERRED stands counter-evidenced.
- **Next lead:** find the WRITER of the `+0x670` array (who appends the sub-objects — the `0x005B2CA0`
  family at `0x005B8B40/B0/D200/D3A0` only reads/positions entries via `+0xD8/+0x4`; the appender is
  elsewhere, likely mount/attachment or squad-assembly code), identify one entry's class, dump ITS
  vtable slot 108 (`+0x1B0`), and decompile that getter. Then map the 0/1/5/6/7 values.

### 9.2 Tree scale byte (§4.1): settled in round 7 by data — see §4.1
- The tweaks `MIN_TREE_SCALE` (0.5) and `MAX_TREE_SCALE` (1.4) are tweak objects `0x0164CD90` / `0x0164CD28`.
  `0x00EC33A0` copies them into the vegetation picker at `+0x4C` (min) and `+0x48` (max).
- Round 7 closed it from the data instead (§4.1): a relative scale, `u8 / 128` clamped to 0.5 ..= 1.4
  (decode INFERRED, three RE negatives kept).

### 9.3 Flag cloth attachment (§3): files, geometry and bone settled in round 7 — see §3
- `ntw_formats::verlet` reads every shipped `.logic` file; install test
  `the_standard_bearers_flag_matches_its_pole` checks the mast, the sail and the pole they share a frame with.
- Still UNKNOWN: the transform the exe applies to the verlet item, and whether the sail hangs or streams.

### 9.6 Round 7 (0-D sandbox, data first): the `+0x1B0` value set is closed, the training level is refuted
Read-only Ghidra runs on this worker's own project copy `%USERPROFILE%\Documents\NR-sb-0d-ghidra`
(scratch `target/tmp/0d_g/run1.txt` .. `run11.txt`, out of git), runner
`analysis/fidelity/run_ghidra_0d.ps1`. Data probes in `crates/ntw_data/examples/unit0d_probe.rs`.
- **CONFIRMED, the whole `+0x1B0` call list** (`scal:0x1B0:0x00400000:0x02000000`): 36 `CALL dword ptr [reg+0x1b0]`
  sites exe-wide, 20 of them in battle code `0x005B0000..0x00680000`, in 15 functions:
  `0x0058EDD0`, `0x005D7DC0`, `0x005D85D0` (x5), `0x005D8E50`, `0x005DFA90`, `0x00611BE0`, `0x0064F6C0`,
  `0x0064F710`, `0x0064F790`, `0x0064F8D0`, `0x00655250` (x2), `0x00655B20`, `0x006631A0`, `0x0066CE00` (x3),
  `0x006A6FDF`. So the value set is closed and it is a small integer.
- **CONFIRMED, what each site tests.** `0x0064F6C0` = `p = this->+0xA8(); p && p->+0x1B0() == 2`;
  `0x0064F710` = `p->+0x1B0() == 1 && FUN_0055AC40()`; `0x0058EDD0` collects every unit whose sub-object's
  `+0xA8()->+0x1B0()` is `6` into a growable array; `0x005D85D0` builds **two** arrays, one for `+0x1B0` in
  `{5,6,7}` (and a check that dword `0x7A` of an object pointer, plus `0x18`, is 0) and one for the rest,
  choosing with a branch-free value of 2 when `p->+0x1B0()` is not 7 and 0 when it is 7; `0x00655250` needs non-zero **and** `== 1`; `0x006631A0` needs
  `{0,1}`. So: `{0,1}` -> trained idle, `2` -> a separate predicate, `{5,6,7}` -> two buckets with 7 singled out.
- **CONFIRMED, the class carries both getters.** `0x006A7DD0` (soldier vtable `0x0132BE3C` slot 42, i.e. `+0xA8`)
  returns the first non-null `(*sub->+0xA8)()` over the array at `this+0x670` (count `+0x66C`); `0x0065B0A0`
  and `0x0065B0E0` reach the same array through `+0x98` / `+0x9C` and return its first entry, so the array is a
  generic child list. Round 6's next lead — who appends to `+0x670` — has **no writer in
  `0x005B0000..0x00680000`** (only `MOV [reg+0x670],0` clears at `0x0068C7D2`); the append is elsewhere.
- **CONFIRMED negative, no name enum in the exe.** `nwbytes:well_trained`, `drill_set_infantry_line`,
  `foot_bayonet`, `melee_animation_category`, `infantry_line`, `man_musket` and `rider_musket_sabre` are **all
  absent from `Napoleon.exe`**. So `+0x1B0` is not a direct index into any of those DB name lists.
- **CONFIRMED from data, `man_animation_type` is not it.** All 328 `unit_stats_land` rows resolve **both**
  `STAND_TRAINED` and `STAND` in their men's animation table, and `man_musket` (113 units) alone serves
  `infantry_elite`, `infantry_grenadiers`, `infantry_irregulars`, `infantry_light`, `infantry_line`,
  `infantry_militia`, `infantry_mob` and `infantry_skirmishers` — one table cannot split them.
  `training_level` has 6 values (round 4) and cannot produce the observed 6 and 7.
- **The two survivors (UNKNOWN which).** Columns with 7 shipped values, i.e. an exe enum of 0..6 plus an
  "unknown" 7: `drill_set` (artillery, cavalry, infantry_grenadiers, infantry_light, infantry_line,
  infantry_melee, infantry_mob — probe `enum`) and `melee_animation_category` (foot_bayonet, foot_rifle_butt,
  foot_sword, mounted_lance, mounted_slash, mounted_sword, one_handed — probe `enum`).
- **CONFIRMED, the exe's own words for the two families.** The shipped fragments head the second set
  `// MOVEMENT UNTRAINED` / `// WALK IRREGULAR` (`standard_bearer_fragment.txt`); `musket_fragment.txt` has 87
  `_TRAINED` slot lines against 97 `IRREGULAR` comments, `musket_sabre` 86/97, `pitchfork` 81/186,
  `swordsman_movement` 85/61, `axe` 81/80, `sword_and_shield_movement` 52/47, `pike` 39/1. So `_TRAINED` is the
  drilled set and the plain one the irregular set — the naming is the original's, not ours.

### 9.7 Round 7 (0-D sandbox): the flag's pole, bone and frame — see §3
- **CONFIRMED (data).** `rigid_equip_euro_flagpole01` in `unitmodels\euro_equipment.variant_weighted_mesh` is
  attachment 1 of 134: 140 vertices, 504 indices, vertex size 20, textures `equip_diffuse` / `equip_normal` /
  `equip_gloss_map`, an `unknown` field of 3, and a bounding box of x `-1.020 .. 3.379`, y `-0.038 .. 0.038`,
  z `-0.046 .. 0.040` — a 4.40 m rod along +x whose origin is 1.02 m up the pole.
- **CONFIRMED (data).** The standard bearer's variant (`britain_inf_line_british_foot.standard_bearer.unit_variant`)
  lists it as the single mesh of category `equipment_personal`, kind 1; `EquipmentLibrary` has no piece by that
  name, so it resolves through the weighted mesh. `warscape_equipment_themes.euro_standard_bearer` =
  `("euro_standard_bearer", "euro_hanger", None, false, None, "euro_flagpole")` and
  `warscape_equipment_items` maps item `euro_flagpole` -> set `euro_flagpole`: the pole is the bearer's personal
  equipment.
- **CONFIRMED (data).** Both equipment containers bind `rigid_equip_euro_flagpole_lod1` and `_lod2` to **bone 3**;
  the other flag-adjacent pieces are bone 1 (muskets), bone 2 (swords / hangers), bone 13 (backpacks),
  bone 29 (bugles) and bone 30 (drumsticks).
- **CONFIRMED (data).** The FLAG_BEARER skeleton has 41 bones; bones 1, 2 and 3 are `Weapon1`, `Weapon2`,
  `Weapon3` and are **parentless roots**. At frame 0 of `FLA_StandT.anim` (20 fps, 4.700 s, 95 frames) `Weapon3`'s
  origin is `(-0.001, 0.400, 0.402)` and its x axis is `(0.004, 1.000, -0.002)`, so bone 3's local +x is world up.
- **CONFIRMED (data).** `mast_rigid_euro_flagpoleA_01`'s eight particles are `top (2.067, -0.986, -0.093)`,
  `bottom (1.021, -0.955, -0.100)` and 1..6 at y `-0.967`, x `1.027 .. 2.183`; the sail's 42 particles are all at
  y `-0.967` and reach z `1.572`; the six `auto_<n>` ropes pin cloth particles 1, 2, 4, 5, 9, 10 — the `u ~ 0`
  column — to mast particles 1..6, and the 101 `srope<n>` ropes are all `invisible`.
- **INFERRED.** The verlet item's frame is the pole's frame translated by `(0, -0.967, -0.087)`, so the exe
  places it 0.967 m from the pole's own origin along the pole frame's y.
- **UNKNOWN.** Which bone index the exe feeds the verlet item, and therefore the exact offset. The data cannot
  separate "the item's origin is the bone origin and the sail flies 0.967 m sideways" from "the exe offsets the
  item to put the sail on the pole".
- **RESOLVED in round 8 (CONFIRMED, from the data alone).** The pole's own bone **is** the verlet item's frame,
  offset by `(0, -0.967, -0.075)`: the six *numbered* mast particles 1..6 are exactly colinear at `y = -0.967`
  and `z = -0.075` with `x` at a constant 0.231 m spacing from 1.027 to 2.183, which is inside the drawn pole's
  own bone-local span `-1.0195 .. 3.3793` (probe `flagcloth`). The `top`/`bottom` markers are the rod's tilted
  ends (y `-0.986` / `-0.955`, z `-0.093` / `-0.100`), which is why round 7 read the offset as z `-0.087` from
  the bounding box; **the exact offset is `(0, -0.967, -0.075)`** and it is now `cloth::FRAME_OFFSET`.
- **RESOLVED in round 8 (CONFIRMED, data).** The six `auto_` ropes have **rest length exactly 0.0000 m**: each
  pinned cloth particle sits *on* its mast particle. So the hoist is **welded**, not sprung — no stiffness,
  no tolerance, no spring constant to guess (`flag_install::the_shipped_flag_cloth_solves` prints `rest 0.0000`
  for all six).
- **The drawn result** (`cloth::bone3_at_ground()` = the exe's own `Weapon3` frame-0 matrix, and
  `flag_install::the_shipped_flag_cloth_solves`): the authored sail occupies world **y 1.422 .. 2.579**, i.e. the
  hoist edge is **1.43 m to 2.58 m above the bearer's feet**, and the pole runs from 0.62 m below the feet to
  3.78 m above them. The file's flat-in-y sheet becomes a **vertical** flag because the bone's +x is world up;
  the file's 1.156 m along the pole becomes the flag's height and its 1.647 m of `z` spread becomes the flag's
  width. See §3.1 for the atlas and §3.2 for the solve.

### 9.8 Round 8 (0-D sandbox): the `+0x1B0` enum through its **consumers** — CLOSED-ATTEMPTED
Round 7 read two of the five tiny adjacent predicates and stopped. Decompiling the whole family
(`target/tmp/0d_g/run12.txt`, targets `target/tmp/0d_g/t12.txt`, this worker's own read-only project copy)
settles the *value set* and the *shape* of the use, and still does not name the enum.

**What the family is.** Five 31-to-57-byte predicates in `0x0064F6C0 .. 0x0064F8D0`, all reading the **parent** unit's
`+0x1B0` through the `+0xA8` getter:

| function | test | extra gate | callers |
|---|---|---|---|
| `0x0064F8D0` | parent value **== 0** | — | `0x00653A10`, `0x0064A520`, `0x0064F770` |
| `0x0064F710` | parent value **== 1** | `this+0x1F0 != 0` and `FUN_0055AC40()` | `0x00653A10`, `0x0064A520`, `0x008125E0`, `0x006A6400` |
| `0x0064F6C0` | parent value **== 2** | — | `0x00653A10`, `0x0064F750` |
| `0x0064F750` | `0x0064F6C0()` (so == 2) | `this+0x1F8 != 0` and `FUN_006CFCC0()` | `0x008125E0`, `0x006A6400` |
| `0x0064F790` | parent value **== 3** | `this+0x1F8 != 0` and `FUN_006CFCC0()` | `0x00653A10`, `0x008125E0`, `0x006A6400` |

- **CORRECTION to round 7, CONFIRMED: the value set is `{0,1,2,3,5,6,7}`, not `{0,1,2,5,6,7}`.** `0x0064F790`
  tests **3** and round 7 never saw it (it read `0x0064F6C0` and `0x0064F710` and assumed the neighbours were
  the same two tests). **4 does not occur**, and neither does anything above 7.
- **CONFIRMED: `FUN_0055AC40` is the reciprocal-parent gate** (41 callers, so this is a general mechanism, not a
  local one). It reads `this->+0x1EC`, returns the **parent's `+0x1B0` as a byte** only when
  `parent->+0x214 == this` — a **back link** — and returns 0 otherwise. So a soldier's parent is only believed
  when the parent claims him, which is why `+0x1B0` is reachable through `+0xA8` at all.
- **CONFIRMED, the shape of the use:** `+0x1B0` is a small **class code on the unit**, consulted by a soldier's
  own capability predicates, each of which also needs a per-soldier pointer (`+0x1F0` for value 1, `+0x1F8` for
  values 2 and 3) and a shared global gate (`0x0055AC40` for value 1, `0x006CFCC0` for 2 and 3). It is **not** an
  index into a per-unit table and **not** a strength, range or morale value.
- The `{5,6,7}` consumers are a different mechanism and stay as round 7 found them: `0x0058EDD0` pushes each
  **sub-unit** that contains a soldier whose value is **6** onto a growable array, and `0x005D85D0` gathers every
  soldier in `{5,6,7}` into records whose bucket is `1` if the *parent's* value is 5, `0` if it is 7 and `2`
  otherwise. `0x005D85D0` also contains the string `"indian"`, which is a debug/assert path and not a name.

**Why it still does not name the enum, stated plainly.** The two data candidates that survived round 7 are
`unit_stats_land` `drill_set` and `melee_animation_category`, each 7 values plus an "unknown" 7. The new set
`{0,1,2,3,5,6,7}` fits neither better than the old one: both columns are 0..6 with every value used by some
shipped unit, so a *hole* at 4 is unexplained under either. The five predicates name *behaviours the exe asks
about*, not a category, and no string in the exe names any of them (round 7's `nwbytes` negatives stand and were
not re-run). **Verdict: CLOSED-ATTEMPTED. No code changed; `_TRAINED` stays preferred (PROVISIONAL).**
To reopen it, the productive lead is no longer the name but the **writer** of the value — the DB column whose
row produces a hole at 4 — and, failing that, `FUN_006CFCC0` (the gate shared by the value-2 and value-3
predicates, called from only three places) and `FUN_005B27C0` / `FUN_00531E40` in `0x00653A10`'s callee list.

## 10. Mounted and riding: the two unit predicates (2026-10-16, horsefright)

The exe has two "is mounted" tests on a battle unit, both already named and plated in Ghidra (no renames
this round; read-only static trace):

- **`0x0055ABF0` IsUnitMounted** (CONFIRMED static): category cavalry (0) or camels (5) → true;
  dragoons (3) → true unless ability 15 (`dismount`) is active; everything else false. ~93 call sites.
  Ours: `ntw_sim::battle::strength::mounted`.
- **`0x0055C2A0` IsUnitRidingAndNotDismounted** (CONFIRMED static): IsUnitMounted and `dismount` not
  active, for every category. It differs from the first only for cavalry or camels with `dismount`
  active. 11 callers. Ours: `strength::riding`.
- A third, **`0x0055C1C0`** "on foot": infantry (2), or artillery (1) whose guns object (`+0x1F4 → +0x10`)
  is 0, or `dismount` active. Not in ours yet.

Our callers, each mapped to the exe caller (address = the call site), one function per predicate:

| Ours | Exe caller | Predicate |
|---|---|---|
| frightened horses, `Battle::morale_inputs` (model.rs) | `0x0053B970` at `0x0053BA0F` | riding |
| new-attacker adjust, `morale::modifiers` (attacker / self riding, from model.rs) | `0x0053C720` at `0x0053C82B`, `0x0053C836`, `0x0053C85E` | riding |
| fighting cavalry, `morale::modifiers` (self / attacker riding, from model.rs) | `0x0053C720` at `0x0053C8AB`, `0x0053C8B6` | riding |
| firing drill `effective_drill` (shooting.rs) | `0x00554920` at `0x00554944` | riding |
| ground column `strength::movement_class` (model.rs unit speed, march.rs men) | `0x006543D0` at `0x006544AB` | riding |
| all-men volley `volley_plan` (shooting.rs) | `0x005694D0` at `0x005694E7` | mounted |
| in-place check (march.rs) | `0x00557BC0` at `0x00557BCA` | mounted |
| neighbour weight halving (model.rs) | `0x0053CBD0` at `0x0053CE77` | mounted |
| AI strength block (ntw_ai battle/mod.rs) | `0x007CE410` → `0x007A4EB0` at `0x007A4EDE` | mounted |
| AI enemy composition (ntw_ai battle/mod.rs) | `0x007536C0` at `0x0075376F` | mounted |
| AI wing filters (ntw_ai battle/mod.rs) | `0x007D33B0` / `0x007D34A0` | mounted |

The ground column (`0x006543D0`): artillery with guns (`0x0055AB90`) takes column 0 or 1 by class; else a
mounted unit takes column 2 while riding, column 3 when not; else an on-foot unit (`0x0055C1C0`) column 3;
else no modifier. Ours now reads the column per tick (`strength::movement_class`), so dismounted cavalry
walks on the infantry column; before, the static setup class kept it on the riders' column.

`ntw_ai::battle::classes::is_mounted` (by category code, dragoons always mounted) was a second copy of
`0x0055ABF0`; it is gone and the AI calls `strength::mounted`, so dismounted dragoons now count as on foot
there too, as in the exe.

Left INFERRED, each with a reason in the code:
- AI missile class factor `0x007D2CF0`, skirmisher arm (class 0x16): the exe tests `0x0055C1C0` (on foot)
  on the attacker; ours tests not mounted, the same for infantry-category skirmishers.
- AI wing bidder's mounted share (`+0x78` / `+0x84`): its builder is not traced; read as mounted.
- Defence contact (`abilities.rs`, chevaux de frise / stakes `0x006E94E0` / `0x006E9160`): the exe asks
  each soldier (vfunc `+0xF0`), not either unit predicate; ours keeps the unit-level mounted test.

### 10.1 Morale attack pass `0x0053C720` and fighting cavalry (2026-10-16, fightcav)

Traced from the disassembly (CONFIRMED static); named `BattleUnitMoraleComponent::ApplyMoraleAttackAndCasualtyModifiers` in Ghidra (V5, completeness 77), with `BattleUnit` fields `nTrainingLevel` +0x194, the casualty ratios +0xCB4/+0xCBC/+0xCC0/+0xCC4 and `nMoraleCategory` +0xD08. `0x0053C720` is a sub-evaluator of the full morale
evaluation `0x00584020` (its only caller, at `0x00584099`, after the suppress flag `+0x51` is cleared). Its
`this` is the morale component (unit `+0xC00`; `+0x4` the unit, `+0x6C` the kv_morale block). Order:

1. Category `+0xD08` (jump table `0x0053CBA8`): effect 5 (+6/+4/+2) or 0x17 (−4/−8), unchanged.
2. For each entry of the attacker list (unit `+0xADC → +0x74C`; count `0x004CE450`, entry `0x0054EE00`,
   the entry is the attacking unit): direction `0x0057F310` (0 front, 1 flank, 2 rear; 0 when this unit
   has ability 1 or 0 active, or the centres are under 2 m apart). Flank sets a flank flag, rear a rear flag.
   Then the insert-unique `0x0054ED20` into the component's set (`+0xC` capacity, `+0x10` count, `+0x14`
   data) says whether the attacker was already seen:
   - **New**: v = `was_attacked_in_front` / `_flank` / `_rear` by its direction (kv slots `+0x15D0`,
     `+0x1630`, `+0x1690`), doubled when the attacker's `+0xADC` passes `0x0055AC20` (charging). If the
     attacker rides and this unit does not, v −4 at training level (`+0x194`) 0/1, −2 at 2; otherwise, if
     this unit rides, v +2. Effect 0x18 with v unless v = 0; the suppress flag `+0x51` is set either way.
   - **Already seen**: if this unit does not ride, the attacker rides, and this unit has neither ability 1
     (`square_formation`) nor 0 (`pike_square_formation`) active (`0x0055C9A0` on `+0xDAC`), the
     fighting-cavalry flag is set.
3. Flank flag: effect 0x19 with `was_attacked_in_flank`; rear flag: 0x1A with `was_attacked_in_rear` (both
   even at 0). Fighting-cavalry flag: kv `fighting_cavalry` (`+0x16F0`) at level 0/1 in full, at 2 halved,
   at 3 quartered, at 4/5 none, any other level in full; the halving and quartering are C division rounding
   toward zero (`0x0053C960`–`0x0053C979`: `CDQ`/`SUB`/`SAR 1`, `CDQ`/`AND 3`/`SAR 2`), so −5 gives −2 and
   −1. Effect 0x25 unless 0.
4. Casualty ladders and blood, unchanged, each skipped at a value of 0 (`TEST`/`JZ` before each add).

Ours (`morale::modifiers`): "riding" in both places is `strength::riding`, so a dismounted cavalry unit is
on foot here and riding cavalry it has seen gives it the fighting-cavalry effect (test
`dismounted_cavalry_fights_cavalry`, model.rs). Fixed against the old code: the per-attacker logic ran
for front attackers only with the front value; its three UNKNOWN inputs were the seen set, the charge test
and "self riding"; `fighting_cavalry` was always false; the level-2/3 scaling used an arithmetic shift
(−5 → −3 / −2) where the exe divides; zero values were added. The seen set is `MoraleComponent::
attackers_seen` (unit ids). INFERRED: it lasts the battle; the only free found is the component destructor
`0x00522690` (from `0x00522130`), and no writer clears `+0x10` in `0x0053A000`–`0x0053F000`,
`0x0054E000`–`0x0054F200`, `0x00580000`–`0x00587000` or the unit constructor `0x0051D0F0`. The attacker
list and the direction are ported in §10.2.

Found, not changed (one item each): the soldier-pair callers of `0x0055C2A0` (`0x0081D8E0`, `0x00820040`, `0x00821B90`, `0x00822940`,
`0x008261A0`, gated on type class 10), `0x00532320` (melee class factor gate in `0x007D23E0`) and
`0x0053EFE0` have no counterpart in ours yet.

### 10.2 The attacker list and the attack direction (2026-10-16, attackers)

Traced from the disassembly (CONFIRMED static unless tagged). Ported in `crates/ntw_sim/src/battle/attackers.rs`
(`AttackerList`, `Battle::fill_attackers`, `attack_direction`); `morale_inputs` (model.rs) reads the committed
entries. Ghidra names (V5, completeness 89 / 97 / 90): `BattleAttackerList::AddSoldierContactToAttackerList`
(`0x0052EDB0`), `BattleAttackerList::CommitAttackerList` (`0x00551A40`),
`BattleUnitMoraleComponent::GetAttackDirectionFromSquadBox` (`0x0057F310`). Ghidra types added: `BattleAttackerList`
(0x2034 bytes, embedded as `BattleSquad::attackerList` +0x74C, which ends at +0x2780 just before the known
`dwInPlaceTick` +0x2784), `BattleAttackerEntry` (0xC); `BattleSoldier` grown to 0x5F0 (0x660 since polish42, see "Ghidra types (polish38, polish42)") with `wBattleIndex` +0x2C,
`dwSideKey` +0x44, `flRadius` +0x58, `pUnit` +0x1F0, `bInEnemyContact` +0x5EC; `BattleUnit::pSquad` +0xADC;
`BattleSquad` box fields +0x644–+0x664.

**Layout.** The list lives at squad `+0x74C` (unit `+0xADC` → squad). `+0x4` capacity, `+0x8` count and `+0xC`
data of the list being filled; `+0x14` / `+0x18` / `+0x1C` the same for the committed list the morale pass
`0x0053C720` reads; `+0x20` a bitset with one bit per soldier index, cleared through a dirty list
(`0x0056EE80` records the index, `0x00542AA0` clears the bits). Entry (stride `0xC`): the enemy unit (soldier
`+0x1F0`), the soldiers in contact, an update counter.

**Add `0x0052EDB0`** (`this` = the list, args: own soldier, other soldier). Nothing when the other soldier has no
unit (`+0x1F0` = 0) or his bit (u16 index `+0x2C`, battle-wide) is already set. Otherwise the bit is set
**first**, then the friend test (soldier `+0x44` equal → nothing) and the distance test: squared ground
distance (`+0x48`, `+0x50`) at most (`r_own + r_other + 2.0`)², radii `+0x58`, 2.0 at `0x01325D14`
(`COMISS`/`JC` at `0x0052EE5F`). On a hit: own soldier `+0x5EC` = 1, then the other's unit's entry gains one
contact, or `{unit, 1, 0}` is appended (capacity doubles from 1). Callers: the soldier-pair collision
handler `0x0064CAD0` (a vfunc in 23 soldier vtables; it calls the add for both soldiers of the pair, then
at `0x0064CB06` compares `+0x44`: different → the melee path `0x00664100` / `0x00629240`, same → the
friendly push `0x00667260`), and the pair loop `0x0066E680` over a local group (from `0x0066C250`,
`0x0066C3B0`).
- Quirk (not called a bug, since nothing shows it is unintended): an enemy soldier is marked before the
  distance test, so the first pair the exe hands over decides for him in that update; a later pair with a
  closer man of ours does not count him. Ours tests every pair (APPROXIMATION, the model has no soldier
  collision pass); the two agree whenever the first pair is in reach.

**Commit `0x00551A40`** (`this` = the list). For every filled entry whose unit is also in the committed list, the
counter becomes the committed entry's counter + 1; the filled list is copied over the committed one (so a unit
that left contact drops out), the filling count is reset and the bitset cleared (`0x00542AA0`). Its only caller
is `SQUAD::update` `0x00580A30` at `0x00580B85`, inside the "squad has men" branch (`+0x18` test at
`0x00580A62`), after `UpdateSquadSummary` (`0x00580A7C`) and `0x00581E80`. Ours: `fill_attackers` in the
unit's update slot after `refresh_summary`, skipped without men; both buffers are kept, so no allocation per
tick once grown.

**Direction `0x0057F310`** (`this` = the defender's morale component, arg the attacker unit). 0 (front) when the
defender has ability 1 or 0 active (`0x0055C9A0` on `+0xDAC`), or when the squared distance between the two
summary centres (squad `+0x62C` summary centre, then `0x0080E9D0`) is under 4.0 (`0x01325D20`, `JA` at
`0x0057F3D9`). Otherwise, from the defender's box (summary `+0x18` = squad `+0x644` / `+0x648` centre,
`+0x650` width, `+0x654` depth, `+0x658`/`+0x65C` the right axis, `+0x660`/`+0x664` the front axis): the two
box diagonals through the centre split the plane into four wedges; the attacker is front when on the front
side of both (`>= 0`), rear when on the back side of both (`<= 0`), and flank otherwise (a NaN gives
flank). So the wedges follow the box's shape: a wide line has a wide front and rear and narrow flanks, a
column wide flanks. Ours `attack_direction`, in the same order of tests; tests for a line, a block, a column, a turned
heading, the square and the same-centre case.

**Removed from ours.** `morale_inputs` filtered out-of-fight and inactive enemies; the morale pass iterates every
committed entry with no filter, so the filter is gone.

INFERRED (debugger, SITTING_SHEET 3k/3l):
- soldier `+0x44` is our `side`. Its writer was not found statically (`0x00518230` only copies it), and
  whether it is the alliance or the army is open (allied armies on one side would differ if it is the army).
- where the soldier collision pass runs in the frame relative to `SQUAD::update`; ours fills at the unit's
  own update, on the men where they stand then.
- the seen set (§10.1) lasting the battle: a whole-program scan found no unit-relative `+0xC0C` / `+0xC10` /
  `+0xC14` access in the battle unit code besides the insert and the destructor.

Not ported: `0x0054F070` (the main attacker = the entry with the most contacts); no reader in ours yet.

**Reach radius (2026-10-22, reachradius; polish42 follow-up, CONFIRMED static).** The contact test reads each
soldier's own radius `+0x58`, never the cover radius `+0x620`: `MOVSS XMM2,[ECX+0x58]` (own soldier) at
`0x0052EE29`, `ADDSS XMM2,[EDI+0x58]` (other soldier) at `0x0052EE38`, then `+ 2.0` and the square. The in-area
test `0x005857A0` also reads `+0x58` (`0x00585800`). So a soldier whose `+0x620` is grown over attached entities
(`0x00607540`) keeps his contact reach. Ours had one field, `LandUnit::man_radius`, documented as `+0x620`, that
both the box and the contact read (the same value, since ours never grows it). Now split: `man_radius` is `+0x58`
(read by `fill_attackers` and `men_inside`), `cover_radius` is `+0x620` (read by `men_standing` →
`SummaryMan::radius`, the box's margin); `LandUnit::set_man_radius` sets both, as the copy at `0x0061D2D8`. The
growth stays PROVISIONAL on `cover_radius`. Test
`contact_and_area_tests_read_the_own_radius_not_the_cover_radius` (model.rs) fails when either test reads
`cover_radius`.

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
    - CONFIRMED negative: **the exe has no string enum** for `training_level`, `drill_set`,
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
     `group_formation_install`) and the original's default deployment (§5.2): template choice, greedy assignment,
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
  (INFERRED names from the slot names).
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
`+0x14C`) and return `rate = entity speed / clip speed`. The walk/run (`WALK_TRAINED_1..5`) code was not found, but
the clip files are authored at those speeds (`MUS_T_Walk_100/127/173`, `Jog_219/313`), so the same rule is INFERRED.
Code: `pick_level` per frame from the unit's speed over its last moving model tick(s) (`view::GroundSpeed`, the
model's equivalent of the entity speed `+0x14C`); the unit's animation clock runs at that rate. Until 2026-10-07 the
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

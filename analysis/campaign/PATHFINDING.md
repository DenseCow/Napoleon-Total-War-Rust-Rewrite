# Campaign pathfinding (BACKLOG §1, `pathfinding.esf` / `sea_grids.esf`)

Worker: pathfinding (branch `work/pathfinding`). Tags: CONFIRMED / INFERRED / UNKNOWN; our stand-ins
PLACEHOLDER / PROVISIONAL. Ghidra findings are written as specs; decompiled code is never stored.
Tooling: `analysis/campaign/ghidra_scripts/run_ghidra.ps1 <targets.txt> <out.c>` runs `PathDecomp.java`
read-only on the main project (outputs go to a scratch folder, never into the repo). `tc:0xF1,0xF2`
marks functions `__thiscall` in memory before decompiling (the decompile then shows each object).

## Where I am / what's next
- **Resumed 2026-10-04 (after the pause).** Done since: same-component pre-check (§1, ported), flag bits 24..31
  (§3, solved), rivers (§5), port/landing rules (§6, decoded, HANDED OVER to `work/pathfinding-ports`).
- **Smoothing (§7)** found and ported (structure CONFIRMED, details PROVISIONAL). Checks 2026-10-04 (head of
  work/pathfinding): workspace build + `cargo test --workspace` all pass; clippy adds no warnings; game run
  `--campaign eur_napoleon --campaign-demo [--campaign-demo-move --campaign-end-turn 2]`: smoothed path shown, army moves, no panic.
- **Zone of control (§8)** decoded and ported (2026-10-04); node budget (§9): none (CONFIRMED). Checks: workspace
  build + `cargo test --workspace` pass (the user-save tests skip: the folder only holds our NR saves now), clippy adds
  nothing; game run `--campaign eur_napoleon --campaign-demo --campaign-demo-zoc [--campaign-demo-move
  --campaign-end-turn 3]` (new harness flag: a target 18 units past the nearest enemy army): the path detours west
  round the Austrian zones in the Alps, the army moves, 3 AI turns, no panic.
- **Region gate in the path search (§8, pathgate 2026-10-20):** traced (CONFIRMED) and ported: armies and fleets
  path behind the zone flood's region flags (plus the target owner's regions), agents without; rebels PROVISIONAL.
- **Still open:** obstacle modes 3/4 and kinds 10/11 (core shape and fog-of-war rule: §10),
  fort and barrier obstacles (`FORT_OBSTACLE`, `BARRIER_OBSTACLE`: none in the shipped startposes), stealth/spotting for
  obstacles; the exact segment rule of flag bits 4..23 (INFERRED, §3); a drawn movement-range zone (not found; the
  path split at the remaining AP is CONFIRMED and ported).
- **Found (2026-10-03):** the original's campaign path search: an A* over (grid cell, polygon) nodes
  (`0x00AC62E0`, more template copies `0x00AC6BF0`/`0x00AC7500`/`0x00AC59F0`/`0x00AC54C0`; neighbour
  expansion `0x00B2CC20`; step cost `0x00B11570` → `0x00B0B260`; heuristic `0x00B45330`). The **cell header
  bytes are the per-direction terrain cost bytes**, the **road cost is per region from
  `road_level_N_action_point_cost`**, the `grid_data` u32 is the polygon count (CONFIRMED, §2). Path cost
  is in **turns** (distance x cost / the character's max action points).
- **Ported:** `ntw_sim::campaign::polypath` (the search, CONFIRMED rules; PROVISIONAL parts listed in its
  module docs and §4), built from `pathfinding.esf` by `ntw_campaign::pathing::poly_map` and stored in
  `PathGrid::poly`; `CampaignModel::plan_path` uses it for armies, agents and fleets. Tests: unit tests in
  `polypath.rs`; install tests `ntw_campaign/tests/polypath_install.rs` (all 5 maps) and
  `campaign_play.rs::armies_move_on_the_map_grid`.
- **Next / open:** path smoothing (UNKNOWN: the search returns cell centres; the code that walks a
  character along its path is not found yet, `0x009CE010` is the order-cursor check); landing/embarking and ports (special nodes, not in our simulation yet);
  rivers = kind 3 strips (§5); flag bits 24..31 = direction mask (§3, solved);
  the "same component" pre-check (§1); the node budget (pathfinder +0xE0).

## 1. Call chain (CONFIRMED structure)
- Character code `0x00923DC0` / `0x00923760` (move / attack checks) → `0x00B21510`, `0x00B22890`,
  `0x00B20B20`, `0x00B18B60`... → `0x00B18F40` / `0x00B5A080` → `0x00ACB160` (4-entry path cache at
  pathfinder +0xF8, entries 0x14C bytes; `0x00B147F0` returns the hit index or 4) → the A* `0x00AC62E0`.
- Move context (0x30 bytes, built by `0x00AEA2D0`): +4 initial cost = `1 - AP_left / AP_max` of the
  mover (the part of this turn already spent), +8 the naval mover's max AP, +0x14 the land mover's max
  AP, +0x18 a second spent fraction (used when landing), +0x1C the start location, +0x24 = max of the
  two max APs (the heuristic's divisor), +0x10 mover type remapped 0→6, 1→7, 3→9, 4→10. Callers pass
  the cost limit FLT_MAX (no limit).
- Location (32 bytes, `0x00AF3520`): +0 pointer to the cell's 8 header bytes, +4/+8 position (Fixed20),
  +0xC the polygon (boundary), +0x10 the grid, +0x14 packed cell (col u16, row u16), +0x18 u16 mover type,
  +0x1A u16 (0xF). Off a kind-7 polygon, mover 9/10/11 become 3/4/5.
- The reachability checks `0x00B17AC0` / `0x00B17DD0` (21 + 18 callers from the order code) first compare a
  u16 at +0x14 of the records found for the start and the goal (`0x00AA0FD0`: a lookup by map position;
  INFERRED a connected-component id) and give up without searching when they differ. Then they run
  search variants without the turn division (`0x00AC59F0`, `0x00AC54C0`: costs in map units).
- Mover types 0..11: naval = {2, 5, 8, 11} (`0x00B4DA70`); polygon kinds allowed per type: see
  CAMPAIGN_DATA.md §11 (types ≡ 0 mod 3: kinds 0, 6, 7; ≡ 1: 0, 6, 7, 8; ≡ 2: 1, 4, 7; kinds 10/11 always).

## 2. The search (CONFIRMED unless tagged)
- **Nodes** = locations: the start point, the goal point, and (cell, polygon) pairs positioned at the
  **cell centre** (`0x00B5B6C0`: origin + col x cell + cell/2). Goal test (`0x00B00730`): same cell, same
  polygon, and the same naval-ness.
- **Open list**: binary heap of 28-byte nodes {location, state 0 new / 1 closed / 2 fresh, parent, g pair,
  h pair}; ordered by **(g0 + h0, g1 + h1) lexicographically** (smallest first). A node already seen is
  re-opened when the new g is smaller (first component with 0.9999 / 1.0001 tolerance, then the second).
  The search stops after a node budget (pathfinder +0xE0) or when the goal is popped. Path = the parent
  chain reversed (`0x00807620`): start, cell centres..., goal.
- **Neighbours** (`0x00B2CC20`): for each of the 8 neighbour cells and the cell itself, every polygon of
  that cell whose kind the mover may enter and that **shares an outline edge** with the current polygon
  (`0x00B12CD0`: equal vertex pairs; the 4 cell corners map across the cell side). For a diagonal step the
  two polygons must have the same kind (`0x00B27EB0`; kind 2 never). Off the grid edge the neighbour is
  looked up in the next grid (several pathfinding areas). Kind 7 polygons with a port/settlement entity and
  embarking (from mover 2/8 into kind 2/3 polygons) add special nodes (not ported: PROVISIONAL).
- **Direction index** of a step from cell (c, r) to (c+dc, r+dr): `d = (3*dr + dc - 1) mod 9`:
  0 E, 1 NW, 2 N, 3 NE, 4 SW, 5 S, 6 SE, 7 W, 8 same cell. Step lengths `{1, √2, 1, √2, √2, 1, √2, 1, 1}`
  (table `0x0137DFBC`). Rows grow northwards.
- **Cost multiplier** `m` of a step: when the current polygon is a road (kind 6) and the step leaves the
  cell: the **road cost of the polygon's region id** (table at grid +0x18C, see below); otherwise the
  **current cell's header byte d**: `m = byte * 0.0099502485 + 0.7960199` (= (2·byte + 160) / 201;
  byte 0 → 0.796, byte 255 → 3.33). A step inside the same cell costs 0.
- **Step cost pair** (`0x00B0B260`), positions in map units:
  - middle steps: `(2 · m · len[d], dist(next centre, segment start→goal))`;
  - from the start: `(|next − start| · m, dist(next, segment))`, or to the goal directly `(|goal − start| · m, 0)`;
  - into the goal from a cell centre (CONFIRMED from the listing): with the offset (dx, dz) from the centre
    to the goal and t = max(|dx|, |dz|): `| |dx| − |dz| | · m(d) + min(|dx|, |dz|) · (a · (1.5√2 − t·√2/2) +
    b · (t·√2/2 − √2/2))`, where a and b are the raw header-byte multipliers for the diagonal d2 of the
    current cell and of the goal's cell, d2 = the diagonal next to d on the goal's side (`0x00B31E40`: E → NE
    if dz ≥ 0 else SE, W → NW / SW, N → NE if dx > 0 else NW, S → SE if dx > 0 else SW), d2 = d for a diagonal
    d. For t in 1..3 the weights sum to √2: the diagonal part blends from the current cell's byte to the goal
    cell's byte.
  - The second component is a tie-breaker: the summed distance of the path's points from the straight line.
- Then (`0x00B11570`): first component divided by the mover's max AP (naval or land), plus the initial
  spent fraction on the step out of the start; a step from a naval to a land location costs the special
  value −1 (or −2 at kind 7), which the adder (`0x00B07A70`) turns into `ceil(max(g, 1)) + ctx+0x18` (or
  `ceil(g) + 1`): **landing ends the turn**.
- **Heuristic** (`0x00B45330`): octile cell distance to the goal `(max−min) · 0.66 + min · 0.93337864`
  (= 2 units per cell x 0.33 per unit; 0.9334 = 0.66 √2), divided by ctx+0x24.
- **Road cost table** (grid +0x18C, one f32 per region id incl. the border groups; allocated in the reader
  `0x00AF2050` and filled with campaign variable #10): `0x00B62760` sets a region's value and every border
  group containing it gets the **minimum** of its regions' values. The value of a region is campaign variable
  `[10 + road level]` (`0x00A89F30`, `0x00A682F0`, table `0x01458934` = {10, 11, 12, 13}); variables 10..13 are
  `road_level_0..3_action_point_cost` (registration order at `0x00432DC0`: INFERRED index mapping, strong).
  Road level = the region's road building level + 1 (0 without, or when the building is under 100 %).

## 3. Flag high bits (word 1 of a boundary)
- `0x00B57520` (used by the adjacency tests `0x00B168D0` / `0x00B16AE0` for kinds 0 and 1, orthogonal steps,
  and an index k < 4): tests bit `k + g · 4` of `(flags >> 4) & 0xFFFFF`, with the side group
  `g = (dr + 1) · 2 + dc`: 0 south, 1 west, 2 the cell itself, 3 east, 4 north (CONFIRMED test).
- Data check (2026-10-04): a reconstruction "bit k of group g = the polygon covers segment k of that side, the side cut
  at the vertices of the cell's enterable polygons" matches 73 % of the enterable polygons (Europe 49877/67825, Italy
  9893/13664); "bit j = shares an edge with polygon j of the neighbour cell" matches 71 %. The exact segment rule is
  not reproduced (INFERRED meaning stays); the port does not need it (adjacency from the outlines).
- Data (Europe, Italy): every whole-cell land/sea polygon has `(flags >> 4) & 0xFFFFF = 0x11011`, i.e. bit 0
  of the S, W, E and N groups and nothing for the cell itself; cells next to road strips have 0x11012,
  0x11014, 0x11021, ... (another bit of one side group). INFERRED: each side of a cell is cut into up to 4
  segments by the polygons along it, and the group's bits say which segments of that side the polygon
  borders: a precomputed form of the shared-edge test (the old note "16 interior land, 32/64/128 next to
  roads" is bit 0..3 of the south group). Our port tests shared edges geometrically, which gives the same
  adjacency.
- **Bits 24..31 = a direction mask** (CONFIRMED by the data: all split-cell polygons of all 5 maps, test
  `polypath_install.rs::direction_mask_bits_24_31`): bit d (direction index d: 0 E, 1 NW, 2 N, 3 NE, 4 SW, 5 S,
  6 SE, 7 W) is set when the polygon reaches that neighbour: an outline edge along that side (orthogonal d) or
  that corner in its outline (diagonal d). Whole cells carry 0xFF; off-map (2) and river (3) polygons carry 0
  (in every flag bit above the kind). Which code reads it: not looked for (our port derives the same from the
  outlines).

## 4. Our port (`ntw_sim::campaign::polypath`)
- CONFIRMED rules as §2: (cell, polygon) nodes at cell centres; neighbours by shared outline edge (exact
  Fixed20 vertex pairs, corners mapped through the cell origin) or, diagonally, by the shared corner point
  with the same kind (CONFIRMED `0x00B125A0`: any shared vertex); kinds per mover (armies/agents 0, 6, 7; fleets 1, 4, 7);
  step costs `cell · len[d] · m`, first step from the start point, last step into the goal point; second
  cost = summed distance from the straight line; ordering (g + h) lexicographic; update rule with the
  0.9999 / 1.0001 tolerance; heuristic 0.33 per unit octile.
- Costs are kept in action points (map units x m); `reachable` = the last point whose cost is within the
  character's remaining AP (the same as the original's "turn fraction ≤ 1" with the spent part added).
- PROVISIONAL: (a) start/goal outside an enterable polygon → the nearest enterable polygon within 2 cells
  (the original runs its "nearest valid position" floods first, `0x00B35640` etc., not ported);
  (b) start and goal in the same polygon → straight distance x the cell's multiplier towards the goal (the
  original's search ends at once; how it charges that move is UNKNOWN); (c) (solved: the last-step blend
  is now CONFIRMED and ported exactly, §2);
  (d) walked points: now smoothed (§7); in `find_path` a cell centre outside its polygon is still moved inside it;
  (e) no node budget, no landing/embark nodes, no cross-area links (every shipped map has one area).
- Data checks (install tests): Europe 103563 polygons / 602650 adjacency entries; from Spain 59 of 71
  settlements are reachable by land, the 12 others are islands or Scandinavia, which the raster built
  independently from the same polygons also cannot reach (no land link in the data); Italy 24/24, Spain
  30/30, Egypt 28/29 (Cyprus), tutorial 5/7 (Sardinia, Corsica). A search across Europe takes ~10 ms
  (dev profile, optimized). Open-land multiplier median 1.10 (Europe, Italy, Spain, tutorial), 1.66
  (Egypt); the bytes are not symmetric across cell sides (56-86 % equal to the neighbour's opposite byte):
  the cost depends on the direction of travel.

## 5. Rivers, fords, bridges (INFERRED from the data + the CONFIRMED kind table)
- Polygon kind 3 is the thin strip along the rivers: on Europe 3866 of its 4241 kind-3 polygons lie in or
  next to a cell crossed by a river spline (tutorial 3969/4346, Spain 540/762, Italy 335/515; Egypt has no
  river splines, 331 kind-3 polygons, presumably the Nile). Kind 3 is in neither mover's kind set
  (CONFIRMED table), so **armies cannot cross a river except through a road polygon (kind 6) that bridges it
  or a kind 7 polygon**; there is no extra cost for crossing, the strip simply blocks.
- Kind 7 (enterable by armies and fleets): Europe 540 of 1969 lie at rivers (fords / bridges over water),
  the rest along coasts (INFERRED: ports and landing places, where fleets and armies meet).
- The old note "2/3 off-map" (CAMPAIGN_DATA.md §1) mixed kind 2 (off-map, impassable land outside the
  theatre and the mountain blobs) with kind 3 (rivers); both carry region id 1023.
- Test: `polypath_install.rs::rivers_are_kind_3_strips`.

## 6. Handed over to pathfinding-ports: ports, embarking, landing (decoded from `0x00B2CC20` / `0x00B11570`; NOT ported here)
- Mover types: 0..11, naval = 2, 5, 8, 11 (CONFIRMED `0x00B4DA70`); in a move context types 0, 1, 3, 4 become
  6, 7, 9, 10 (`0x00AEA2D0`) and `0x00B29B20` maps 6, 7, 8 back to 0, 1, 2 (other values kept, unknown → 12).
  INFERRED: x mod 3 = army / agent / fleet.
- **Port entry** (CONFIRMED structure): when the mover is naval and the current polygon is kind 7, or a
  neighbour polygon it may not enter is kind 7, the entity at that polygon's anchor point (`0x00B489E0`) is
  looked up; if it lies within √2 map units (distance² < 2), has a garrison/port object (+0x1E4) and belongs to
  the mover's faction or passes the relation test `0x008CE9B0`, a node at the entity's position is added
  (only one such node per expansion).
- **Landing** (CONFIRMED structure): for movers 2 and 8 (INFERRED: fleets carrying armies), when no port node
  was added, every polygon of kind 2 or 3 in the current cell is tried: from its anchor point a 3 x 3-cell
  search for a valid land position (`0x00B373F0`) runs, and a found position becomes a node with the land
  mover type. The step from a naval to a land location costs the special value −1 (−2 out of a kind 7
  polygon), which the cost adder turns into `ceil(max(g, 1)) + spent` (or `ceil(g) + 1`): **landing ends the
  turn** (the cost jumps to the next whole turn).
- Owner from 2026-10-04: the `work/pathfinding-ports` worker (ports, landing, embarking, the navy search where it meets land),
  in a new module calling into `polypath`. Useful pieces there: `PolyMap::{locate, polygon_at, neighbours, kind, region_id,
  poly_cell, header, component, centre, cell_of}`, `Mover::may_enter`, `kind::SHARED` (7). Kind 7 polygons: Europe 1969, 540 at
  rivers, the rest along coasts (§5). The landing cost rule (end of turn) belongs with them; `find_path` does not apply it.
- API note: `PolyMap::component` ([land, sea] component per polygon) was added on 2026-10-04; no other public API change since.
- **Ported (pathfinding-ports): `ntw_sim::campaign::embark`, see PATHFINDING_PORTS.md** (a separate transport search, so
  `polypath` keeps its behaviour; only some helpers became `pub`).

## 7. Path smoothing (CONFIRMED structure; ported with PROVISIONAL details in `ntw_sim::campaign::polysmooth`)
- Entry: the order code (`0x00B6xxxx` wrappers of `0x00B6E6E0`, 36 callers from `0x0091xxxx`) runs the
  map-unit search variant (`0x00B31320` → `0x00AC54C0`), then `0x00B6AD30` turns its location list into the
  walked points (16-byte entries {x, z, f32 cost, flag}).
- `0x00B6AD30`: walks the cell path; at each diagonal step it looks in the two side cells (from the run-time
  polygon cache at +0x304) for a polygon the mover may enter that links both (`0x00B6AC40`, `0x00B73320`)
  and inserts it: the **corridor**.
- `0x00AF3610`: cuts every corridor polygon into **triangles** (`0x00B75860`; 40-byte records: 3 points,
  3 neighbour indices, the polygon) and links triangles of nearby cells (|dc|, |dr| ≤ 1) that share an edge
  (equal point pairs); keeps start, goal and 256 · distance.
- `0x00ACAB90` → `0x00AC4C20`: an A* over those triangles (a third template copy of the search, with an
  optional cost limit, 0 here) from the start's to the goal's triangle.
- `0x00B6BC60`: pulls the line **taut** through the triangle sequence (angles via atan2, the fixed-point
  segment/edge intersection `0x00B49A80`); output points are pushed with a cost and a flag.
- `0x00B15CD0`: **costs**: for each cell step of the search path (multiplier `0x00B204C0` × the distance
  between the cell points; a path inside one cell uses the octant of the start→goal direction and
  `0x00B20590`) the cost is spread (`0x00B16240`) over the output points between the matching indices
  (`0x00B71AB0` maps locations to output points); then consecutive equal points are merged, adding costs.
- Ours (`polysmooth::smooth`, used by `plan_path`): the same four steps with ear-clipping triangles, a
  shortest start → centroids → goal triangle search (any triangle holding the start or goal), the "simple
  stupid funnel", costs interpolated from the search points' costs (total kept, never decreasing), and the
  turning points moved 0.01 units into their triangle (so a mover never stands exactly on a corner shared
  with a river or off-map polygon). Falls back to the unsmoothed path if a step fails.
  Test `polypath_install.rs::smoothed_paths_stay_walkable` (258 paths on the 5 maps: never longer, every
  point in an army polygon, none left unsmoothed).
- API note (2026-10-04): new module `ntw_sim::campaign::polysmooth` (`smooth(&PolyMap, &PolyPath)`); `polypath`
  unchanged apart from `PolyMap::component`.

## 8. Zone of control: character obstacles (decoded 2026-10-04)
- **Obstacle per character** (CONFIRMED): created by `0x00B087A0` → `0x00AE8A50` (key = character +0xCC | 0x80000000,
  obstacle kind field = **9**), refreshed after moves by `0x00B1B7A0` (remove `0x00B654C0`, re-add `0x00B08610`).
  The obstacle class (vtable `0x0137DD20`) gives two shapes: slot 7 → character **+0x1C8 = the zone outline**,
  slot 8 → character **+0x1D8 = the core outline**; slot 4 says two obstacles matter to each other only when both
  are armies or both navies and they are closer than 200 units (distance² < 40000).
- **The zone** (CONFIRMED, `0x00A28440` → `0x00B7BAF0` → `0x00B7B7A0` → `0x00ACA170`): a Dijkstra flood from the
  character's location over the polygons its mover may enter, with **pure distance costs** (`0x00B11820`: a cell step
  costs cell · len[d], the first step the distance from the character; no terrain bytes, no roads); a neighbour is
  taken while **g + step / 2 ≤ limit**; the zone is the union of the reached polygons (its outline and bbox kept at
  +0x1C8 / +0x1B8). The limit (`0x00A2A1C0`): **6 map units for an army, 12 for a navy** (CONFIRMED, corrected
  2026-10-04: the test pair `0x009D0F50` → 6, `0x009D0F80` → 12, and `0x009D0F50` is the army test, PATHFINDING_PORTS.md
  §9.2; the saved boxes of the eur_napoleon start position match these floods, 0 match the swapped limits,
  `zoc_install.rs`; the earlier "12 army" reading took the saved box, which is the flood rounded out plus two cells, for
  the zone), 0 for other characters (no zone); **+2** when the character sits in a settlement/fort with a garrison (`0x009FB9D0`
  conditions; INFERRED).
- **Modes per searching faction** (`0x00B3F4F0` / `0x00B3F570` → `0x00B69C20`, CONFIRMED structure):
  mode 5 = ignored (no polygons): the obstacle's character is not visible to the faction (slot 9, `0x008CE880`,
  INFERRED fog of war) or outside the faction's allowed area (+0x6F8); mode 1 = **core only, kind 9** (own faction
  and factions not at war, `0x00B57C00`); otherwise (at war) mode = the faction's +0x6C value, INFERRED 0 =
  **zone as kind 8 plus core as kind 9**. Modes 3 and 4 (+0x4C variants) are not decoded. The mover's own obstacle is
  removed for its search (`0x00AF5E80` → `0x00B54900`).
- Kinds: 8 = enterable only by agents (mover types ≡ 1 mod 3), 9 = by nobody (CONFIRMED kind table, §11 of
  CAMPAIGN_DATA). So **armies and fleets cannot enter an enemy's zone nor anyone's core; agents cross zones but not
  cores**. Kinds 10/11 (from inside a 10 only 10/11/7, from 11 only 11) belong to modes 3/4 (not decoded).
- **After a diplomacy change** (polish46 2026-10-17, CONFIRMED by disassembly; details in CAMPAIGN_FIDELITY.md
  under `GrantMilitaryAccessToFaction`): access, alliance and protectorate changes call
  `RefreshFactionPathGridObstacles` `0x00B795A0` on each faction (the grid, model +0xF64, recomputes every layer's
  modes against that faction without storing the view; each of its live characters recomputes its zone,
  `0x00A28440`, and is re-added) and then `CampaignPathGrid::ReapplyPathGridFactionView` `0x00B67490` (rebuilds
  the layers for whichever view the grid last stored, mode +0x414; 15 of its 27 callers run it without
  `0x00B795A0`). Ours builds the obstacles at search time from war stances only (`plan_path`, as `0x00B57C00`;
  that test's third-faction argument is UNKNOWN), so neither refresh needs a port for the modes; the
  zone rebuild it runs does once zones are stored (next bullet).
- **The zone flood's obstacles and when it runs** (zocflood 2026-10-17, CONFIRMED by disassembly unless tagged):
  - `0x00A28440` (character, already-prepared flag): returns at once while model +0xFC7 is set; clears the zone box
    (+0x1B8..+0x1C4) and +0x1D0; only for a character with an obstacle owner (`0x006649C0`(+0x2A8)) that is an
    army or fleet commander or not in the special state (`0x009D3CB0` / `0x009FB9A0` / `0x00A17CF0`) it floods the
    zone with grid (model +0xF64) `0x00B7BAF0` and keeps the outline (+0x1C8) and its box.
  - `0x00B7BAF0`, flag 0: the view faction V is the character's own (char +0x2A0) while the campaign's byte
    +0x39 (model +0xF5C) is clear, else the grid's stored view faction +0x3FC (`0x00B07790` returns +0x3FC/+0x400;
    zoneview 2026-10-20, below). A scope (`0x00AF5200`) prepares every layer for it: the
    layer's third faction (+0x1AC) set to 0 (`0x00B72D70`), then `0x00B06280` (the character's own faction for the
    region flags, V for the modes) gives **every obstacle of the layer**
    (characters, forts, barriers; no relevance or fleet filter) its mode for V (`0x00B3F4F0`: 5 hidden,
    1 core only for its own faction or one not at war, else its +0x6C mode) and rebuilds the changed ones' cells.
    For a character whose army has +0x98 set, `0x00B078C0` replaces the layer's region flags (below) with "only
    region X": flags[slot] = 1 when the slot's region (+0x144 short) is X (army +0x98 → handle +0x68 → +0x10 →
    +0xA4), every extra slot 0 (polish58 2026-10-19, CONFIRMED by disassembly). Army +0x98 is the region the army
    rose in, its `REBEL_DATA` (0x74 bytes; zonegate 2026-10-19, CONFIRMED by disassembly and data): set by the
    risings `0x00870E60` / `0x008AF5D0` → `0x00886BA0`; loaded with the army itself (ARMY #7, `0x00870FD0` →
    `0x00886950`) and saved by the ARMY writer `0x008FA810` (the REBEL_DATA writer `0x008971C0`, called at
    `0x008FAA09` when +0x98 is set, before the navy u32 +0xEC; rebelgov 2026-10-20, CONFIRMED by decompile); the
    region reader `ConstructCampaignRegionFromEsf` `0x00A45950` reads the optional `REBEL_DATA` template
    (+0x21C, `0x00886C90`), then the region's id list (`0x0086F140`), each id resolved (`0x0105AC60`) and added
    to the set +0x220 (`0x008CFB60` → `0x008CF7D0`, appended in file order), linking the army's +0x98 block to
    the region; an army's destructor `0x0088CE30` unlinks it (`0x0088F310` → `0x0088CA00` → `0x008B0150`:
    erase, the rest shifted down, order kept; rebelgov 2026-10-20, CONFIRMED by decompile). In the save the list is
    `REGION` #35 when #34 is the `REBEL_DATA_INNARDS` record, else #34 (orig_fr_may1811: each id a rebel land
    army). Then the flood (`0x00B7B7A0`) and the
    scope's end (`0x00AF7A00`: third factions cleared, `CampaignPathGrid::ReapplyPathGridFactionView` `0x00B67490`
    restores the stored view). Flag 1: the flood runs on the layers as they stand (the caller prepared them).
    A V without a record (`IsFactionWithoutRecord` `0x008CEEF0`) while +0x400 is non-zero floods unprepared.
  - **The grid's stored view** (zoneview 2026-10-20, CONFIRMED by decompile, disassembly and an instruction
    search for every store): `CampaignPathGrid` (model +0xF64) +0x3FC view faction, +0x400 single-region index,
    +0x404..+0x410 the view list, +0x414 mode (0 faction, 1 list, 2 rebel region; the constructor `0x00AE7350`
    leaves +0x400 = 0, +0x414 = 3). `0x00B06210`(faction, store): every layer's modes and flags for the faction
    (`0x00B06620` → `0x00B06280`); with store set it also writes +0x3FC and +0x414 = 0. Store set only from the
    load pass `0x00B0DC10`, `0x00B0DF30`, `0x00B67490` mode 0 and the acting view `0x008A87D0`; the diplomacy
    refresh `0x00B795A0`, `0x00A87F50` and the barrier refresh `0x00B64E30` pass 0. `0x00B06C00` stores the list
    view (mode 1). `StorePathGridRebelRegionView` `0x00B077B0`(character): `0x00B06210`(his faction, 0), per layer `0x00B078C0`, then +0x3FC = his
    faction, +0x400 = his army's +0x98 region index (handle +0xA4) at `0x00B0784C`, +0x414 = 2; nothing else
    writes +0x400, so it is never cleared. `ReapplyPathGridFactionView` `0x00B67490`: mode 0 → `0x00B06210`(+0x3FC,
    1); 1 → `0x00B06C00`; 2 → `0x00B06210`(+0x3FC, 0) and `0x00B07870`(+0x400) (per layer `0x00B07950`, only
    that region's slots); `0x00B67420` (from `0x00B617E0`) is the same with `0x00B05E50`. Who stores what: the
    load pass per faction `0x00B06210`(f, 1) before its characters, per rebel character `0x00B077B0`; the campaign
    start `0x00872550` runs `0x00B0DF30` (re-adds the obstacles, no flood, stores the same views, rebels last),
    sets campaign +0x39 = 1 at `0x00873D4D`, then the acting view `0x008ED860` at `0x00873DF3`; `0x008ED860` →
    `0x008A87D0` stores the acting faction's view (`0x00974EF0`: the turn faction, campaign +0x18, outside a
    revolution; see "Revolutions" below), at each faction turn start (`0x008F2620` at `0x008F28D3`) and after
    battles (`0x008F7880`); the campaign AI's order steps (`0x00A645C0`, `0x00A65360`, `0x00AAF550` →
    `0x00C3C320`, others in `0x00A6xxxx`–`0x00AAxxxx`) call `0x00B077B0` for the order's character when his
    faction has no record, before issuing the order (`0x00A645C0`: `0x00B077B0`, then `0x00915F20`), and never
    put the old view back, whether the order succeeds or not. Campaign +0x39: cleared by the campaign constructor `0x0090C4A0` (word +0x38 at
    `0x0090C527`), set at `0x00873D4D` and `RunCampaignRoundStart` `0x0096C054`, never cleared after. So the load
    pass floods each commander with his own faction's modes, and every later flood with the stored view's: in a
    faction's turn every commander's zone, of any faction, sees the obstacles with the turn faction's modes, and
    after a rebel's order with the rebel view (only his home region's slots). `0x00A84230` (AI scratch) stores and
    puts back a view around its own work. Ours: `zones::GridState` (view, +0x400, +0x39), `store_acting_view` at
    `TurnStep::FactionStart`, `store_rebel_view` before a rebel's force order in `CampaignModel::apply` (kept
    when the order fails); the load pass is unchanged (zoc_install: 64 of 87 commanders' saved zones match).
    PROVISIONAL: the views `0x008F7880` (post-battle continuation) and `0x008C1140` store are not modelled.
  - **Revolutions (acting view; CONFIRMED by disassembly, zoneview round 2):** `0x00974EF0` returns campaign
    +0x1C instead of the turn faction +0x18 only while +0x18 is in a revolution (`0x008CA0B0`: +0x814) with
    +0x81C == +0x81D. `0x008A87D0` returns a character only when that acting faction differs from +0x18
    (`0x00898B80`), has no record (`0x008CEEF0`), the revolution settlement (+0x818, set by `0x008EE3E0`) resolves
    (`0x008B7810`, its vtable +0x34) and its +0x220 list is non-empty (`0x006A3110` = +0x22C; items +0x230): the
    first entry whose +0x74 handle resolves (`0x00898E80`) is the character, and `0x008ED860` stores his rebel view
    (`0x00B077B0`) instead of a faction view (`0x00B06210`). So in the rebels' own turn the acting view is the
    rebels' faction view, as ours. PROVISIONAL (BACKLOG §6 revolutions, faction +0x814 / +0x81C): revolutions
    are not in our model, so `zones::acting_view` always takes the turn faction.
  - **Other `0x008ED860` / `0x008A87D0` callers** (listed, not traced further; ours stores the acting view only
    at the campaign start and the faction turn start): `0x00AAB440` (the rebel spawn of the unrest outcomes)
    stores it again near its end (at `0x00AAD3E6`); `0x008BD6D0` (a faction's end of turn, after
    the autosave) stores it when the turn faction is in a revolution with +0x81D clear, setting +0x81D = 1 first
    (the revolution's second half of the turn); `SHROUD_SetEnabledAndRefresh` `0x00B68500` (script
    `show_shroud`) stores it before posting the redraws; `0x008ED820`, the same choice with a faction view stored
    by `0x00B05E50` (+0x3FC = faction, mode 0; or the rebel view), is called only by the revolution setter
    `0x008EE3E0` (at `0x008EE479`). A seventh call at `0x008B9EC2` lies outside any defined function.
  - `0x00B7B7A0`: starts at the army's, the carrier's or the character's own position (an army-less, carrier-less
    character with +0x4E8 = 0 must pass `0x00B6A860`, else no zone); locates it (`0x00B53EE0`, kind 0xC = no zone);
    limit `0x00A2A1C0` + 2.0 when the army's slot 0x58 or 0x48 test holds; flood `0x00ACA170` with neighbours
    `0x00B7BBB0`. When the character's faction has +0x6E0 set, the active script barriers (grid +0x3F4 list, 0x9C
    stride, +0x14 active; Lua `add_barrier` / `remove_barrier`, `remove_barrier` = `0x00977700` → `0x00B64E30`) are
    cut into the flood (`0x00B09020` → `0x00B5E7D0`). None are in the shipped start positions: not ported.
  - So a zone is bounded by the other characters' cores (kind 9) and by the zones of factions at war with the
    owner (kind 8) as the owner's faction sees them. The owner's own obstacle does not stop it (the saved
    start-position zones are never empty, `zoc_install.rs`; CONFIRMED by data).
  - **The region gate** (polish58 2026-10-19, CONFIRMED by disassembly unless tagged): a layer holds one flag
    byte per slot (+0x158..+0x164 vector, data +0x164): +0x140 land slots, each a pathfinding region (+0x144
    shorts), then +0x150 extra slots (+0x154, 0x10 stride, each a region list at +8 count / +0xC data). The
    constructor fills the flags with 0 (`0x00AF309E`) and sets the gate byte +0x168 to 1 (`0x00B0C9F0` /
    `0x00B0CA10` write it). `0x00B78A10` (layer, faction) sets flags[slot] = `IsRegionOpenToFaction` `0x00B622E0`(faction, the slot's
    region index; cdecl): 0 when the faction has no record (`0x008CEEF0`, the rebels); 1 when the region is
    not found (`0x00AA12A0` on campaign +0xF5C +0x34, null on a miss), its owner (region +0xF4, `0x006649C0`)
    has no record, or the owner is the faction itself; otherwise the owner's record towards the faction
    (`0x00B64C50` on owner +0x528; on a miss it returns the owner's first record, never null, and
    `0x00B16570` reads it with no check: an unrelated faction's record is tested; zonegate 2026-10-19) passes `CheckRelationshipLetsFactionStand` `0x00B16570` (access turns +0x78C, allied_in_war_against +0x7D0,
    protectorate `0x00B5DCC0` or the war stance at +0x790; ours `record_lets_stand` in `expulsion.rs`). This
    is not `0x008D9DC0`'s flag, which ours ports as `may_stay` (true for a recordless faction, through
    `at_war`). An extra slot gets 1 only when every
    region of its list passes. The flood's neighbour test (`0x00B7BBB0` at `0x00B7BD68` → `0x00B16CF0`, context
    +0xC the current polygon's key, +0x10 the layer) takes a neighbour whose key has the same top 10 bits (the
    pathfinding region, `0x00B680A0`; `Boundary::region()` in ours), else only when `0x00B16BD0` passes on the
    neighbour's key: +0x168 is 0 or flags[key >> 22] (`0x00B62280`) is set. So **a zone does not flood into a
    region its owner may not stay in** (and, for a +0x98 army, out of region X). The gate byte +0x168 is
    cleared only by `0x00B0C9F0`, from the scopes `0x00AF5B60` (when its first argument is set) and `0x00AF5C00`,
    and set back by `0x00B0CA10` (`0x00AF7BE0`); their callers are path searches. The zone flood's scope
    `0x00AF5200` leaves it set, so the gate always applies to a zone (zonegate 2026-10-19).
  - **The path searches share the gate** (pathgate 2026-10-20, CONFIRMED by disassembly unless tagged). The
    neighbour expansion `0x00B2CC20` calls `0x00B16CF0` at `0x00B2D021` (its first argument is the node's
    context: +0xC the current polygon's key, +0x10 the layer) and the second expansion `0x00B16BA0` at
    `0x00B2DCA3` (the same test, the layer passed in); so do `0x00AE3300`, `0x00AE3DD0`, `0x00AE4740`,
    `0x00B2AB40`, `0x00B2C0A0`, `0x00B2E020`, `0x00B2F4E0`. Every step is tested, the goal's polygon
    included: **a move does not cross into a region whose flag is clear** while the gate byte is set.
    `0x00B0C9F0` clears +0x168 (returns 1 when it was set), `0x00B0CA10` sets it (only from `0x00AF7BE0`,
    the scopes' restore). The scopes: `0x00AF5B60`(flag, layers, ctx) and `0x00AF5C00`(flag, mover,
    target owner, layers, ctx); flag 1 turns the gate off on every layer for the search. All 22 callers
    of `0x00AF5C00` (`0x00B20600`..`0x00B25470`, the move-to-target queries, e.g. `0x00B21510`,
    `0x00B22890`) pass 1 only when the mover leads no army (`0x009D0F50`) and is no fleet (`0x009D0F80`):
    **an agent searches with the gate off, an army or fleet with it on**. Most `0x00AF5B60` callers make
    the same test (`0x00B0FB60`, `0x00B37370`, `0x00B0FF90` by mover type). With flag 0 and a target,
    `0x00AF5C00` also opens the target owner's regions when the owner is not the mover's faction
    (`0x00898B20`), both have records and the owner's record towards the mover fails `0x00B16570`: per
    layer `0x00B628E0` walks the owner's region list (faction +0x778/+0x77C), takes each settlement's
    position (item +0xFC, vtable +0x3C), skips one outside the grid (`0x00B1FC40`), locates its polygon
    (`0x00B53930`, mode 6) and sets that slot's flag (`0x00B62100` → `0x00B620A0`, slot = key >> 22 by
    `0x00B62280`), keeping the changed slots; then every extra slot whose regions' flags are all set
    gets 1 (an empty one too); `0x00AF7BE0` restores them. The target owner is the region at the target
    point (`0x00AA1030`, a quad tree `0x00AA16A0` / `0x00AA0EC0`; ours `PathGrid::region_at`). The layers
    hold the acting faction's stored view (`0x00B06210` → `0x00B06280` → `0x00B78A10`).
  - **Our port:** `View::gate_lets` (polypath.rs) is the one neighbour test, used by the flood
    (`zoc::reach_in`) and by `View::find_path_gated` (the A*'s neighbour loop; `find_path_avoiding`
    passes no gate). `Zones::path_region_gate` builds the flags from the flood's rule
    (`regions_open_to` → `region_open_to`, `slot_flags`) plus the target owner's regions;
    `CampaignModel::plan_path` (no target) and `plan_path_to_target` (attack, merge, enter a settlement,
    an agent's action) run armies and fleets behind it and agents without it. Tests: zones.rs
    `an_army_paths_only_into_regions_its_faction_may_stand_in`, `an_agent_and_a_rebel_force_path_without_the_gate`,
    `the_path_gate_has_the_zone_floods_flags`; zoc.rs `the_region_gate_stops_a_path_at_a_closed_region`.
    PROVISIONAL: a recordless (rebel) mover's search runs on the layers as the grid's stored view left
    them (after his own order `0x00B077B0`'s: his home region's single slots, +0x400). Ours keeps that view
    (`zones::GridState`) but `path_region_gate` does not read it, so a rebel force paths with no gate; how
    the search scopes (`0x00AF5B60` / `0x00AF5C00`, `0x00B67490` mode 2) treat a recordless mover is not traced.
    Not ported: the transport / port-entry
    searches (embark.rs) and the commander-recruitment search; the smoothing corridor `0x00B6AD30`'s
    region test (not traced).
  - **When it runs** (callers of `0x00A28440`): `0x00A285C0` (flag 0, then the core `0x009CC0E0` and the sight
    `0x00A27F10`); after a move `0x00A285E0` (flag 1, then the core); `RefreshFactionPathGridObstacles` `0x00B795A0`
    (flag 1 after preparing the layers for the faction, `0x00B06210`, each live character of it, then re-added);
    `0x00B64E30` (a barrier removed: every faction's characters); also `0x008E2260`, `0x009CB4C0` (from
    `0x00881120`), `0x009D3B60`, `0x00A16110` (no direct caller) directly. A zone is kept between these, so it
    depends on their order: two enemies that meet differ (the later one's zone stops at the earlier one's).
    The events (zocflood 2026-10-17, CONFIRMED by disassembly unless tagged):
    - **Loading** `0x00B0DC10` (from the campaign constructor `0x008742C0`): zones are not saved, every one is
      flooded again: factions in turn order (campaign +0xF5C +0x30), the rebels last; per faction each character of
      its list (+0x768) flooded, then added as an obstacle (`0x00B084F0`). So each flood sees only those added
      before it. The constructor runs it right after the world is read (`0x0090C4A0` at ~`0x00874E03`) and before
      the later subsystems (`0x00882C30` at +0xF6C onwards): it clears the model's +0xFC7 flag (`0x00874E0D`; while
      it is set `0x00A28440` returns at once, so the world load floods nothing), then calls `0x00B0DC10` on
      the grid +0xF64 (`0x00874E22`).
    - After a move `0x00A285E0` (a virtual: character vtable `0x0136A62C`, slot +0x20; its only reference): the
      sight `0x00A27F10` for the character (`0x00A2863A`) and for each character it carries (`0x00A28694`); then,
      only when its first argument is 1, `0x00A28290`, the zone flood `0x00A28440(1)` (`0x00A28843`) and the core
      `0x009CC0E0`; then the hidden flag `0x009D3000` (`0x00A2885C`). Argument 2 returns early (`0x00A2860D`).
      (polish58 2026-10-19) Before the sight, a character not yet lifted is lifted off the map (`0x00A28613`
      → vtable slot +0x1C `0x00A28B50`: +0x1B7 = 1, its obstacle and listeners dropped, the zone box +0x1B8..+0x1C4,
      +0x1D0, +0x1E0, +0x4C8, +0x4E0 cleared); argument 1 sets him down: +0x1B7 = 0 at `0x00A28855`, between the
      core and the hidden flag. So argument 0 is a mid-move step (lifted, sight only) and argument 1 the last
      (flood, core, hidden flag); the hidden flag also runs only with 1. `0x00B795A0` skips lifted characters
      (`0x00B795FC`). Its callers (walkflood 2026-10-22, CONFIRMED by disassembly; Ghidra name
      `UpdateCharacterAfterMoveStep`): the character's primary vtable `0x0136A62C` (set at `0x0098F3CA`) sits
      over a movable base at offset 0 (vtable `0x0135FAC8`, slots +0x1C / +0x20 purecall; constructors
      `0x00907440` / `0x009078A0`, which store the character himself at +0x20, `0x0099154E`). The move manager
      (campaign +0xF60) keeps the moving characters in a list at +0x24..+0x28 whose node +8 is the character;
      `UpdateMovingCampaignCharacters` `0x00973B90` steps it every update and `StepCampaignCharacterMove`
      `0x00972CA0` steps one character (`this` = the character, `0x00973C73`). Its +0x20 calls pass: 1 when the
      order is done (`0x00972DC9`) and when the current waypoint is the path's last and no order object is
      left (`0x00972EB1`); in the stop branch, after the route is trimmed (`0x00924D30` / `0x00924BD0`), 1 if
      no waypoint was kept and 0 if one was (`0x009732E3`, flag byte +1 of `0x00924BD0`'s result read by
      `0x0096E4E0`); 2 for a blocked step (`0x0097342C`) and a pending stop that still holds (`0x00973B44`);
      the finished flag at the waypoint loop's end (`0x00973A01`, 0 mid-route; a multi-turn order, character
      +0x9C). Its result codes: 0 keep moving, 1 remove, 2 blocked, 3/4 restart, 5/6 queue. Other callers
      pass 1: `0x00960620`, `0x00923140`, `0x00921EE0`, `0x00921CD0`, `0x00A1A6E0`, `0x0090B770`, and the
      placement `PlaceCampaignCharacterAtPosition` `0x0096C8D0` on its second call (`0x0096C9C4`, after a 0
      at `0x0096C930`), so a placed character is set down even in place. INFERRED: which branch ends a walk
      cut short by its action points, and that it passes 1 (debugger: break at `0x00A285E0` on a move longer
      than a turn's points, read [ESP+4] at that character's last call of the turn and +0x1B7 at turn end).
    - `0x009CB4C0` (polish58): called only by the `MILITARY_FORCE` reader `0x00881120` (from the `ARMY` reader
      `0x00870FD0` and the navy reader `0x008822F0`) on the commander read by id: links him and the force
      (`0x008CFAB0`, a thunk that adds 0x74 to its stack argument and jumps to `0x008CF920`) and, when his
      +0x4C4 is set, `0x00A28440(0)`, the core and the sight. It runs while a save is read, when +0xFC7 is
      still set, so it floods nothing; the load pass does.
    - `HireAdmiral` `0x00A16110` (polish58; a virtual, its pointer at `0x0136AEC8`): when the fleet's
      commander after the pick (`0x008B8E50`) is not the one before, he is put at the fleet's place saved on
      entry (`0x00963840`) and his zone flooded again (`0x00A28440(0)` at `0x00A162CA`), then `0x0096C8D0`.
    - `0x0096CBE0` (polish58; from the battle set-ups `0x008E18D0` / `0x008E1A90`, model +0xF60): floods
      (`0x00A285C0`) and re-adds (`0x00B084F0`) each character of its list (+0x24..+0x28, entry +8 → +0x20)
      that is lifted (+0x1B7): the list is the move manager's moving characters (above; node +8 the character,
      +0x20 his movable base's pointer to himself, read at `0x0096CC06`), so a character caught in mid-move
      gets his zone back for the battle (walkflood, CONFIRMED by disassembly; Ghidra name
      `UpdateLiftedMovingCharacterZones`).
    - `0x00A285C0` callers: `0x008F9400` (a force's commander plus the characters it carries or that carry it;
      from the commander pick `0x008B8E50` and the unit transfers `0x008D47B0` / `0x008D5F10`), a split or new
      force `0x008F04C0`, the boarding joins `0x008F0F20` / `0x008F17D0`, a commander's death `0x008DB3E0`, the
      promotions in the field `0x008E1C20` / `0x008E2260`, `HireGeneral` `0x00A164C0`, a recruited general placed
      `0x00B67670`, a settlement entered `0x00B4E760`; `0x0096CBE0` for lifted characters (above).
    - `0x00B795A0` per diplomacy rule: an expulsion over another's lands (`0x00B13A30`, `0x00B0CA20`, `0x00B0D0C0`,
      `0x00B29170`, `EndProtectorateTie` `0x00B64B10`) refreshes the expelled faction, then the owner, inside the
      expel branch; `CancelMilitaryAccessOnRecord` `0x00B67BD0` none (its command path `0x00B67B20` refreshes the
      target after the grievance mark, before the broadcast; the war abuse path `0x00B0DAF0` none);
      `GrantMilitaryAccessToFaction` `0x00B444B0` the receiver, then the giver; `MakeProtectorate` `0x00B105C0`
      the patron once, before the broadcast (`0x00B106CC`); peace `0x00B262C0` the proposer, then the target;
      a war declaration `0x00B268B0` the declarer's allies that joined (its list +0), the target's that joined
      (+0x20), the target's that refused and broke with it (+0x30), the target, the declarer (the joins'
      `0x00B26700` passes 0 for its refresh); a captured region's owner change (`0x00B62120` from
      `TransferRegionToFaction` `0x00A64AC0`) the new owner, then the old.
  - Ours (zocflood 2026-10-17): `ntw_sim::campaign::zones` stores each commander's zone in the model
    (`world.zones`, not saved or compared), built by the load pass when the map is attached to a loaded model
    (`attach_terrain` from `MapData::attach`, as the constructor runs it right after the world load; 3.8 ms for
    the 87 eur_napoleon commanders, release) and never by a read, so a path preview changes nothing (test). They
    are rebuilt at the events above (`rebuild_zone`, `rebuild_force_zones`, `rebuild_moved_zones`,
    `refresh_faction_zones`, called from the walk, merge and transfer, settlement entry, recruit, hire,
    promotion, command succession, boarding and landing, and the treaty, expulsion and capture rules in the
    exe's orders). The walk runs the sight, then the zones, then the hidden flag, as `0x00A285E0`'s last step
    does; it moves whole, so no character is left lifted. One function, `CampaignModel::set_down` (walkflood),
    is that mode-1 step: the walk's end (`0x00972EB1`), the landing's fleet sail, the landed army (its route
    ends at the landing, PATHFINDING_PORTS.md §5 "Disembark") and the expulsion placement (`0x0096C9C4`) call it, and only the moved character's hidden flag is re-evaluated (`0x00A2885C`), not
    the flags of the characters he carries. A stored zone stays until such an event, wherever its
    character stands (an army an enemy walks up to keeps its zone; an expelled army the rule places is
    flooded at its new place, one it cannot place (`to: None`) keeps its old zone until its faction is flooded
    again: the treaty expulsions refresh both factions inside the expel branch, the cancel `0x00B67BD0` none,
    above). The
    path search (`zoc::cuts`, `CampaignModel::obstacles_seen_by`) reads the stored zones, so two neighbouring
    enemies leave no gap between their zones (`zones.rs` tests, one per event kind). A commander with no stored
    zone (a path that floods none) reads as his bare flood, logged once. The faction list order is the
    list's own (`World::character_order`, appendorder 2026-10-20, below).
    The region gate (zonegate 2026-10-19): `zones::region_gate` gives each layer slot its flag as
    `0x00B78A10` / `0x00B622E0` do (`region_open_to`, through `record_lets_stand`) or, for an army in a
    region's `World::region_risen_armies` list (read from and written back to `REGION` #34/#35), only its
    home region's single slots (`0x00B078C0`); the load pass gives every rebel character the region it stands
    in (`0x00B077B0`). `zoc::reach_in` takes the flags in its neighbour test. A missing relationship record
    reads as no access (ours; the exe's first-record read above is an ORIGINAL BUG, unreachable with shipped
    data: every non-rebel pair has a record in eur_napoleon's startpos and the evidence saves, measured).
    Every flood after the load pass reads the grid's stored view (`zones::GridState`, zoneview 2026-10-20,
    "The grid's stored view" above). With the gate the stored zones give the saved box and zone
    range for 64 of 87 start-position commanders, the bare flood 60 (`zoc_install.rs`), so the save writer
    now takes each commander's stored zone (`NewObstacle::zone`, `obstacle_record_with_zone`); agents keep
    no zone. The other 23 are UNKNOWN (BACKLOG). Not ported, none needed: the barrier refresh
    `0x00B64E30` (no barriers), `0x0096CBE0` (no character of ours is left lifted), `0x009CB4C0` (floods
    nothing); `HireAdmiral`'s flood is `rebuild_force_zones` after the admiral is put at the fleet's place.
    The order of a faction's +0x768 list (appendorder 2026-10-20, CONFIRMED by disassembly): a PtrVector at
    faction +0x758 (count +0x764, data +0x768) of link objects at character +0x2A0 ({+0 character, +4 owning
    list}). Append `0x008CF6B0`: a link not yet in a list is pushed at the end (`0x00448B90`, growth by doubling).
    Every character constructor reaches it through `0x00878C10` (adds +0x758) and `0x0086FC50`: the ESF reader
    `0x00991520`, the generic, historical and officer constructors (calls at `0x0098F4F7`, `0x0098FB1A`,
    `0x00991278`) and `0x00991F40`, `0x009902D0`, `0x00990900`, `0x0098FE60`, so a new character goes at the end.
    Load: the faction loader `0x0087A190` (`0x0087AD56`..`0x0087ADE2`) constructs the `CHARACTER_ARRAY` records in
    file order, so the list is the file order. Save: the faction writer `0x00892A80` (`0x00892EC0`..`0x00892F61`)
    writes `CHARACTER_ARRAY` walking the list, so the file order is the list order. Removal `0x008B0150` erases
    and shifts (order kept). A faction transfer `0x00A1B4E0` unlinks, then appends at the end of the new
    faction's list (thunk `0x008CF9F0`); callers `TransferRegionToFaction`, `0x00AAE250`, `0x008BEAA0`,
    `0x009DBBB0` and a helper at `0x008E477F` (no function defined). Ours: `World::transfer_character` sets the
    faction and moves the id to the end of `World::character_order` (test
    `a_transferred_character_goes_to_the_end_of_the_list`); no rule transfers a character yet, and the rest of
    `0x00A1B4E0` (character +0x478, the force calls `0x008AF5D0` / `0x008F3340`, the +0x2A8 vfunc +0x4C, three
    events) is left for the port that first needs it. Readers in order: the load pass `0x00B0DC10` and
    `0x00B795A0`. The original's saves list characters out of id order (`orig_fr_may1811.save`: 57 descents among
    146 characters; ours and the start position rise), so the old id order differed from the exe on every vanilla
    save. Ours: the importer fills `World::character_order` in file order, new characters (rising `alloc_id` ids)
    follow it, the ESF writer keeps file order and appends new ones; `zones.rs` walks
    `World::characters_in_list_order` (tests `the_load_pass_floods_a_factions_commanders_in_list_order`,
    `zoc_install.rs` `vanilla_save_faction_lists_keep_the_file_order`). Other +0x768 readers (charorder 2026-10-20, CONFIRMED by disassembly):
    the round-end economy `0x008BC650` (read at `0x008BC6FD`) collects the characters whose yearly death check
    `0x009DA190` passes (one campaign RNG draw each) in list order and kills them in that order, so the draws
    follow the list: ours `yearly_character_pass` walks `World::characters_in_list_order` (test
    `the_yearly_death_checks_draw_in_list_order`). Expulsion `0x008D9DC0` (loop at `0x008D9E7A`) walks the list
    once, each character's land army then his docked navy, so the moves and notices follow the list: ours
    `expel_faction_from_region` did armies by force id, then navies, and now walks the list (test
    `the_expulsion_walks_the_faction_list_in_its_order`). Order-independent: the out-of-game flag `0x008B4330`
    (an all-of check: with no region, +0x824 is set only when every character's +0x2B8 handle resolves through
    `0x006649C0`, returning at the first that does not; +0x2B8 is the character's government post handle, set by `0x009CB1E0`, CONFIRMED by charflag, see CHARACTERS_FIDELITY.md "Out-of-game flag"). Not
    order-independent after all: peace `0x00B262C0` (`0x00B263A6`, `0x00B263E4`): each character's `0x00A25570`
    ends his own siege, blockade and raids, but each end's price update runs the raid check, which can end a
    later character's raids first, so the number of updates follows the list; ours walks it
    (`end_war_actions_on_peace`, peaceprice 2026-10-20). About forty more functions read +0x768 (BACKLOG).
- Save layout (`OBSTACLE_LISTS/CHARACTER_OBSTACLE[]/OBSTACLE`): #0 six boundary slots (slot 0 = zone pieces,
  slot 1 = core pieces, entries 0x80000000 | run-time boundary index), #6 = 9, #7..#10 Fixed20 bbox, #11..#18 u16
  ranges (cell ranges of the two cut layers, decoded in PATHFINDING_PORTS.md §10.4). Tool: `cargo run -p ntw_campaign --example obstacle_probe -- <esf>`.
- The core shape (+0x1D8): see §10 (24-gon radius 1; 0.25 triangle inside a settlement).
- **Ported (2026-10-04)** in `ntw_sim::campaign::zoc` and used by `CampaignModel::plan_path`: obstacles = the
  commanders of the other forces; per mover: both navies or both not (agents meet armies; corrected round 4, `0x00B57260`
  compares the fleet test) within 200 units; at war
  → the zone (flood ≤ 6 army / 12 navy, +2 in a settlement; corrected 2026-10-04) blocks, otherwise the core blocks
  (since round 4 both are cut into the map by `rtcut`, PATHFINDING_PORTS.md §10.2); agents are stopped only by cores. `PolyMap::find_path_avoiding` takes the blocked polygons; a goal inside
  a blocked area ends at the nearest free polygon within 10 cells (PROVISIONAL for the original's "nearest valid
  position" floods). PROVISIONAL rules: obstacles holding the mover's start or standing within 3 units of the goal
  (the order's target: attack, merge, enter a settlement) are ignored; every character is visible; agents carry no
  obstacle of their own. Tests: `zoc.rs` unit tests, `campaign_play.rs::paths_bend_round_enemy_zones` (eur startpos:
  French paths past Austrian/British armies never enter their zones, 3-11 ms per plan).
- API note (2026-10-04): new `PolyMap::find_path_avoiding`, `polysmooth::smooth_avoiding`, `BLOCKED_GOAL_RADIUS`,
  module `zoc`; `find_path` / `smooth` unchanged (they call the new ones with nothing blocked).

## 9. Search node budget (pathfinder +0xE0)
- CONFIRMED: the search objects built by the `pathfinding.esf` loader (`0x00AF3B20`, `0x00AF44A0`) set the counter
  (+0xDC) to 0 and the budget (+0xE0) to 0xFFFFFFFF: no budget in practice. Ours has none either.

## 10. Zone of control leftovers (2026-10-04, second pass)
- **The core shape: CONFIRMED** (`0x009CC0E0`, run when a character's obstacle is refreshed): character +0x1D8 is a
  polygon of **24 points on a circle of radius 1 map unit** round the character (angles i x 2π/24, Fixed20); in a
  special state (`0x009D3CB0` true: INFERRED busy / not on the map normally) it is a triangle of
  radius 0.25 when `0x00A0B820` holds, else empty (the special state is a character inside a settlement: the saved core
  cell ranges of every garrisoned commander fit the triangle, PATHFINDING_PORTS.md §10.4). Ported: `zoc::core_shape`;
  since round 4 the core is clipped into the map exactly (`rtcut`, PATHFINDING_PORTS.md §10.2).
- **Visibility (fog of war) of obstacles: CONFIRMED rule** (obstacle vtable slot 9 `0x00B4D600` → `0x008CE880`): for a
  searching faction F, another faction's character obstacle is ignored (mode 5) when F cannot see the character: it is
  hidden (`0x009CB560(5)` > 0, INFERRED a stealth/hidden value) or flagged at +0x4E0, and it is not in F's list of
  spotted characters (faction +0x80C/+0x810, re-linked after loading by `0x008E07F0`). Ported in round 6 of 0-G
  (CHARACTERS_FIDELITY.md §10): `plan_path` leaves out obstacles outside the faction's sight or of characters it does
  not know; `0x009CB560(5)` is the `subterfuge` attribute (CONFIRMED); the stealth test and spotting are not ported.

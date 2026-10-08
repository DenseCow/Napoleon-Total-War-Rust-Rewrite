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

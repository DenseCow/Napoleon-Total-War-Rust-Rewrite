# Campaign pathfinding: ports, embarking and landing (BACKLOG §1)

Worker: the AI slot on loan (branch `work/pathfinding-ports`, from `work/pathfinding`; worktree
`%USERPROFILE%\Documents\NR-ai6`; Ghidra copy `NR-ai-ghidra`, run with `PathDecomp.java` through a
private runner in `target/tmp`). Tags: CONFIRMED / INFERRED / UNKNOWN; stand-ins PROVISIONAL.
Read `PATHFINDING.md` first (the search itself, §2; the first port/landing notes, §6). No decompiled
code is kept here, only specs.

## Where I am / what's next
- DONE (decode): the port node, the landing node and its position search, the landing cost, the
  mover types of a transport query, the embark query (its own search variant), the polygon kinds at
  the coast (kind 2 = the coastal strip between land and sea; port polygons are tiny kind 7 islands).
- DONE (port, §7): `ntw_sim::campaign::embark` (transport search, landing positions, port nodes,
  harbour exits, embark points), the commands `Embark` / `Disembark` (and a move order to an
  embarked army), `World::embarked` (saved by position, restored by the loader), navies carry their
  passengers, `plan_path` for embarked armies and for navies into / out of ports, the script events
  `CharacterEmbarksNavy` / `CharacterDisembarksNavy`, the app (right-click an own fleet with an army
  selected = embark; harness `--campaign-demo-embark`).
- Tests: unit tests (`embark.rs`, `campaign/tests.rs`), install tests `ports_install.rs` (the coast
  strip on all 5 maps) and `embark_campaign.rs` (eur_napoleon: the French fleet sails into Genoa,
  Masséna's army boards it, is landed on Corsica in the same turn, walks on next turn; a save
  round trip keeps it aboard). Game run: `napoleon --campaign eur_napoleon --campaign-demo-embark
  --screenshot x.png` (log lines "Embark demo: ...").
- ROUND 2 (2026-10-04, §8): CONFIRMED and ported: the polygon point (`0x00B489E0`), the landing
  valid-position check (`0x00B6A0D0`), the port relation test (`0x008CE9B0` = at war: ports are
  nodes for their owner and its enemies, not for allies). INFERRED and ported: the harbour exit
  through the port footprint (`regions.esf`; fits all 67 ports, the run-time cutting code not
  found), one carried army per fleet with a second joining it within 20 units (comparison
  CONFIRMED), boarding spends no AP itself.
- ROUND 3 (2026-10-04, §9): the run-time cutter decoded down to the polygon clipper (structure,
  shape kinds per mode, the kind link table CONFIRMED; the clipper's overlap kinds INFERRED; not
  ported). Map-slot (building) obstacles do not concern fleets, so the harbour exit stays INFERRED.
  Mover families CONFIRMED (every character moves as family A unless inside a building) and the
  kind-7 rule ported for land movers (`movers`; goal footprint open, PROVISIONAL; all settlements
  reachable on the 5 maps). The transport link in saves CONFIRMED (NAVY #4 / ARMY #7) and read by
  the loader; writer change handed to save-compat. Zone limits look swapped in `zoc` (army 6,
  fleet 12; §9.2, for the pathfinding worker).
- ROUND 4 (2026-10-04, §10; `zoc` owned here now): zone limits fixed (army 6, fleet 12) and checked
  against the start position's saved obstacles; the overlap kinds decoded (`0x00B13520`) and the
  cutter ported (`rtcut`, `polypath::Overlay` / `View`, `plan_path` searches the cut map); the second
  kind 7 rule (`0x00B167B0`) decoded and both rules applied to every mover, fleets included; the
  garrison core (0.25 triangle) CONFIRMED; `zoc::obstacle_record` for the save writer.
- ROUND 5 (2026-10-04, §11): the original's obstacle storage decoded completely (pieces, cell
  versions with their flags rule, grid nodes, pair lists, slots = modes) on vanilla data only;
  `ntw_campaign::grid_obstacle::add_character_obstacle` writes a complete obstacle for a new
  character (passes `save_check`; same cells and rings as the original for 68 of 95 start-position
  obstacles). The §10 open items looked at again (§11.3).
- ROUND 6 (2026-10-04, §12): modes 3 / 4 decoded; mode 4 (an enemy zone holding the goal, as
  kind 10) and mode 1 for a zone holding the start ported, with the kind 10 / 11 step rule. The
  campaign slowdown traced to `Effects::compute` per region / faction in the AI snapshot (report to
  the manager; fix in 0-B's / the AI's code, not applied here).
- ROUND 7 (2026-10-04, §13): mode 2 / 3 CONFIRMED (the order's target, zone 10 / core 11) and
  ported; family A never enters its target footprint by search (CONFIRMED; the order code
  snaps it in); the zone flood's node points decoded in structure (residual one-cell rims).
- OPEN (residual, none blocking): how the target is named to the query; modes 2..4 in written
  obstacles; 26 of 69 army zone boxes one cell off (flood entry points, `0x00B011B0`); the pieces'
  exact shapes; the meaning of the capacity virtuals (`+8` / `+0x48`); a port entity's virtual
  `+0x18` and the third port case (a faction named in the move context); the harbour exit
  (INFERRED); the embarked army commander's saved position.
- Edits to `polypath.rs` for the pathfinding worker to reconcile (visibility only, no behaviour
  change): `PolyMap::cell_rc`, `PolyMap::multiplier`, `PolyMap::goal_step`, `octant_dir`,
  `dist_to_segment`, `dist_to_polygon` made `pub` (two got a doc line). Also `commands.rs`:
  `plan_path` asks `embark` first for embarked armies and for navies starting in a harbour or
  heading into a port (the pathfinding worker's polypath search is otherwise unchanged); `walk`
  moves a navy's passengers; `Walk`, `walk`, `commander_of` are `pub(crate)`.
- Boundary: the pathfinding worker keeps smoothing, flag bits 24..31 and the movement-range zone.

## 1. Coast geometry (CONFIRMED from the data, `tests/ports_install.rs`)
- Sea polygons (kind 1) **never** share an edge with land (kind 0) on any map: between them lies a
  strip of **kind 2** polygons (Europe: 31353 kind-2 polygons; 6966 edges to land, 5040 to sea).
  Kind 2 is also the impassable off-map land (PATHFINDING.md §5), so kind 2 = "impassable for
  everyone", and the coast is a kind-2 strip. Kind 7 (enterable by armies and fleets) touches sea
  (240 edges) and land (1885): the ports, fords and landing places.
- So no ordinary search step ever crosses the coastline: armies and fleets meet only through the
  special nodes below.

## 2. Mover types of a transport query (CONFIRMED structure)
- A path query (`0x00B18F40`, `0x00B18C20`, `0x00B5A080`) takes a **start location with its mover
  type** and a **goal location with its mover type** (mapped by `0x00B29B20`: 6/7/8 → 0/1/2). Naval
  types are 2, 5, 8, 11 (`0x00B4DA70`); a naval start inside a kind-7 polygon becomes 8 (2 → 8, 5 →
  11). INFERRED: 0 army, 1 agent, 2 fleet; 3/4/5 and 9/10/11 the same at a theatre / in context.
- The move context (`0x00AEA2D0`) keeps the goal's mover type at +0xC (unmapped) and +0x10
  (mapped 0 → 6, 1 → 7, 3 → 9, 4 → 10); +4 the start's spent fraction of the turn (`1 − AP left /
  AP max`, of the fleet for a transport), +8 the naval mover's max AP, +0x14 the land mover's max
  AP, +0x18 the **land mover's spent fraction**, +0x24 = max of the two max APs (heuristic
  divisor). So **a fleet carrying an army plans with start type 2 (fleet) and goal type 0 (army)**,
  and the landing nodes it creates carry the goal's mover type.

## 3. Port node (CONFIRMED, `0x00B2CC20` and its twin `0x00B2E020`)
When the current location is naval and its polygon is kind 7, or (while scanning a neighbour cell)
a polygon of kind 7 that is not a normal neighbour: take the polygon's point (`0x00B489E0`, a point
inside the polygon from a scanline sweep, INFERRED), find the map entity there (`0x00974860` +
`0x00BBBF90`); it qualifies when it lies within **√2 map units** (distance² < 2), has a port object
(`+0x1E4`), passes two run-time checks (`0x006649C0`, UNKNOWN) and its virtual `+0x18` is 0
(INFERRED: not besieged / not blockaded), and its owner is the mover's faction, or passes the
relation test `0x008CE9B0` (the "at war" test, §8), or is a special faction
(`0x006F0690`, UNKNOWN). Then **one** node at the entity's position (its virtual `+0x3C`), with the
mover's type; only one port node per expansion (a flag stops the landing scan too).

## 4. Landing node (CONFIRMED)
- For naval movers of type 2 or 8 only, when no port node was added: every polygon of **kind 2 or
  3** in the scanned cell gives its point (`0x00B489E0`), from which `0x00B373F0` looks for a land
  position for the goal's mover type: over the **3 x 3 cells** around the point, every polygon the
  type may stand on (`0x00B41860`: type 0 → kinds 0 and 6; types 6/3/9 → 0, 6, 7; 1 → 0, 6, 8;
  7/4/10 → 0, 6, 7, 8; naval → 1, 4 (and 7); kinds 10/11 with the flag) is tested: the point inside
  it → distance 0, else the nearest point of the polygon; the nearest wins, and it is accepted when
  it is **closer than 1.5 map units** and the final check `0x00B6A0D0` (valid position for the
  mover, INFERRED: not blocked by another force) passes. The node gets the goal's mover type.
- Its step cost (`0x00B11570`): a step between a naval and a land location costs (0, 0), divided
  by the max AP as usual; a step **from naval to land** is replaced by the marker −1, or −2 when the
  naval location's polygon is kind 7. The adder (`0x00B07A70`) turns −1 into **`ceil(max(g, 1)) +
  land spent fraction`** and −2 into **`ceil(g) + 1`** (g = turns so far). So a landing always
  ends the turn; from the open coast it is reached this turn only when the fleet got there within
  its turn and the army has not moved this turn; from a port (kind 7) the landing costs a whole
  extra turn unless the fleet starts there (g = 0).
- After a landing node the search continues with the land mover's polygons (the location's mover
  type is the land type), so the path can go on overland (next turn).

## 5. Embark query (CONFIRMED structure, `0x00B5A470` → `0x00B5A870`)
- An army boarding a fleet plans from the army (land type) to the fleet with its own context
  (`0x00AEAB80`: also maps 2 → 8, 5 → 11) and search variant `0x00ACB660` (cost limit FLT_MAX),
  whose expansion `0x00B2E020` is `0x00B2CC20` plus one extra scan: polygons of **kind 8 or 10**
  (run-time polygons cut around forces, PATHFINDING.md "ZOC") get nodes through `0x00B7B020`
  (INFERRED: the fleet's own zone, so the land path ends at the fleet). The embark point is the
  first location of the path whose mover type is naval (2/8) after a land one: a kind-7 polygon →
  the port entity there, else that position. Spent fractions passed: the army's and the fleet's.
- The embark command (`CCQ_EMBARK_NAVY`, handler `0x00934820` → `0x00948980` → `0x0091A560`) checks
  the army can act (`0x008BE360`), then compares **the fleet's units + the army's units with the
  fleet's capacity** (fleet virtual +0x48 vs +8; value UNKNOWN) before boarding.
  Disembark: `CCQ_DISEMBARK_NAVY` (`0x00934720` → `0x009315C0`).
- Map cursor names (CONFIRMED strings, `0x00DE55F0`): `land_can_disembark`,
  `land_cannot_disembark`, `player_port`, `non_player_port_occupied/unoccupied`.

## 6. sea_grids.esf and the "same component" check
- `sea_grids.esf` is the campaign AI's coarse sea-zone graph (CAMPAIGN_DATA.md §2, `CAI_SEA_GRID_*`);
  the path search does not read it (no reference from the pathfinder code found). Its zones list
  the port slots (`port:<region>:<town>`) and land regions each sea cell touches: what the AI's
  transport planning uses to pick landing regions (not decoded further).
- The component pre-check (PATHFINDING.md §1) compares the start's and the goal's records; for a
  transport the start is naval and the goal is land, so the components differ by construction:
  INFERRED that the transport queries skip it (they call the searches directly, `0x00B18F40` and
  `0x00B5A870` do not call `0x00AA0FD0`).

## 7. Our port (`ntw_sim::campaign::embark`)
- **Transport search** `find_transport_path(pm, from, to, goal mover, ports, roads, Transport)`: an
  A* like `polypath` (same step costs, ordering, update rule and heuristic, the divisor being the
  larger max AP, CONFIRMED) over naval and land locations, plus the special nodes: port nodes (§3),
  landing nodes (§4, cached per coastal polygon) and, INFERRED, harbour exits (from a naval
  location in a port's kind 7 polygons to the sea polygons its footprint reaches, §8). Costs in **turns**: naval
  steps / the fleet's max AP, land steps / the army's, start = the fleet's spent fraction, landing
  = `ceil(max(g, 1)) + army spent` or `ceil(g) + 1` from a kind 7 polygon (CONFIRMED).
- **Landing position** `land_position`: nearest point of a kind 0 / 6 polygon in the 3 x 3 cells,
  `< 1.5` units (CONFIRMED); the coastal polygon's point and the valid-position check as the original (§8, CONFIRMED).
- **Embark points** (PROVISIONAL): the landing positions of the coastal polygons around the
  fleet, and the fleet's own position in a kind 7 polygon (a fleet in port).
- **Commands**: `Embark { force, navy }` (the army walks to the cheapest embark point, then boards;
  boarding itself spends no AP, §8), `Disembark { force, to }` / `MoveForce` of an embarked
  army (the fleet sails as far as its AP and the turn rule allow; the army lands when the landing
  costs ≤ 1 turn, i.e. the fleet got there this turn and the army has not moved; landing leaves it
  with 0 AP). One army per navy, a second joins it within 20 units (§8). Embarked armies cannot attack,
  merge or enter settlements before landing. Ports are nodes for their owner and its
  enemies (§8, CONFIRMED).
- Data check (eur_napoleon, `embark_campaign.rs`): both French fleets start at sea away from any
  coast; the port polygons (Genoa, Toulon, Nantes, Brest, Bastia) are kind 7 groups of 2 to 4
  polygons that touch no sea polygon (their nearest sea vertex lies 1.4 to 1.7 units away), each
  port slot lying 0.2 to 0.6 units from its polygon's point (so within the √2 port radius).

## 8. Round 2: closing the PROVISIONAL parts (2026-10-04)
- **Polygon point `0x00B489E0` (CONFIRMED, ported `polygon_point`).** A scanline sweep over the
  polygon's non-horizontal edges, from the lowest z upwards, level by level (the edges' end
  points, edges sorted by their low z, `0x00AC3450`): at each level the edges ending there are
  dropped and those starting there added, their x at the level computed with rounding
  (`x_low + ((z − z_low)·(x_high − x_low) ± dz/2) / dz` in 20-bit fixed point), sorted ascending
  (`0x00AC3240`), leading pairs with equal x dropped; the first level that keeps two different x
  returns **(smallest x, level)**. A fallback sweep steps z by one fixed-point unit; (0, 0) if
  nothing. So the point lies **on the outline** (the left end of the lowest stretch with width),
  not inside. With it every port slot of the 4 maps lies within 0.8 units of its kind 7 polygon's
  point, inside the √2 port radius.
- **Landing valid-position check (CONFIRMED, ported in `land_position`).** `0x00B373F0` →
  `0x00B6A0D0` → `0x00B6A1F0`: the location at the point is built (`0x00B53700` → `0x00B53930`:
  a run-time polygon there if one fits the mover, else **the polygon containing the point**,
  `0x00B1FA60`, which must be a kind the mover type may stand on, `0x00B5AE50`); if invalid,
  `0x00B38FC0` looks for the nearest valid position within **0.001** units (`0x3A83126F`). Before
  the test the moving force's own run-time polygons are taken out (`0x00AF5A80`). We keep the
  nearest point if a land polygon contains it, else move it 0.001 into its land polygon; run-time
  polygons are not in our model.
- **Port relation (CONFIRMED).** `0x008CE9B0(a, b)` is false for a == b, true when either faction
  has `+0x514 == 0` (no diplomacy object; INFERRED the rebels), else it reads the pair's
  relationship record (`0x00B64C50`, 0x848 bytes per pair; `+0x790 → +0x10 == 0`). The Lua binding
  `force_declare_war` (table `0x01458258` → `0x0097A930`) declares war only when it is false, so it
  is the **at war** test. The port node therefore exists when the port's owner is the mover's
  faction, at war with it, or (third case, `0x006F0690` on the move context, UNKNOWN) a faction the
  caller names. Further conditions (CONFIRMED listing `0x00B2CD60..`): the port is the goal or the
  move context's `+0x60` is empty, the context's `+0x68` is empty, and the port entity's virtual
  `+0x18` is 0. Ported: open = own or at war (`CampaignModel::at_war`: stance war, or either
  faction the rebels).
- **Kind 7 entry (CONFIRMED `0x00B16A30`).** For mover types 0, 1, 2, 6, 7, 8 an ordinary step
  from a polygon of another kind into a kind 7 polygon is refused; types 3, 4, 5, 9, 10, 11 may.
  Settlements lie on kind 7 (e.g. Corsica's), so the movers that reach settlements must be of the
  second family (INFERRED); which family each kind of character uses is UNKNOWN, so neither
  `polypath` nor `embark` applies the block (noted for the pathfinding worker).
  **Superseded by §9.2:** characters move as family A unless inside a building; the rule is now
  applied to land movers (`movers`).
- **Leaving a harbour (INFERRED, ported `Harbour`, `harbour_exits`).** A port's kind 7 polygons
  form groups of 2 to 4 polygons that touch only land and the coastal strip (CONFIRMED on all 5
  maps); nothing in `pathfinding.esf` links them to the sea, and no fleet stands in a port in any
  start position. The `regions.esf` port slot (`slot_descriptions`) carries, besides its position,
  a Coord2d about 1.3 units seawards (only port slots have a distinct one; it lies in the second
  outline) and three Coord2d outlines; the third (the union of the other two) reaches into a sea
  polygon at **every one of the 67 ports** of the 4 maps with ports, the first (the land part)
  never (CONFIRMED data). INFERRED: the port's footprint is cut into the grid at run time (the
  pathfinder's run-time polygons, kinds 8..11; the cutting code was not found: the candidates
  around the pathfinder pool `+0x124` and the cutter `0x00B49A80` lead to the battle-outcome code
  `0x00B4F090`) and the fleet leaves through it. Ported: from a naval location in kind 7 near a
  port (within √2) or inside its footprint, the sea polygons the footprint overlaps are neighbours.
  `MapSlot::dock` / `footprints` read the slot data; `ntw_campaign::pathing::harbours` builds them.
- **Capacity and boarding (CONFIRMED comparison, INFERRED meaning).** In `0x0091A560` (reached
  from `CCQ_EMBARK_NAVY` → `0x00934820` → `0x00948980`): when the navy already holds an army
  (`0x009D3A80` → `+0xD8`), the units of that army (virtual `+8`) plus the boarding army's
  (`+0xC` of the order's force record) are compared with the held army's virtual `+0x48`; above it
  the order takes another branch (`0x00885F30`, not decoded), else the armies are joined
  (`0x008F17D0` / `0x008D47B0`). INFERRED: one army per fleet, a second one joins it when both fit
  in one army (20 units). No action-point write was found on the boarding branch (INFERRED:
  boarding itself costs nothing; the walk to the fleet does, and the landing rule then keeps an
  army that walked this turn aboard until the next turn). Ported accordingly
  (`CampaignModel::embark`).

## 9. Round 3: the run-time cutter, mover families, the saved transport (2026-10-04)
### 9.1 The run-time polygon cutter (CONFIRMED structure)
- Entry points (SAVE_COMPAT.md §6): the refresh `0x00B1B7A0` removes a character's obstacle
  (`0x00B654C0`: look-up by key `+0xCC | 0x80000000`, take its pieces out of every cell, mark the
  character `+0x16C = 2`) and re-adds it (`0x00B08610` → `0x00B087A0` → insert `0x00B09030`) when
  the character moved off its old bbox or its outline changed (`0x00B4CF70` on `+0x1C8` when it
  has a zone, else `+0x1D8`). `0x00B54900` is the same removal for a search (the mover's own
  obstacle), restoring the pieces afterwards (`0x00B78C60`).
- Insert `0x00B09030` and the per-faction mode switch `0x00B69C20` do the same thing per **layer**:
  the obstacle's shapes are queued as (outline, kind) records (`0x00B5DF10`, 0x14 bytes each):
  mode 0 = the **zone** (virtual `+0x1C`) as kind **8** plus the **core** (virtual `+0x20`) as the
  obstacle's kind field (`+0x90`: 9 for characters, 7 for map-slot entities); mode 1 = the core
  only; an obstacle without a zone uses mode 1 everywhere. Modes 3 / 4 go to `0x00B545B0` /
  `0x00B54B00` (not decoded). The queued shapes' bbox gives the cell range (`0x00B0C240`, clamped
  to the grid), and `0x00B0C3A0` → `0x00B0C4E0` rebuilds every cell in it:
  - the cell's polygons are copied on first change (copy-on-write, `0x00B0E820`); a cell on the
    range border is clipped, inner cells too unless the border flags say they are fully covered;
  - **clip** `0x00B495D0`: fast path when the queue is a single 4-point shape of a kind other than
    12: every polygon of the cell **except kinds 2, 3, 5, 7, 9, 11** simply takes the shape's kind
    (`0x00B789C0` writes flags & 0xF). Otherwise a general polygon clipper (static object
    `0x015F0DA8`: `0x00AC4100` adds each cell polygon, `0x00AC3A10` each shape; `0x00AECC00` tags
    the cell polygons with `(region id << 15 | kind) * 2` and the shapes with their kinds;
    `0x00B309C0` runs it; `0x00ADEAB0`, 8.8 KB, extracts the pieces) cuts the cell along the shape
    outlines. Which kind a piece gets where shape and polygon overlap is decided inside
    `0x00ADEAB0` (not decoded); INFERRED from the fast path: the shape's kind, except over the
    protected kinds above;
  - the new pieces are linked into the pathfinder's lists (`0x00B09570`) and, after the whole
    range, every rebuilt cell's polygons are re-linked to their neighbours in the 3 x 3 cells
    around (`0x00B79460` → `0x00B79280` → `0x00B69FD0`, edge test `0x00B16800`, direction bits
    in the polygon's flags) for the polygon kinds 0, 1, 4..11, using the **kind link table**
    `0x00B573C0` (CONFIRMED): 0 / 6 link to 0, 6, 8, 10, 11, 7, 5; 1 to 1, 10, 11, 7, 4; 4 to
    0, 6, 1, 10, 11, 7, 5, 4; 5 to 0, 6, 5, 4, 10, 11; 7 to 0, 6, 1, 8, 10, 11, 7; 8 to 0, 6, 1, 8,
    10, 11, 7; 10 to 10, 11, 7; 11 to 11; 2, 3 and 9 to nothing.
- **Character obstacles** keep their two shapes in the character: zone `+0x1C8` (the flood of
  PATHFINDING.md §8), core `+0x1D8` (CONFIRMED `0x009CC0E0`: a 24-gon of radius 1, ported by the
  pathfinding worker as `zoc::CORE_RADIUS`, whole polygons blocked: PROVISIONAL until polygons are
  cut).
- **Map-slot entities** (`0x00B08D60`, kind field 7, core = entity `+0x1C4`, zone = the entity
  virtual `+0x20` → `+0x1B8`) matter to a mover only when it is **not a fleet** and stands within
  150 units (slot 4 `0x00B57310`; the fleet test `0x009D0F80` is decoded in 9.2). So the building
  footprints cut as kind 7 concern armies and agents. **Correction to §8 / round 2:** they cannot
  be how a fleet leaves a harbour. The harbour exit therefore stays INFERRED (`Harbour`,
  `harbour_exits`, unchanged): no fleet-side run-time cut of the port footprint was found, and
  fleets reach a port through the port node (§3), which is CONFIRMED.
- Not ported: cutting polygons at run time. Our zones of control block whole polygons
  (`zoc`, PROVISIONAL); the next step would be a clipper in `PolyMap` with run-time pieces.

### 9.2 Mover families (CONFIRMED)
- Four getters on the moving character's pathfinding wrapper (character at `+0x20`) give the mover
  type, each a triple (army, other land character, fleet); "land" is the wrapper's virtual `+0x24`,
  "army" is `0x009D0F50` (the character's type record `+0x1AC` has `+0x18` set and an object at
  `+0x4C`), the other test `0x009D0F80` (`+0x58`) is the fleet one:
  `0x0095E3D0` → 0 / 1 / 2 (or 3 / 4 / 5 when the character's byte `+0x4E8` is set);
  `0x0095E4C0` → 3 / 4 / 5; `0x0095E4F0` → 9 / 10 / 11; `0x0095E520` → 6 / 7 / 8.
  The kind table (CAMPAIGN_DATA.md §11) agrees: types ≡ 0 mod 3 may not enter kind 8 (enemy
  zones), ≡ 1 may (agents cross zones), ≡ 2 are naval.
- `+0x4E8` is set by `0x009DA300`, which also looks up the building slot of the character's region
  within 1 unit of it, and cleared by `0x00A0C810` (INFERRED: "inside a building", i.e. in a
  settlement, fort or port).
- Who passes what: the move-to-a-character query `0x00B21510` → `0x00B18F40` gives the **mover**
  its `0x0095E520` type (6 / 7 / 8) and the goal the **target's** `0x0095E4C0` type (3 / 4 / 5); the
  other query builders (`0x00B22890`, `0x00B20B20`, the `0x00B5A080` family `0x00B594A0..0x00B59EA0`
  and many more) take the mover's type from `0x0095E3D0`. Search nodes inherit the type of the
  node they are expanded from (`0x00B2CC20`: the neighbour location gets the current location
  `+0x18`; `0x00AF3520` only turns 9 / 10 / 11 into 3 / 4 / 5 off kind 7). So **every character
  moves as family A** (types 0..2 / 6..8) **unless it starts inside a building** (family B).
- Data (install test `movers_install.rs`): every settlement of the 5 maps lies on kind 7, kind 7
  never joins two land areas (no bridge role: removing it leaves the land connected as before),
  and only 29 of the 72 Europe settlements have a land polygon next to the settlement's own
  polygon (the rest sit inside multi-polygon footprints).
- So for family A the kind-7 rule means: walk round every building footprint. How the original then
  enters the settlement it is ordered into is UNKNOWN (the order code snaps the character onto the
  slot, `0x009DA300`; the query's goal side was not followed further).
- **Ported** (`ntw_sim::campaign::movers`, used by `plan_path`): land movers outside a building
  (not garrisoned) are family A; the kind 7 polygons of their start's and their goal's footprint
  stay open (the goal part PROVISIONAL), every other kind 7 polygon is closed (through the same
  blocked-set hook as the zones of control, so `polypath` is unchanged). Garrisoned movers are
  family B. Fleets: not applied (PROVISIONAL: 227 Europe kind-7 polygons touch the sea; our fleets
  enter ports through `embark`'s port nodes). Test: `movers_install.rs::every_settlement_stays_reachable`
  (156 settlements on the 5 maps reached from another settlement's doorstep without crossing any
  other footprint; 10 are alone in their land area).
- **For the pathfinding worker (zone limits look swapped).** `0x00A2A1C0` gives **6** for the army
  test `0x009D0F50` and **12** for the fleet test `0x009D0F80`. The mover-type getters above fix
  which test is the army one (it yields the type that may not enter enemy zones), and slot 4 of
  the map-slot obstacles (`0x00B57310`) skips `0x009D0F80` characters for land buildings, which only
  makes sense for fleets. The saves agree: the obstacle bbox is the outline bbox rounded to
  2 units plus 4 on each side (an agent's 1-unit core gives a 12-wide box), and in the Great Britain
  saves army commanders' boxes are at most 26 wide (zone half-width at most 9 = 6 plus polygon
  overhang) while fleets reach 34 (about 12 + 1). So: **army zone 6, fleet zone 12**, not 12 / 6 as
  in PATHFINDING.md §8 and `zoc` (not changed here: their module).
  **Evidence note (round 5):** the Great Britain saves used here were made with the NTW3 mod and
  are no evidence (user rule); the limits are CONFIRMED on the vanilla eur_napoleon start position
  instead (§10.1, `zoc_install.rs`).

### 9.3 An army aboard a fleet in the save (CONFIRMED structure)
- (Round 5: the six Great Britain saves and `quick_save` are NTW3-modded, no evidence; the vanilla
  saves of SAVE_COMPAT.md §1 hold no embarked army either.)
- None of the 9 recovered original saves (`NR-save-compat\target\tmp\original\`: the six Great
  Britain saves, `auto_save`, `quick_save`, `NR-A new campaign turn1`) holds an embarked army: every
  link field below is 0 and no fleet holds land units. The encoding comes from the code.
- `NAVY` v1 (loader `0x008822F0`, writer `0x008FAB60`): #0 `MILITARY_FORCE`, #1 `UNITS_ARRAY`,
  #2 / #3 u32[] (`+0xB0`, `+0xC4`), **#4 u32 = the force id of the army aboard** (link `+0xD8`;
  0 = none), #5 `THEATRE_TRANSITION_INFO` (`+0xF0`), #6 u32 = another link (`+0xE8`, UNKNOWN).
- `ARMY` v2 (loader `0x00870FD0`): #0..#3 as above, #4 i32 (the force id again), #5 u32 a link
  (`0x008CF990`, UNKNOWN), #6 bool (creates a 0x88-byte object when true), an optional record,
  **#7 u32 = the force id of the fleet carrying it** (link `+0xEC`; 0 = none), #8 bool (`+0xE8`,
  only from version 2).
- Both are one two-way link (`0x008CF920` joins two link slots only when both are free): army
  `+0xEC` ↔ navy `+0xD8`. Ids are resolved through the global id map `0x0105AC60`, in which each
  force registers under its `MILITARY_FORCE` #0 id while loading. The embark order reads navy
  `+0xD8` to find the army already aboard (`0x00918870`, `0x0091A560`). The units stay in the
  `ARMY` record; nothing is copied into the fleet.
- The embarked army commander's position in a save: UNKNOWN (no example).
- **Ported (loader):** `ntw_campaign::world` reads both fields (`transport_link`) and fills
  `World::embarked` from them; the old position rule stays as a fallback for our own saves until
  the writer stores the link (PROVISIONAL). **For the save-compat worker (through the manager):**
  write NAVY #4 = the carried army's force id and that ARMY's #7 = the fleet's force id (both 0
  otherwise) for every `World::embarked` pair.

## 10. Round 4: zone limits, the cutter ported, kind 7 rules, obstacle records (2026-10-04)
`zoc` (zones of control) is owned here since this round.

### 10.1 Zone limits (CONFIRMED, fixed)
- Army **6**, navy **12** map units (+2 inside a settlement), as §9.2 argued. Check against the
  original's own data (`ntw_campaign` test `zoc_install.rs`, eur_napoleon start position): with
  these limits our flood gives exactly the saved obstacle box for 43 of 69 army commanders and 17
  of 18 navy commanders; with the limits swapped, for none. The other 26 armies (mostly garrisoned)
  differ by one cell at one edge (cause UNKNOWN: kind 7 rules in the flood were tried, no change).
- Relevance (vtable slot 4 `0x00B57260`): both navies or both not, so an agent meets armies, not
  navies (corrected; agents met everything before).

### 10.2 The run-time cutter, ported (`ntw_sim::campaign::rtcut`)
- **Overlap kinds (CONFIRMED `0x00B13520`, called per output face from the extractor
  `0x00ADEAB0` via `0x00ACDB80`)**: each face of the clipped cell knows the cell polygon it lies in
  and the shapes covering it. A face of a polygon of kind 2, 3, 5, 7, 9 or 11, or covered by no
  shape, keeps the polygon's kind; otherwise the **last** covering shape gives the kind, skipping
  kind 12 shapes (transparent; all 12 = the polygon's kind); kind 10 over a kind 8 polygon stays 8.
  The extractor also merges back the faces of one kind 2, 3, 5 or 7 polygon (`& 0xFFFE` tests on
  the tags), so those polygons keep their shape. The first face of a polygon reuses its slot
  (`0x00B789C0` / `0x00B78960`), further faces are new polygons with the same region id.
  Self-touching outlines are split into simple polygons first (`0x00ACDB80`).
- Ours: `rtcut::build(map, cuts)` → `polypath::Overlay` (base polygons keep their index; new
  kind/outline/links for changed ones; extra pieces appended per cell); searched through
  `polypath::View` (`PolyMap::with(&overlay)`), which the A*, the smoothing (`polysmooth::smooth_view`)
  and `locate` read. Shapes: `Shape::Polygons` (a zone: its flood's polygons simply change kind,
  since the original's zone outline is the outline of exactly those polygons) and `Shape::Convex`
  (a core: clipped exactly; the outside part as convex wedges along the core's edge lines, concave
  cell polygons triangulated first). Then every new vertex is inserted into the edges it lies on
  (no T-junctions, so the smoothing's shared-edge links hold), and the rebuilt cells are re-linked
  to the 3 x 3 cells around them with an edge-overlap test and the kind link table (§9.1); static
  polygons next to them keep their other links. Cells whose polygons only change kind are not
  rebuilt. PROVISIONAL: the piece geometry (the original's clipper output beyond kinds is not
  decoded), the diagonal link rule kept from the static map.
- `zoc::overlay` builds the cuts per search (`zoc::cuts`: at war → zone kind 8 + core kind 9,
  else core; agents: no zones, they may enter kind 8 anyway, new `Mover::Agent`); `plan_path`
  searches `pm.with(&overlay)`; the path's polygons are reported as the static ones they were cut
  from. A goal held by a cut ends at the nearest free polygon (as before).
- Cost: about 10 ms per plan on Europe with ~65 cores in range; 3 AI End Turns of eur_napoleon
  take 2.56 s in the `determinism` harness (2.46 s before this round, load included).

### 10.3 The kind 7 rules (CONFIRMED)
- Two checks run on the current node L and each neighbour polygon N, in every neighbour expansion
  of every search variant (`0x00B2CC20`, `0x00B2E020`, `0x00B2F4E0`, `0x00B2AB40`, `0x00B2C0A0`,
  `0x00AE3DD0`, `0x00AE4740`; registers checked in the listing):
  `0x00B16A30`: types 0, 1, 2, 6, 7, 8 outside kind 7 may not step into kind 7;
  `0x00B167B0`: types 3, 4, 5 inside kind 7 may not step out of it (9, 10, 11 may; outside kind 7
  they become 3, 4, 5, `0x00AF3520`, and nothing turns them back).
- So family A never enters a building footprint from outside, family B can enter one but not
  leave it: either way the usable kind 7 polygons are the start's group plus, for family B, the
  group where the path ends. Data: kind 7 never joins two land areas nor two seas
  (`movers_install.rs::closing_kind_7_splits_nothing`, 5 maps).
- How a family A mover enters the settlement it is ordered into: no search can take it there.
  The move-to-a-character and the other query builders give the mover family A unless it is in a
  building, and the goal side the target's family B type, but node types come from the start.
  UNKNOWN; INFERRED: the order code moves the character in (`0x009DA300` snaps it onto the slot).
  Ours (unchanged): the goal's kind 7 group stays open (CONFIRMED-equivalent for family B,
  PROVISIONAL for family A).
- **Fleets**: the same rules (types 2 / 8 are family A): fleets never sail into kind 7 from the
  sea; they enter ports only through the port node (§3, CONFIRMED), which `embark` provides. Now
  applied to every mover in `plan_path` (`movers` module).

### 10.4 Obstacle records for the save writer (`zoc::obstacle_record`)
- `obstacle_record(map, pos, Owner::{Army, Navy, Agent}, garrisoned) -> ObstacleRecord` gives what
  a `CHARACTER_OBSTACLE[]/OBSTACLE` record needs; CONFIRMED against every obstacle of the
  eur_napoleon start position (`zoc_install.rs`: 69 armies, 18 navies, 8 agents):
  - `#1..#4` = `mode` (the obstacle's last mode state `+0x64..+0x70`; start positions hold
    1, 0, 0, 1 for commanders and 5, 1, 0, 1 for agents; INFERRED: the next search resets them);
  - `#6` = `kind` = 9;
  - `#11..#14` = `zone_cells`, `#15..#18` = `core_cells`: the cell ranges the two cut layers
    rebuild (`0x00B09030`: layer 0 = zone and core, layer 1 = core), each by the rule of
    `0x00B0C240` on the shapes' bounding box: from the cell of (min − one cell + one Fixed20 unit)
    to the cell of max plus one (a zone's max on a cell edge stays in its cell), clamped to the
    grid; `zone_cells` is all 0 without a zone (agents). Core ranges: 95 of 95 equal; zone ranges
    equal wherever the flood matches;
  - `#7..#10` = `bbox` (Fixed20 in the save): the zone range (the core range without a zone)
    widened by one more cell: x0 = origin + (col0 − 1)·cell, x1 = origin + (col1 + 2)·cell
    (equal for all 8 agents and every matching flood);
  - the core: `core` (`zoc::core_shape`: 24 points radius 1, or the 0.25 triangle for a character
    inside a settlement, CONFIRMED by the core ranges of all garrisoned commanders), `zone` (the
    zone's static polygons) and `pieces` (the run-time polygons the core cuts: cell, source
    polygon, kind, outline) for the boundary pools.
- Not decoded here: `#0` (six boundary slots, 0x80000000 | run-time boundary index) and how the
  run-time boundaries are stored in `PATHFINDING_GRID` (`#1` pool, `OBSTACLE_BOUNDARIES`,
  `OBSTACLE_BOUNDARY_MANAGER`, `OBSTACLE_BASE_GRID_NODE[]`, the `MANAGED_OBSTACLE_BOUNDARY` pairs of
  SAVE_COMPAT.md §6). A writer that leaves a new commander without an obstacle stays the safe
  choice until those are decoded (the original creates it on the next refresh, SAVE_COMPAT.md §6).
  **Superseded by §11** (round 5): all of these decoded; `grid_obstacle` writes them.

## 11. Round 5: how the original stores obstacles, and writing one (2026-10-04)
Evidence: the vanilla start positions (install) and the vanilla saves of SAVE_COMPAT.md §1 only
(copies in `target/tmp/vanilla`); code: the grid writer `0x00AFE020` and loader `0x00AF89C0`.

### 11.1 `PATHFINDING_GRID[0]` (CONFIRMED)
- `#0` u32 = the number of run-time pieces; `#1` u32[] = the pieces, each `n`, `n` Fixed20 (x, z)
  points, then the number of cell versions that use it (writer: pool `+0x120/+0x124`, 0x18-byte
  items; the count equals the references in every vanilla save; unused pieces stay with 0).
- `#3 OBSTACLE_BOUNDARIES` {u32[]}: the **cell versions** (`0x00AFED10`, list `+0xA4`): `n`,
  `n` x (flags, link), the cell key (col | row << 16), a byte (always 0) as u32. A link is a static
  boundary's own link word (region << 22 | outline list offset; flags and link as in
  `pathfinding.esf`, `Boundary`) or region << 22 | 0x200000 | piece index (all 7 991 piece links of
  `auto_nr4_t4` lie in their version's cell). A polygon a cut covers wholly keeps its static link
  and only its kind changes (e.g. kind 8 over land / sea, 9 over land); split polygons become
  pieces (on both sides of the cut), and so do protected ones the cut crosses (kind kept).
- **Flags** of every polygon of a version are recomputed (`0x00B79280` → `0x00B69FD0`): the kind
  (bits 0..3); for the 4 side cells and the cell itself a 4-bit mask of the polygons it shares an
  edge with, counted among that cell's polygons the kind link table (§9.1) allows (bits 4..23 in
  the order S, W, self, E, N; index ≥ 4 not recorded); a bit per search direction with any such
  neighbour (bits 24..31, `dir_index` order); diagonals need the same kind (not 2) and the shared
  corner. Recomputing the static flags of nap_europe from its geometry this way gives the file's
  value for 103 321 of 103 563 boundaries (the rest: kinds 4 and 7 at a few odd spots).
- `#4 OBSTACLE_BASE_GRID_NODE[]` (`0x00B00310`, reader `0x00AEE2E0`): one node per cell with
  versions: u32 key = the cell's **first static boundary index** (= our `PolyMap::cell_first`,
  11 568 of 11 568), u32 = the cell's static boundary count (100 %), list 1 =
  `MANAGED_OBSTACLE_BOUNDARY` rows {u32 version index, u32 x, bool true, u32[] pair list}, list 2
  = rows {bool true, pair list} (the same lists, in another order: the loader keeps them as a
  linked list `+0x2C`, list 1 as a map pair list → (version, x)). `#5` u32[] = (cell key, node
  index) for every node.
- A **pair list** names the obstacle layers a version applies: pairs (character id | 2, slot).
  `x` = how many of them have the cell only in their ring (CONFIRMED: equals the obstacle's
  BOUNDARIES high bit on every single-pair row of 4 vanilla files and a start position). Versions
  for combinations of several obstacles exist (made by the runtime when several are active).
- `#2 OBSTACLE_BOUNDARY_MANAGER`: the set of pair lists (SAVE_COMPAT.md §19).
- The `OBSTACLE` record: `BOUNDARIES` (6 slots) slot k = the versions of the obstacle's layer k in
  row-major order over its cell range widened by one cell; the ring versions (re-linked only) carry
  the high bit (0x80000000); `MANAGED_OBSTACLE_BOUNDARY` slot k = {true, the layer's pair list}
  when the layer exists, else {false}. Slots = modes of `0x00B69C20`: 0 = zone and core, 1 = core,
  2..4 other kind mappings (kinds 10 / 11 appear only in slot 2 and 4 versions; built from
  existing versions by re-kinding, `0x00B545B0` / `0x00B54B00`; 15 of 95 start-position obstacles
  have one; not decoded further). The mode state `#1..#4`: commanders mostly 1, 0, 0, 1 and agents
  1, 1, 1, 1 in the start positions. `#11..#14` / `#15..#18` = the cell ranges of slots 0 / 1.
- **The zone shape** (`0x00B7BAF0`): the flood runs with the other obstacles cut in for the
  character (`0x00AF5200` ... `0x00AF7A00`), so its outline can follow other cores (arcs in zone
  versions). Our flood on the static map gives the saved zone range for 60 of 87 commanders; with
  the other obstacles cut in (our `rtcut`), 59: the remaining differences are UNKNOWN (half-step,
  strictness, garrison bonus and first-step variants tried; none fits better).

### 11.2 Writing an obstacle (`ntw_campaign::grid_obstacle`)
- `add_character_obstacle(grid, &StaticGrid { area, cells, map }, &NewObstacle { character, pos
  (Fixed20), owner, garrisoned })` appends to `PATHFINDING_GRID[0]` everything the original stores
  for a freshly made obstacle: per layer (slot 0 when there is a zone, slot 1 always) a version for
  every cell of its range (cut: zone polygons re-kinded to 8, the core clipped, `rtcut::cut_polygon`)
  and of the ring (static geometry), flags recomputed by the rule above, new pieces with their use
  count, the grid nodes (created when missing, `#5` updated), list 1 / list 2 rows, the manager's
  pair lists, the `OBSTACLE` record (`zoc::obstacle_record` for box, ranges and mode state) and the
  `OBSTACLE_LISTS` entries. The caller removes an existing obstacle of the character first (the
  save writer's `obstacles.rs` does that).
- Tests (`grid_obstacle_install.rs`): three new obstacles (army, agent, garrisoned army) on the
  eur_napoleon start position add no `save_check` violation and survive writing and reading the
  file; rebuilding every one of the 95 start-position obstacles gives the same cells and rings
  (slot by slot, ring bits included) for 68 (all 8 agents, 17 of 18 fleets, 43 of 69 armies: the
  zone floods of 11.1) and the same polygon kinds in 81 % of the cut versions compared.
- PROVISIONAL: the pieces' shapes (our cut: the inside of the core, and at most four parts left,
  right, above and below it; the original's clipper output beyond kinds is not decoded),
  protected polygons crossed by a cut keep their static outline (the original makes them pieces
  with the crossing points), no combined versions with other obstacles (the runtime makes those
  when needed), modes 2..4 not written (as 80 of 95 start-position obstacles).
- Piece winding (fix, 2026-10-04): every used piece of the original's saves has 3+ points and runs
  counter-clockwise (CONFIRMED by the save worker in 11 vanilla saves, a `save_check` rule). Snapping
  a cut part's points to the exact polygon / core points could fold a hair-thin part over: the NR-10
  trial wrote two clockwise 3-point slivers (doubled area -1 515 703 Fixed20², about 1.4e-6 map
  units²). The cut now drops a part that is flat or clockwise with a doubled area under 1e-4 map
  units² (PROVISIONAL threshold) and would turn a larger clockwise part round (none seen); every
  counter-clockwise part is kept. NR-10 with obstacles for its 9 new commanders: no new
  `save_check` violation, 0 bad pieces; same cells and rings as before (57/82 on `auto_nr4_t4`).
  Test: `grid_obstacle_install` checks every written piece (the start-position rebuild wrote one
  clockwise piece before the fix).
- For the save-compat worker: call it for a new commander (army / navy: `Owner::Army` /
  `Owner::Navy`, garrisoned when inside a settlement) after removing any old obstacle of the
  character, with the map's `pathfinding.esf` area, `GridData::expand()` cells and
  `pathing::poly_map` built from them.

### 11.3 The §10 open items
- **Family A entering its target settlement**: not found. The other query builder `0x00B20B20`
  (from `0x00923760`) gives both ends the `0x0095E3D0` type; the target's obstacle removal
  (`0x00AF5930`) handles character obstacles only. UNKNOWN; ours unchanged (goal footprint open).
- **Modes 3 / 4**: slots 3 / 4 (11.1): chosen by `0x00B3F4F0` (faction `+0x6C`, `0x0047B940`) for
  factions at war; versions re-kinded from existing ones. Not ported (our searches use modes 0 / 1).
- **The one-cell-off zones**: see 11.1 (UNKNOWN after the variants tried).
- **Harbour exit**: no fleet stands at a port slot or its dock in any vanilla start position or
  save (one at a port slot in `auto_b2b3_0211`, from our own recruitment); unchanged (INFERRED).
- **Embarked commander's position**: no vanilla save holds an embarked army; UNKNOWN.

## 12. Round 6: modes 3 / 4 (2026-10-04)
- **Who picks a mode** (CONFIRMED): per search, `0x00B3F4F0` gives every obstacle mode 5 (hidden),
  1 (own faction or not at war) or, at war, the obstacle's own field `+0x6C` (`OBSTACLE` #3: 0 for
  every commander with a zone in the vanilla start positions, 1 for agents). Then two helpers
  change modes around the ends of the search:
  - `0x00AF5A80` / `0x00AF59B0` → `0x00B65FC0` at the **start**: an obstacle whose zone shape holds
    the start point goes from mode 0 or 4 to 1 (zone dropped, core kept), from 2 to 3;
  - `0x00AF6460` → `0x00B551A0` at the **goal**: an obstacle in mode 0 whose zone holds the goal
    point goes to mode 4;
  - `0x00AF7B70` → `0x00B66B30` puts them back afterwards.
- **Modes 3 and 4** (CONFIRMED, `0x00B69C20`): mode 4 = the zone layer's versions (slot 0) copied
  with kind 8 → 10 (`0x00B54B00` → `0x00B54AE0`; the core keeps 9); mode 3 = the core layer's
  versions (slot 1) copied with 8 → 10 and 9 → 11 (`0x00B545B0` → `0x00B54570`). By the kind link
  table every mover may enter 10 and 11, but from 10 only 10, 11 and 7 follow, from 11 only 11: a
  path may end inside such a zone (or core), never cross it.
- What sets mode 2 (the source of mode 3) is not decoded; 2 is not built by `0x00B69C20`.
- **Ported** (`zoc::cuts`): an enemy zone holding the mover's start is left out (was PROVISIONAL,
  now CONFIRMED as mode 1); an enemy zone holding the goal is cut as kind 10 (mode 4,
  `zoc::GOAL_ZONE_KIND`), replacing the PROVISIONAL "stop at the nearest free polygon"; the search
  (`View::find_path_avoiding`) applies the link table out of kind 10 / 11 polygons. Mode 3 is not
  used (mode 2 unknown). Tests: `zoc.rs` (into a goal zone and not across it),
  `campaign_play.rs::paths_bend_round_enemy_zones` (a goal 4 units from an enemy: the path walks in
  and does not leave the zone again).
- **Mode 2 (time-boxed search, round 7):** no mode picker returns 2: the obstacle field `+0x6C`
  (`OBSTACLE` #3) is 0 or 1 in every vanilla start position and save, and its only writers set 0
  (constructors `0x00AE8530` / `0x00AE8790`, the latter also copies it) or 1 (`0x00B09030`, no zone).
  The restore after a search maps 1 → 0 and 3 → 2 (`0x00B66B30`: DEC / NEG / SBB / AND 2), the start
  helper 0 / 4 → 1 and 2 → 3, so 2 and 3 are a pair like 0 and 1. Slot 2 is built outside
  `0x00B69C20` by `0x00B0D9C0`, which calls the mode 3 re-kinding `0x00B545B0` and is reached from
  `0x00B54900` (the mover's own obstacle taken out for its search). INFERRED: mode 2 is the state of
  the searching character's own obstacle (core as kind 11), so it never stands in another mover's
  way; ours drops the mover's own obstacle altogether (same effect on paths). Not ported further.
  **Correction (round 7, below):** the INFERRED reading above is wrong. `0x00B54900` is applied to
  the order's target, not to the mover.

## 13. Round 7: closing the §1 item (2026-10-04)
Five open points, each given a focused try. Results:

**1. Search mode 2 (source of mode 3): CONFIRMED and ported.**
- `0x00AF5E80` builds the target scope of a query. Its callers are the order queries, about 35 of
  them.
- It takes the order's target from the query object:
  - virtual `+0x48` gives a building;
  - virtual `+0x20` gives a character obstacle.
- A target character is dropped as a target when the searching faction has a shroud and cannot see
  it (`0x00B7A150`). It then stays an ordinary obstacle, or a hidden one.
- For every layer manager of the search, the target's obstacle is switched by `0x00B54900`, which
  finds the character id | 0x80000000. `0x00B0D9C0` then does the switch:
  - new mode = 2 when the mode was 0, else 3;
  - slot 2 or 3 is built from the current slot's versions, re-kinded 8 → 10 and 9 → 11
    (`0x00B545B0` → `0x00B54570`);
  - the old mode is kept at `+0x70`, and `0x00B66B30` restores it afterwards.
- A target building gets the same through `0x00B54E50`. That obstacle's id is the building +0x158 |
  0x40000000; it is a map-slot obstacle, kept at run time only and never saved. The building is also
  stored on the query (`0x00B3F620`, `+0x10C`).
- So the target can be walked into and a path can end inside it, but no path can cross it. By the
  kind link table, from 10 only 10, 11 and 7 follow, and from 11 only 11.
- Mode 2 is the target in mode 0 (enemy zone and core). Mode 3 is the target in mode 1 (core only:
  not at war, or its zone holds the start).
- **Ported** (`zoc::cuts`):
  - the target's zone is cut as kind 10 and its core as kind 11 (`TARGET_CORE_KIND`);
  - before, the target was left out, so a path could cross it;
  - which character is the target is still the PROVISIONAL stand-in: the one within 3 units of the
    goal. The original is told the target.
- Test: `zoc.rs::enemies_cut_their_zone_friends_their_core`. Sent onto an enemy, the path walks
  through its kind 10 zone into the kind 11 core and ends on it.

**2. How a family A mover enters its target settlement: CONFIRMED that no search does it; ours
tested.**
- Both kind 7 rules have no exception for the target:
  - `0x00B16A30`: mover types 0..2 and 6..8 outside kind 7 never step in;
  - `0x00B167B0`: types 3..5 never step out.
- The mode 2 re-kinding only maps 8 → 10 and 9 → 11 (`0x00B54570` / `0x00B54AE0`). The kind 7
  footprint of the target building stays kind 7.
- So the original's path ends at the footprint. Entering is the order code's step: `0x009DA300`
  sets "inside a building" (+0x4E8) and the slot within 1 unit, called at the end of the move
  (`0x00A16110` and 9 others). Exactly where that step puts the character was not traced (UNKNOWN).
- Ours leaves the goal's footprint open, so the path itself walks in. The end position is the same:
  the settlement point.
  - Difference: the action points of the few footprint polygons, which ours charges and the
    original's snap may not (UNKNOWN).
  - Test: `movers_install.rs::every_settlement_stays_reachable` (156 settlements on the 5 maps).

**3. The 26 army zones that differ (43 of 69 army boxes match, 17 of 18 navies, 8 of 8 agents):
decoded further, not fixed.**
- The zone flood is `0x00ACA170`, called from `0x00B7B7A0` via `0x00B7BAF0`.
- Its nodes are (polygon, point) search locations. The point is the location's fixed-point
  position, node +4 (`0x0047B850`).
- The step cost is a pair (`0x00B11820`):
  - Between two non-start nodes: 2 × the direction table at `0x0137DE70` (1.0 straight, 1.41421
    diagonal), plus a tie-break distance from the point to the shared edge segment.
  - From the start: the true distance between the two points (from fixed-point coordinates).
  - Into the start's own cell: an octile formula on the fixed-point offsets.
- A neighbour is taken when g + step / 2 ≤ limit × 0.9999, or ≤ limit × 1.0001 with the tie part
  ≤ the limit's.
- The centre is the building's position when the character is in one (`0x009FB9D0`), and +2 for a
  fort or settlement (CONFIRMED garrison bonus).
- Ours (`zoc::reach`) steps between cell centres with the same table and limit rule. The
  difference is the node points: the original's entry points are computed by `0x00B011B0`
  (1 660 bytes, not ported).
- That accounts for a one-cell difference at the rim of a 6-unit flood (INFERRED). The exact point
  rule is UNKNOWN.
- Safe behaviour: ours. The zones block whole polygons within the limit. 62 % of army boxes are
  exact, and the rest are one cell off (`zoc_install.rs` asserts at least 60 %, and every core
  range exact).
- Effect in play: a zone may reach one cell further or shorter at its edge.

**4. The harbour exit: INFERRED, unchanged.**
- A fleet in port is inside a building (+0x4E8), so its query types are 3 / 4 / 5 (`0x0095E3D0`).
- `0x00B167B0` forbids types 3..5 from stepping out of kind 7, and the port polygons are kind 7
  islands. Only types 9 / 10 / 11 (`0x0095E4F0`) may leave kind 7.
- So the original must start an in-port fleet's search with a 9..11 type, or from the port node.
- `0x0095E4F0` has about 40 callers, the zone flood among them. The one used for a fleet's sail
  order was not identified in the time box (UNKNOWN).
- Data cannot settle it: no fleet stands in a port in any vanilla save.
- Safe behaviour (tested): ours leaves through the port footprint to the sea (`Harbour`). It fits
  all 67 ports (`ports_install.rs`), and `embark_campaign.rs` sails a fleet out of Genoa.

**5. Where an embarked army's commander is saved: UNKNOWN, data cannot settle it.**
- The link fields are CONFIRMED and loaded: NAVY #4 ↔ ARMY #7 (§9.3).
- No vanilla save holds an embarked army.
- The embark order code (`0x00918870` and its helpers `0x0091F720`, `0x0091CAB0`, `0x00917700`)
  plans the boarding. The code that moves the carried army along with its fleet was not found in
  the time box.
- Safe behaviour (tested): ours keeps the passenger at the fleet's position (`sync_passengers`). The
  loader restores `World::embarked` from the links, with the position rule as a fallback.
  `embark_campaign.rs` round-trips an army aboard through a save.

Residual unknowns of the §1 item:
- how the target is named to the query (our 3-unit stand-in);
- the snap step's cost;
- the flood's entry points;
- the in-port fleet's search type;
- the embarked commander's saved position.

None of these blocks loading, saving or play.

## 14. Round 8: polypath visibility reconcile (2026-10-04)
- Audited the §reconcile visibility list (CONFIRMED by grep over `crates/`): `embark.rs`
  uses `PolyMap::cell_rc` (heuristic + neighbour cells), `PolyMap::multiplier` and
  `PolyMap::goal_step` (transport step costs), `octant_dir` and `dist_to_segment` (same).
  All five stay `pub` (REQUIRED, behaviour unchanged).
- `dist_to_polygon` has no caller outside `polypath.rs` (only `View::polygon_at`,
  `View::locate_where`, `View::polygons_within`): demoted from `pub fn` to `fn`
  (behaviour-neutral, one line). `View::cell_rc` / `multiplier` / `goal_step` left `pub`
  (pathfinding worker's surface, untouched).
- `commands.rs` note reconfirmed unchanged: `plan_path` asks `embark` first for embarked
  armies and port entries, `walk` carries passengers, `Walk` / `walk` / `commander_of`
  stay `pub(crate)`.
- Tests: `cargo test -p ntw_sim --lib campaign::` (133 passed), `cargo test -p ntw_campaign
  --lib` (29 passed).

## 15. Round 9 (sandbox): one-cell-rim failing-case sample (2026-10-04)
- Scope: the 26 of 69 army zone boxes one cell off (§13.3; navies 17/18 and agents 8/8
  unchanged, `zoc_install.rs`). Grid: origin (−410, −190), cell 2, 375 × 193; cells are
  (col, row) from the south-west; Δcells = ours − saved per edge (c0, r0, c1, r1).
- Method (test-only, no shipped code change): a TEMP probe test rebuilt every army
  `obstacle_record` on the vanilla eur_napoleon start position and dumped saved vs ours
  bbox/cells per mismatch (Steam read-only; probe file deleted afterwards, so the only
  change is this note). CONFIRMED counts: 26 misses = 21 garrisoned (limit 8) + 5 field
  (limit 6), vs 55 garrisoned of 69 overall: no garrison enrichment. 26/26 starts locate
  (`polygon_at == locate(_, _, Land, 2)`, a cell each): NOT a locate failure. 19 are
  single-edge single-cell, 7 multi (two edges/cells or mixed). Direction: 18 ours-bigger
  (we reach further), 7 ours-smaller, 1 mixed (INFERRED from the Δcells signs).
- Sample (10 of 26; "class" answers flood-entry / node-point / rim: every sampled miss is
  a rim-pattern bbox delta; flood-entry locate failure ruled out CONFIRMED; node-point is
  the leading INFERRED mechanism behind the rims, H1 below):

| id | pos | garr / lim | saved bbox | ours bbox | saved cells | ours cells | Δcells | class |
|---|---|---|---|---|---|---|---|---|
| 861590044 | (−79.76, −89.35) | garr / 8 | (−92, −100, −68, −78) | (−92, −100, −68, −76) | [160, 46, 169, 54] | [160, 46, 169, 55] | (0, 0, 0, +1) N | rim, ours-bigger N |
| 750720740 | (−84.22, 87.47) | garr / 8 | (−98, 74, −72, 100) | (−96, 74, −72, 100) | [157, 133, 167, 143] | [158, 133, 167, 143] | (+1, 0, 0, 0) W | rim, ours-smaller W |
| 863138516 | (−51.90, −103.32) | garr / 8 | (−60, −114, −38, −92) | (−64, −114, −38, −92) | [176, 39, 184, 47] | [174, 39, 184, 47] | (−2, 0, 0, 0) W | rim-2, ours-bigger W ×2 |
| 861607036 | (−83.90, 61.86) | field / 6 | (−94, 54, −72, 72) | (−94, 50, −72, 72) | [159, 123, 167, 129] | [159, 121, 167, 129] | (0, −2, 0, 0) S | rim-2, ours-bigger S ×2 |
| 750316724 | (10.83, 11.35) | field / 6 | (0, 2, 22, 20) | (0, 0, 22, 22) | [206, 97, 214, 103] | [206, 96, 214, 104] | (0, −1, 0, +1) S+N | multi, ours-bigger both |
| 750305396 | (−20.50, 27.26) | garr / 8 | (−32, 16, −8, 40) | (−34, 14, −8, 38) | [190, 104, 199, 113] | [189, 103, 199, 112] | (−1, −1, 0, −1) W+S+N | multi, mixed |
| 748934492 | (−175.15, −69.75) | garr / 8 | (−188, −80, −162, −56) | (−186, −80, −162, −58) | [112, 56, 122, 65] | [113, 56, 122, 64] | (+1, 0, 0, −1) W+N | multi, ours-smaller W+N |
| 750303980 | (−69.83, 51.68) | garr / 8 | (−82, 40, −56, 64) | (−82, 40, −58, 64) | [165, 116, 175, 125] | [165, 116, 174, 125] | (0, 0, −1, 0) E | rim, ours-smaller E |
| 749603092 | (−145.02, −49.30) | garr / 8, 53 polys | (−152, −62, −132, −38) | (−152, −62, −134, −38) | [130, 65, 137, 74] | [130, 65, 136, 74] | (0, 0, −1, 0) E | rim, ours-smaller E, small flood |
| 748919388 | (−136.51, 7.60) | field / 6, 36 polys | (−148, −2, −126, 18) | (−148, −4, −126, 18) | [132, 95, 140, 102] | [132, 94, 140, 102] | (0, −1, 0, 0) S | rim, ours-bigger S, small flood |

- Full 26 edge list (Δcells; B = ours-bigger, S = ours-smaller): N+1 ×2 (B), W+1 ×1 (S),
  E+1 ×1 (B), S−1/N+1 (B), W−2 (B), S−1 ×6 (B), S−2 (B), S−1/N+1 (B), E−1 ×4 (S),
  W−1/S−1/N−1 mixed, W+1 (S), W−1 ×5 (B), W−1/S−1 (B), W+1/N−1 (S). Garrisoned misses: 21.
- Ranked hypotheses (no fix ported: none is CONFIRMED behaviour-safe):
  - H1 node-point (INFERRED, rank 1): our steps use cell centres with no tie-break;
    the original adds the point-to-shared-edge tie-break (`0x00B11820`) and steps between
    entry points (`0x00B011B0`, 1 660 bytes, structure only). Ours is systematically cheaper,
    so we reach ~1 cell further: fits the 18 ours-bigger. Needs `0x00B011B0` decoded before
    any code change.
  - H2 other obstacles cut in (CONFIRMED mechanism §11.1, INFERRED as cause here): the
    original floods with nearby obstacles cut in (`0x00AF5200`); ours floods the static map.
    Candidate for the small floods (53, 36 polys) and the ours-smaller rims near other forces.
  - H3 first-step offset (INFERRED, rank 3): true-distance first step + octile-into-start-cell
    vs our centre-distance + 0-cost same-cell step shifts rims ±1 unit by sub-cell offset
    (offsets recorded, no systematic bias): fits single-edge pattern and both signs, not the
    ×2 cells alone.
  - H4 range rounding (UNKNOWN, rank 4, unlikely): our `0x00B0C240` rule fits every matching
    flood and every core range, so rounding is not the driver; residual check (polygon max
    exactly on a cell edge, Fixed20-short outline) left open.
  - Ruled out (INFERRED by size): the 0.9999/1.0001 limit tolerance (§13) is ~1e-3 units,
    far below one 2-unit cell, so not a sole rim cause.
- No code change (rule: fix only if CONFIRMED + behaviour-safe with tests). Safe behaviour
  stays: whole-polygon zones within the limit; `zoc_install.rs` still asserts ≥ 60 % army
  boxes exact (43/69) with every core range exact.

## 16. Round 10 (sandbox): H1 tested against the decompile (2026-10-04)
- GHIDRA (read-only, `NR-sb-ghidra-ports`; output in `target\tmp\h1_decomp.c`,
  `target\tmp\h1b_decomp.c`, never committed): `0x00B011B0` (1 660 B, flood entry-point
  table builder) and `0x00B11820` (1 614 B, the step-cost/tie pair), plus their flood
  caller `0x00ACA170` and zoner `0x00B7B7A0`/`0x00B7BAF0`.
- `0x00B11820` CONFIRMED: computes a cost pair and returns it via `0x00AEF4E0`:
  - same direction class 8 (no row/col offset): cost (0, 0) — same-cell step is free;
  - both endpoints non-start: first = 2× `DAT_0137DE70[dir]` (2.0 straight, 2.8284
    diagonal at cell 2), second = distance from the projection onto the shared-edge
    segment to the node position (the point-to-shared-edge tie-break);
  - into the start's cell: first = octile formula on the fixed-point offsets,
    second = true distance;
  - from the start: first = true point distance (scaled by 9.0949e-13, the 2^20
    Fixed20 -> map-unit factor), second = 0 or the segment distance by mover flag
    (`0x00B00730`, building-occupied check).
- `0x00B011B0` CONFIRMED structure: per node it keeps a 0x38-byte slot record
  (point ptr, key, limit pair, nine per-direction max distances at +0x10..+0x2C,
  flag byte at +0x34), keyed by grid cell (`0x00B5B720`) and direction class;
  called once per neighbour admission inside the flood, and once beforehand
  (`0x00B7B7A0` seeds it with the zone centre), building the per-direction
  rim-distance seed the flood then consumes.
- Verdict on H1: the mechanism is CONFIRMED present in the original and partly absent
  in ours, but NOT coverable behaviour-safely:
  - the one thing ours lacks — the tie channel's second accept clause
    (`g+step/2 <= limit*1.0001` AND tie <= limit2) — is a second-axis marginal
    rule; with cell cost 2 it can only reclassify borderline cells by sub-unit
    distances, i.e. the same +-1e-3-unit effect already ruled out in section 15
    (H4), not a systematic one-cell shift;
  - the node points in the original come from each polygon's stored flood record
    (its entry point, fed by `0x00B011B0`'s rim seed), whose exact rule is still
    UNKNOWN; our nodes use the polygon cell centre. Porting the tie-break without
    the entry-point rule, or either without the other, shifts the 26 rim boxes in
    neither the saved nor a predictable direction — the predicted sign
    (ours-bigger) came only from assuming the original's step is cheaper, which the
    decompile contradicts for straight moves (identical 2.0) and matches exactly
    for diagonal/start/same-cell cases;
  - measured: with `map.map.cell * DIR_LEN[dir]` our step costs are already
    identical to `2xDAT_0137DE70[dir]` (cell 2), the start step is true distance,
    same-cell is 0 in both, and the admission test `d + step*0.5 > limit` mirrors
    `g + step/2 <= limit`. No code path of H1 is missing *numerically* except the
    point substitution (sub-cell, H3 territory, offsets recorded as unbiased).
- DECISION: H1 NOT ported (not CONFIRMED behaviour-safe; the coverable part reduces
  to the already-ruled-out tolerance). Residual stays INFERRED; the per-polygon
  entry-point rule remains UNKNOWN. `zoc::reach` unchanged; all tests green
  (`cargo test -p ntw_sim -p ntw_campaign --lib`, 283 passed).
- NEXT: H2 (CONFIRMED mechanism section 11.1, INFERRED as cause): the original
  floods with the other characters' zones/cores cut in (`0x00B7BAF0` ->
  `0x00AF5200`); ours floods the static map. Candidate for the small floods
  (53/36 polys) and the ours-smaller rims near other forces. Trace what
  `0x00AF5200` cuts in (modes 8/9/10/11 overlay) before any change to
  `reach_in`.

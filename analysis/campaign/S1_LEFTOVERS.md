# BACKLOG §1 leftovers: farm records and diplomacy fields

Worker: s1-leftovers (branch `work/s1-leftovers`). Tags: CONFIRMED / INFERRED / UNKNOWN; our stand-ins PLACEHOLDER /
PROVISIONAL. Ghidra findings are written here as specs; decompiled code is never stored.
Ghidra: own copy `%USERPROFILE%\Documents\NR-s1-ghidra` (copied from `NR-f0c-ghidra`), headless runner
`analysis/campaign/run_ghidra_s1.ps1` + `analysis/campaign/ghidra_scripts/S1Decomp.java` (modes as in the 0-C worker's
`F0cDecomp.java`, plus `strs:` = list every string containing a needle with its referencing functions, and `strat:` = print
the strings at given addresses). Output stays in the scratch folder.

## Where I am / what's next
- **Finished (2026-10-04), ready to merge.** Merged origin/main (my Ghidra runner is now `run_ghidra_s1.ps1`; main's
  `run_ghidra.ps1` belongs to save-compat). `cargo build --workspace` and `cargo test --workspace` pass; `cargo clippy
  --workspace --all-targets` adds no new warnings (the two left in `battle_markers.rs` lines 365/392 predate this branch).
  Game run: `--campaign eur_napoleon --campaign-demo` and `--battle-key NHB_Austerlitz` (a map with farms) start without
  panics (the game does not read the farm files yet). Optional later: the UNKNOWN farm values via `FARM_AUTO_GENERATOR`
  (`0x00FE26F0`).
- Scope (manager, 2026-10-03): (1) farm record fields [CAMPAIGN_DATA.md §6], (2) diplomacy record fields
  [CAMPAIGN_DATA.md §3]. Missions and UI layouts were handed over to `work/s1-missions-ui` (see "Handed over").
- **Diplomacy: done** (§1). Every `DIPLOMACY_RELATIONSHIP` field is named in `ntw_sim::campaign::details::Relationship`
  and loaded by `ntw_campaign::details::relationship`, except #16, #21, #22 (UNKNOWN: the exe only loads, saves, copies
  or resets them). Checked on all 8 startpos files and the 8 user saves (`tests/real_install.rs::check_details`).
  Our rules do not use these fields yet (PROVISIONAL: no diplomacy rules update them; the save keeps the original values).
- **Farm records: done as far as the files and the readers/writers allow** (§2): every field of `FARM_COLLISION`,
  `FARM_TILE_SET`, `FARM_INSTANCE` and the whole `FARM_MANAGER` is typed (`ntw_formats::battle_markers::FarmManager`,
  `FarmTileTemplate::collisions`); meanings CONFIRMED by relations that hold in every shipped file, the rest tagged
  INFERRED / UNKNOWN (open list at the end of §2). Nothing in our renderer uses these yet (PROVISIONAL: farms are not drawn).
- **Next (if more time is given):** the farm users in the exe (generated-battlefield code near `0x00FE26F0`
  `FARM_AUTO_GENERATOR::generate_farms`) for the UNKNOWN farm values; #16/#21/#22 of the relationship stay UNKNOWN.
- Tool: `cargo run -p ntw_campaign --release --example diplo_probe -- stats <files>` (value histograms per field) /
  `rows <file>` (one line per relationship, with the non-zero attitude factors).

## 1. `DIPLOMACY_RELATIONSHIP` v14 (all fields)
Writer `0x00AFC340` (one caller `0x00AFC160`), reader `0x00AE9480` (from the array reader `0x00AE8E10`, 0x848-byte
relationship objects), copy `0x00AE9160`, record-name getter `0x00B023B0`. ESF type codes in the writer: 1 bool, 4 i32,
8 u32, 0x0E utf16. The writer→member map and the version rules come from the writer and reader (CONFIRMED). Meanings come
from the code that uses each member (listed) and the value survey (8 startpos files: 3,608 relationships; 8 saves:
13,120).

| # | type | member | model field | meaning | tag |
|---|---|---|---|---|---|
| 0 | i32 | (faction object) | key | target faction id | CONFIRMED |
| 1 | 24 records | +0x008, 24 x 0x28 | `attitudes` | the attitude factors (below) | CONFIRMED |
| 2 | bool | +0x788 | `trade_agreement` | trade agreement: `0x00B750D0` adds "current_treaty_trade_agreement" when set; the debug text `0x0092E1B0` prints "(TRADE RIGHTS)"; cleared when trade breaks (`0x00B45C20`) | CONFIRMED (was INFERRED "military access": wrong) |
| 3 | i32 | +0x78C | `military_access_turns` | military access the owner gives the target: turns left, -1 indefinite, 0 none ("current_treaty_giving_military_access_turns/_indefinite"; the target's own record gives "has_military_access") | CONFIRMED |
| 4 | utf16 | +0x790 | (`Faction::diplomacy`) | stance: a `DIPLOMATIC_STANCE_RECORD` pointer; its +0x10 index is 0 war, 1 neutral, 2 allied, 3 patron, 4 protectorate | CONFIRMED |
| 5 | i32 | +0x794 | `war_ally` | a faction id (post-load pointer fix-up in `0x00B5BD70`). Set by the war declaration `0x00B27070` when the war is declared on another faction's behalf (that path uses other attitude penalties); cleared by the peace `0x00B26590` | CONFIRMED type, INFERRED meaning |
| 6 | u32 | +0x798 | `alliance_commitment_turns` | 20 on every startpos alliance/patron/protectorate; -1 a turn (`0x00B29170`); when an alliance breaks while it is > 0, the factions on the owner's side get an attitude penalty of minus this value (`0x00B13840`); zeroed when the alliance ends (`0x00B0CA20`) | CONFIRMED mechanics, INFERRED name |
| 7 | i32 | +0x7D8 | `war_momentum` | battle results add -8..+8 by battle kind and win/loss (`0x008AC370` → `0x00B10460`, which also zeroes #13); each turn a positive value drops by 2, a negative one rises by 1; the AI peace evaluation `0x00A351E0` reads it | CONFIRMED |
| 8 | i32 | object at +0x7AC (value +0x7B4) | `protectorate_tribute` | while the owner is the target's protectorate: `0x00BBCD00`'s amount, reported to the economy tracker as category 6. Startpos: 661 on the one protectorate record | CONFIRMED source, INFERRED name |
| 9 | i32 | +0x7BC | `protectorate_income` | while the owner is the target's patron: the same amount, economy category 2 ("finances_protectorate_income"). Startpos: 661 on the one patron record | CONFIRMED source, INFERRED name |
| 10 | i32 | +0x7DC | `war_region_balance` | at war, each turn: (value of the target's regions now held by the owner minus the reverse) / 1000, clamped -10..10 | CONFIRMED |
| 11 | i32 | +0x7E0 | `war_wealth_balance` | at war, each turn: a strength/wealth difference / 2500, clamped -10..10 | CONFIRMED shape |
| 12 | u32 | +0x7E4 | `war_turns` | +1 per turn at war; zeroed at peace | CONFIRMED |
| 13 | u32 | +0x7E8 | `turns_since_battle` | +1 per turn at war; zeroed by a battle result | CONFIRMED |
| 14 | `REGULAR_PAYMENTS[]` | +0x7A4 count, +0x7A8 data, 0x18-byte items | `payments` | items {i32 amount, u32 turns left}; turns -1 a turn, removed at 0; cleared by war | CONFIRMED layout; direction INFERRED |
| 15 | u32 | +0x7C0 | `friendship_turns` | set to 10 by peace, by granting military access, and each turn while allied (stance 2..4) or while the target gives the owner access; raised to min(turns, 10) by a new regular payment; -1 a turn | CONFIRMED, name INFERRED |
| 16 | u32 | +0x7C4 | `unknown_16` | only constructed (`0x00AEA0F0`), reset (`0x00B2B920`, `0x00B72900`), loaded, saved, copied; 0 in every sample | **CONFIRMED unused** (2026-10-04, see below) |
| 17 | `ALLIED_IN_WAR_AGAINST[]` | +0x7CC cap, +0x7D0 count, +0x7D4 data | `allied_in_war_against` | items {u32 enemy faction, i32 saved access turns}: joining an ally's war (`0x00B0CCE0`) stores the access turns and sets #3 to -1; ending the alliance (`0x00B0CA20`) restores the first item's turns and clears the list; items of dead factions are dropped and the turns count down each turn | CONFIRMED |
| 18 | u32[14] | +0x7EC | `diplomacy_options` | `force_diplomacy` permissions (script handler `0x009792D0`), one per option: trade agreement, military access, cancel military access, alliance, regions, technology, state_gift, payments, protectorate, peace, war, join_war, break_trade, break_alliance. Value = (accept false) + 2 x (offer false) for `force_diplomacy(a, b, option, offer, accept)` (`0x009793C8`..`0x009793DE`; the bools are read last-first through `0x01055840`): 0 allowed, 2|3 a may not propose, 1|3 a declines it from b, 3 blocked; set on a's relationship to b only (CAMPAIGN_FIDELITY "Scripted diplomacy permissions"). Files before v3 stored bools (true → 0, false → 3); reset to 3 after load when the owner is the pirates faction (`0x00B28690` from `0x00B5BD70`, test `0x008CEE20`; corrected 2026-10-08, was "the human"). The AI reads them through the table `0x01459080` (negotiation item → option) | CONFIRMED (bit order: disassembly of `0x009792D0`, 2026-10-10) |
| 19 | u32 | +0x824 | `military_access_streak` | +1 per turn while #3 ≠ 0, else 0 (v>6 only) | CONFIRMED |
| 20 | utf16 | +0x828 | `previous_stance` | a second stance record (v<8: copied from #4) | INFERRED previous stance |
| 21 | bool | +0x82C | `unknown_21` | only constructed, loaded (reset as a pair with #22), saved, copied (v≥8); no clean correlation with trade, access or alliance in the saves | **CONFIRMED unused** (2026-10-04) |
| 22 | bool | +0x82D | `unknown_22` | as #21 (no instruction in the exe uses displacement 0x82D at all; the reader, writer and copy reach it from +0x82C) | **CONFIRMED unused** (2026-10-04) |
| 23 | i32 | +0x830 | `start_attitude` | the `diplomatic_relations_attitudes` thresholds (-85, -45, 0, 45, 85 only); read by the relationship setup `0x00B2BC10` / `0x00B72AC0`, which passes it to `0x00B45A40` as the `initial_modifier` factor value | CONFIRMED values, INFERRED use |
| 24 | i32 | +0x834 | `military_access_granted` | the granted length: `0x00B44550` (grant) stores turns or -1 here, sets #15 = 10, #25 = 0 | CONFIRMED |
| 25 | u32 | +0x838 | `military_access_elapsed` | +1 per turn of timed access | CONFIRMED |
| 26 | u32 | +0x83C | `access_cancel_grievance` | `0x00B67BD0` (access cancelled early) adds base - used share: granted 5 → 50 - 10 x elapsed; 10 → 60 - 6 x elapsed; 20 → 70 - 7 x elapsed / 2; else 90 - 5 x elapsed / 2; -2 a turn (min 0); `0x00B56770` returns max(other value, this) | CONFIRMED shape, INFERRED name |
| 27 | u32 | +0x840 | `trade_embargo_turns` | `0x00B28DB0` sets 10 (breaking a trade agreement first), -1 a turn; ≠ 0 means embargoed ("current_treaty_trade_embargoed"); one save has 7 | CONFIRMED |
| 28 | bool | +0x844 | `allows_region_return` | default true (files before v14 lack it); cleared by a war declaration while the stance is patron (`0x00B27070`); read by the region-capture/liberation rule `0x00B14930` | CONFIRMED uses, INFERRED name |

**#16, #21, #22 unused (CONFIRMED 2026-10-04, pathfinding worker):** every instruction in the exe whose displacement is
0x7C4, 0x82C or 0x82D was listed (`scal:` over 0x00401000..0x01300000). On relationship objects only the constructor
`0x00AEA0F0`, the resets `0x00B2B920` / `0x00B72900` (the relationship setups), the reader `0x00AE9480`, the writer
`0x00AFC340` and the copy `0x00AE9160` touch them; the other hits are other classes (a faction counter pair +0x7C0/+0x7C4
in `0x00A845B0`, a counter at +0x82C in `0x00A9F200`, an AI object's +0x168 → +0x82C in `0x00C164C0`, battle and
render code). So no rule reads them: they are kept only so saves round-trip. (A use through a computed address cannot be
ruled out by a displacement scan, but none of the relationship code computes one.)

The object also has a second 24 x 0x28 attitude array at +0x3C8 that is **not saved** (the campaign-start setup
`0x00B45A40` points the first array's write pointers at it).

### Attitude factors (#1)
Slot order (CONFIRMED: the exe's static array of 24 keys initialised at `0x0042F300`; the war code writes slot 7 at
+0x120, the abused-access penalty slot 13 at +0x210, the embargo slot 23 at +0x3A0, the start attitude slot 17 at
+0x2B0, all equal to 8 + slot x 0x28): state_gift, alliance, alliance_broken, cultural_alliance_broken,
declared_war_against_enemies, trade, trade_broken, war, peace_treaty, allied_with_enemies, declared_war_against_friends,
abandoned_ally_in_war, annexed_territory, abused_military_access, assasination_attempt, religion, government_type,
initial_modifier, sabotage_attempt, spying_attempt, threatened, faction_leader, enlightenment, trade_embargoed. These are
the `diplomacy_factor_strings` keys (that table's 25th key, `diplo_character_bonus`, has no slot). Our constant:
`details::ATTITUDE_FACTORS`.

Item {i32, i32, i32, bool, i32, bool} (CONFIRMED by the per-turn update `0x00B290D0`, the setters `0x00B69640`,
`0x00B69620`, `0x00B02CB0`, `0x00B69B90` and the sum `0x00B0DB60`):
- `drift` (per turn), `value` (current), `limit`, `limited`: each turn value += drift; when limited, the value stops at
  the limit (from below for a positive drift, from above otherwise). Samples: alliance +1 → 80, war -2 → -200.
- `cap`, `capped`: in the attitude total the factor counts at most `cap` (cap > 0) or at least `cap` (cap ≤ 0). Set at
  campaign start from the diplomacy config on declared_war_against_enemies (15) and declared_war_against_friends (-15).
- The total attitude = Σ contributions (`Relationship::attitude_total`); `0x00B0DBA0` maps it to a category.
- An effect from the diplomacy config (the struct returned by `0x00B27FF0`, triples at +0x0C, +0x18, +0x24, +0xA8, +0xB4,
  +0x12C, +0x150, +0x15C, ...) either sets {value, drift, limit} (`0x00B69640`) or adds to the value, then sets drift and
  limit and clamps (`0x00B02CB0`). Mapped calls: war 0x12C, war on an ally's call 0x150, peace 0xA8 / 0xB4 (ally's war),
  abused access 0x0C (plus a per-use term), embargo 0x15C, alliance 0x18. The config values themselves were not decoded
  (they belong to the diplomacy rules, not to this record).

### Treaty display (`0x00B750D0`, CONFIRMED)
Builds the "current treaty" lines from `diplomacy_strings` keys: protectorate of player / non-player (stance 4), at war
(stance 0), alliance (stance 2), trade agreement (#2), giving military access (#3, turns or indefinite), has military
access (the target's #3), trade embargoed (#27), embargoing trade (the target's #27).

## 2. Farm records (`.farm_template_tile`, `.farm_manager`)
Typed readers in `ntw_formats::battle_markers`: `FarmTileTemplate::collisions` (`FarmCollision`) and `FarmManager`
(`FarmTemplateRef`, `FarmTileSet` / `FarmTileCell`, `FarmInstance`, `OwnerInstance`). Install test
`tests/battle_extras_install.rs::farm_records_typed` (all 32 templates, 387 collisions; all 34 managers, 1,008 farm
instances) checks every relation below. Survey tool: `cargo run -p ntw_formats --release --example farm_probe --
show <RECORD> [max] [filter] | collision | tilesets | pieces`.

Record names → name getters: `FARM_COLLISION` `0x00FD68F0`, `FARM_TILE_SET` `0x00EA44F0`, `FARM_INSTANCE`
`0x00EA43D0`, `FARM_DATA_ITEM_OWNER_INSTANCE` `0x00EA42B0`, `WALL_INSTANCE` `0x00EA4BB0`, `ROAD_INSTANCE` `0x00EA4970`,
`POST_INSTANCE` `0x00EA4850`, `FARM_TILE_TEMPLATE` `0x00EA4550`, `FARM_MANAGER` `0x00EA4490`.

**`FARM_COLLISION` v2** (reader `0x00FD63B0`; in every `FARM` and `ROAD` of a template):
| # | type | meaning | tag |
|---|---|---|---|
| 0 | coord2d | reference point (centre) of the outline: not the box centre (146/387) nor the area centroid | INFERRED centre; how it is chosen UNKNOWN |
| 1 | coord2d[] | the outline polygon | CONFIRMED |
| 2 | f32 | distance from #0 to the nearest outline edge (inner radius); files before v2 stored its square (the reader takes the root) | CONFIRMED (387/387) |
| 3 | f32 | distance from #0 to the farthest outline point (outer radius); squared before v2 | CONFIRMED (387/387) |
| 4, 5 | coord2d | box min / max = the outline box grown by 12 m, floored (375/387) | CONFIRMED box, INFERRED margin rule |
| 6 | bool | v2 only, default true, true everywhere | UNKNOWN |

**`FARM_MANAGER` v3/v4** (writer `0x00E99490`, reader `0x00EC67B0`): coord2d min, coord2d max (map area), u32 n +
n x {`FARM_TILE_TEMPLATE`, u32 hash}, u32 n + n x `FARM_TILE_SET`, two lists of u32 n + n x `FARM_INSTANCE`, u32 n + n x
`WALL_INSTANCE`, u32 n + n x `ROAD_INSTANCE`, u32 n + n x `POST_INSTANCE`, then one utf16 name per farm (both lists),
wall and road instance (`field66`, ...), then one coord2d per post (CONFIRMED order from the writer; the v4 files are the
empty form: 2 coord2d + 7 zero counts).
- `FARM_TILE_TEMPLATE` v3/v4 (writer `0x00E99E60`): template path, u32 count + the 9 `.farm_fields_tile_texture` paths
  (blend / colour / grass map, each for field / building / road), the tiled underlay map, the wall texture folder, the wall
  model key and its end piece, the wall `.rigid_spline`, the fence pieces 1/2/4/8 + end, the hedge pieces 1/2/4/8 + end
  (CONFIRMED by the writer's order and every shipped value). The u32 after it is computed by `0x00EAB460` (rotate-xor hash;
  INFERRED a checksum of the template).
- `FARM_TILE_SET` v1 (reader `0x00E89790`, writer `0x00E99A00`): 3 coord3d rows of a 2D affine transform (default identity;
  every file: identity plus a translation), u32 template index (the reader resolves it in the manager's template list),
  u32 n + n cells {i32 column, i32 row (values -2..2: the cell of the template tile grid; INFERRED), u32 k + k x {u32 index,
  i32 list} = the farm instances in the cell: `index` is below the size of farm list `list` (0 or 1) in every file
  (CONFIRMED relation), u32 m + m x u32 wall instance indices (below the wall count everywhere; INFERRED), v1 only: u32 +
  u32[] that the reader skips and the v2 writer drops (empty), u32 + u32[] (empty everywhere; UNKNOWN, INFERRED roads or
  posts)}.
- `FARM_INSTANCE` v2 (reader `0x00E88C80`, writer `0x00E992D0`): `FARM_DATA_ITEM_OWNER_INSTANCE` v1 {u32 owner = index in
  the template's `FARM_LIST` (below its size everywhere; CONFIRMED relation), u32 (0 everywhere; UNKNOWN), 3 coord3d
  transform rows}, u32 n + n x {u32, u32} pieces, bool (false everywhere; UNKNOWN), bool duplicate (the reader of files
  before v2 sets it when an instance in either farm list has the same owner and transform within 0.001; CONFIRMED). The
  pieces: running indices across the manager with a 0/1 value; where a farm appears in both lists (same owner and
  transform) the two copies carry the same indices with opposite values (INFERRED: which copy owns each piece; meaning
  UNKNOWN).
- `WALL_INSTANCE` (writer `0x00E9ABB0`): owner instance, i32, u32 n + n x {u32, i32, u32}, u32 m + m x u32 (kept raw;
  UNKNOWN meanings). `ROAD_INSTANCE` (`0x00E9A8A0`): owner instance, u32 n + n x {u32, u32}. `POST_INSTANCE`
  (`0x00E9A670`): u32, u32 n + n x u32, u32, 3 coord3d. No road or post instances ship.

Open (not needed to draw the farms): the wall instance values, the 0/1 value of the piece pairs (see the update below for
the collision centre, the bool and the two lists).
### Update (pathfinding worker, Ghidra, 2026-10-04): farm generator and collision rules
- **Collision object at run time** (`0x00FD6680` builds one from a polygon, called by `0x00FE1F40`; tests `0x00FD6950`,
  `0x00FD6D90`, `0x00FD6E70`): +0 centre, +0xC/+0x10/+0x14 the outline vector, +0x18 inner radius, +0x1C outer radius,
  +0x20..+0x2C the box, byte +0x30 the bool (#6). The builder sets **centre = the average of the outline's vertices**,
  **bool = centre inside the outline** (`0x00FD6A00`), inner radius = the nearest-edge distance only when the bool is set
  (`0x00FD6A90`), the outer radius and the box from the points (no 12 m margin at run time).
- **The bool (#6) = "the inner circle is usable": CONFIRMED.** The point and rectangle tests accept at once when the
  point/rectangle lies within the inner radius only if it is set, else they reject beyond the outer radius and run the
  exact polygon test. The shipped files have it true everywhere, also on the 19 collisions whose stored centre lies
  outside the outline (a data quirk: there the quick accept can be wrong).
- **The stored centres** were not made by that builder: 0 of 387 are the vertex average; 144 are the outline's box
  centre, the rest lie near it (INFERRED: the editor took the box centre of an earlier outline that was edited later;
  UNKNOWN for sure). Only the bool and the radii matter to the game's tests.
- **The two farm lists: CONFIRMED.** When the generator (`0x00FE26F0`, per farm of a template tile and its 3 x 3 tile
  offsets, after the exclusion tests `0x00FE2490`) adds a farm (`0x00EA6AF0` → `0x00EA6920`), `0x00EA6780` puts it in
  **list 0 when its transformed box lies inside the map's playable rectangle and in list 1 when it reaches outside**
  (`0x00EA68F0` packs {index, list}). Only list-0 farms with the placement flag (instance byte +0x54, set when the farm
  overlaps the "keep" exclusion categories or a matching faction filter) get their buildings placed (`0x00EA7200`, and
  `0x00EA7310` adds 0..5 random buildings with the probability table `0x013BC85C`); INFERRED: the piece pairs are those
  building placements.
- Still UNKNOWN: the meaning of the piece pairs' 0/1 value, the second u32 of the owner (the wall instance values: settled below)
  instance.
instance values, the two farm lists' roles.

### Update (save-compat worker, Ghidra, 2026-10-04): the wall instance values, settled
`WALL_INSTANCE` (run-time object 0x68 bytes, in the manager's list at +0x6C; writer `0x00E9ABB0`, reader `0x00E93B90`):
- **#1 i32 (+0x34) = the wall's boundary kind: CONFIRMED.** The generator (`0x00EA9260`, per wall edge of a placed farm)
  draws it with its MS LCG from the farm's own kind list (the `i32[]` at the end of each template `FARM` record, run-time
  +0xB4/+0xB8; shipped lists: [0], [0, 3, 2], [0, 3, 1], [0, 4], [0, 3], [0, 1]). A drawn 3 is replaced by another entry
  of the list (or 0 when the list has one entry), and a wall keeps the first non-zero kind any adjoining farm gives it.
  Shipped values: 0, 1, 2, 3 (44 instances). Which model each kind draws (the template's wall, fence or hedge pieces) was
  not traced (the drawing code was not found); the generator rule above is what a writer needs.
- **The {u32, i32, u32} list = the farms on the wall: CONFIRMED.** `0x00EA6C10` appends {farm index, farm list (0/1),
  slot}, where slot is the index of the farm's template in the wall's template list (+0xD4/+0xD8); one entry per farm that
  borders the wall (two in every shipped wall with neighbours on both sides).
- **The trailing u32 list: CONFIRMED unused in the data.** Empty in all 44 shipped instances; the generator never fills
  it (only the reader and the writer touch it).
## Handed over (to `work/s1-missions-ui`, 2026-10-03)
- **Missions:** only the ESF record-name getters were found before the hand-over (CONFIRMED, each returns the
  name string): `CAMPAIGN_MISSION` `0x009C70D0` (string `0x0137064C`), `CAMPAIGN_MISSION_MANAGER` `0x009C7190`
  (`0x01370660`), `CAMPAIGN_MISSION_LOCALISATION_OVERRIDES` `0x009C7130` (`0x0137067C`), `CAMPAIGN_MISSION_REWARDS`
  `0x009C7250` (`0x013706A4`). Their callers are the writer/reader to decode next. Nothing on the layout yet.
- **UI layouts:** no work done.

### Update (save-compat worker, Ghidra + data, 2026-10-04): the farm piece pairs and the owner's second u32, settled
- **The `FARM_INSTANCE` {u32, u32} pairs = the walls on the farm's edge: CONFIRMED.** Each pair is {wall instance
  index, side}, and the side is the wall's slot for this farm: the third value of the wall's {farm index, farm list,
  slot} entry (see the wall update above), so 0 or 1. Data (`farm_probe wallpieces`): in the 3 managers with farms,
  every wall-to-farm link has the matching pair, with side = slot each time (36 of 36 in indian_artillery_fort, 50 of
  50 in indian_great_fortress; western_artillery_fort has no walls). The pair count equals the link count. Exe: when
  a farm is in both lists, its copy (`0x00E891C0`, from the duplicate pass `0x00EA8760` / `0x00EBA500`) gets
  {wall, 1 - side} for each pair. It leaves out walls whose object (owner accessor `0x00EEE7A0`) has a single
  template (+0xD4 = 1) and +0xE4 = 0. That is the "same indices, opposite values" seen in the files. The earlier
  "building placements" guess was wrong: buildings go to the manager's own list (`0x00EA7110` / `0x00ED8F00`).
- **The owner instance's second u32 = the tile template index: CONFIRMED.** The owner accessor `0x00EEE7A0` returns
  item `owner` (0xFC bytes each) of the object list of the manager's template number `+0xC` (the second u32). The
  generator copies it from a farm to the buildings (`0x00EA7200`) and walls (`0x00E941B0`) it places. A farm's
  identity (`0x00EA6920`) is (owner, template). 0 in every file: no shipped manager has more than one template
  (7 have one, 26 none).
- Still open in the farm records (none needed by the game or a writer): the stored collision centres (an editor
  leftover), the tile-set cell coordinates and wall indices (INFERRED), and the cell's last u32 list (empty
  everywhere).

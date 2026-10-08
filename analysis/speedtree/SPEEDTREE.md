# SpeedTree 4 (SpeedTreeRT inside Napoleon.exe) — format and generator notes

SpeedTree worker, branch `work/speedtree`. Goal: trees and shrubs 1:1 with the original (BACKLOG §1 `.spt`, §2 trees).
Tags: CONFIRMED (read in the exe and/or checked on every shipped file), INFERRED, UNKNOWN. Stand-ins: PLACEHOLDER /
PROVISIONAL. Everything here is our own description of behaviour; no decompiled code is committed.

Code: `crates/ntw_formats/src/speedtree/` (`spt.rs` reader, `spline.rs` splines), probe
`cargo run -p ntw_formats --example spt_probe -- all | dump <substr>`, install test
`cargo test -p ntw_formats --test speedtree_install -- --ignored`.
Ghidra: own project copy `%USERPROFILE%\Documents\NR-spt-ghidra\NTW.gpr`, script `ghidra_scripts/SptDecomp.java`
(copy of the AI worker's script plus `bytes:<ascii>` = find raw bytes and decompile their users).

## Where I am / what's next (updated with each push)
- DONE: `.spt` reader (all 229 files to the last byte); spline + 500-sample table; Newran + CRT rand; the whole
  `CBranch::Compute` (frames, gravity, disturbance, twist, flares, roughness, rings, smooth normals, children with
  snap/prune/probability, roots, floor); leaf placement with blossoms, spacing, dimming, basis and colour draw;
  frond spines; blade and extrusion fronds; leaf cards and leaf meshes in the renderer. Generated heights vs
  `data.tree_model`: median ratio 0.99 over all 229 files (oak X minimum matches to 1 mm).
- Rendering (`napoleon/src/terrain/speedtree.rs` + `.wgsl`): near trees (≤ `tree_far_distance`, from
  `gfx_tree_quality`) as real geometry in 64 m cells, Bevy-instanced; alpha fizzle into the billboards; shrubs drawn.
  Austerlitz: ~230 FPS in a forest, same as without near trees.
- Wind sway (SpeedWind2) and the billboards' two-picture blend are in.
- NEXT: branch/frond/leaf LOD selection (`FUN_012c9250`, `FUN_012c6480`, `FUN_012c96b0`); per-tree rotation and slope
  (source UNKNOWN); leaf normal smoothing (28000) and normal-mapped lighting as in the shipped shaders; the game's
  wind strength/direction source; leaf-mesh axis packing; leaf rocking angles.
- Fidelity 0-D (`analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` §4): tree-list items carry x, y, a u8 scale and a
  source, **no rotation** (CONFIRMED), so any per-tree angle is made at run time; tweak defaults `tree_near_distance`
  30, `tree_wind_response` 0.6 / limit 1.0, `wind_tree_gust_low` 0.17 / `high` 2.0, `MIN/MAX_TREE_SCALE` 0.5 / 1.4.

## 1. Files
- 229 `.spt` under `rigidmodels\vegetation\battle\<climate>\` (rigidmodels.pack), 4–34 KB each.
- Every shipped file has the same section set (CONFIRMED by `spt_probe all`): 1002, 1004, 1011, then 8000, 9000,
  10000, 11000, 12000, 13000, 15000, 16000, 16013, 16014, 19000, 20000, 21000, 21001, 22000, 25000, 26000, 27000,
  28000, 29000, 30000, 40000, 50000, 60000, 71000, 72000, 73000, 74000, 75000. No 7000 leaf-LOD block, no 18000,
  23000, 24000.
- Branch levels per tree: 3 (108 files), 4 (107), 5 (14). Leaf textures 1–4; frond textures 1 (226) or 2 (3).
  The seed (2005) is present in all files and never 0 or 1, so every tree has a fixed seed.

## 2. Binary layout (CONFIRMED, exe parser)
Stream of LE `u32` tokens; payload type fixed per token. Payloads: `i32`, `f32`, `bool` (1 byte), `str` (`u32` n + n
bytes), `vec3`, `spline` (a `str` with BezierSpline text). Error strings in brackets are the exe's.

Entry `CSpeedTreeRT::LoadTree(Memory block)` = `FUN_012b2e10`; tree block parser `FUN_012ca110`; token reader
`FUN_012c2a80` (i32), `FUN_012c2a30` (f32), `FUN_012c2ad0` (str), `FUN_012c2b90` (vec3).

Tree block: `1000`, str `"__IdvSpt_02_"` ("missing begin_file token", "not a valid SpeedTree SPT file"), then
sections until `1001` ("malformed SpeedTree SPT file"):
- **1002..1003 general tree** (`FUN_012cc4f0`): 2000 str branch texture; 2001 f32 (+0x3c); 2002 bool (skipped);
  2003 f32 (+0x40); 2004 i32 (skipped); 2005 i32 seed (`FUN_012cc8f0`: 0 → random, 1 → keep, else = seed; +0x44);
  2006 f32 (+0x48); 2007 f32 (+0x4c); 1014 branch levels: i32 n, n × level, then 1015.
  - level 1016..1017 (`FUN_012da3f0`, record 0x13730 bytes, ctor `FUN_012da1b0`): 6000–6007 spline (8 profile
    curves), 6008 i32 (default 6), 6009 i32 (default 3), 6010 f32 (0.3), 6011 f32 (1.0), 6012 f32 (0.3), 6013 f32,
    6014 f32, 6015 bool, 6016 bool, 6017 spline.
- **1004..1005 general leaf** (`FUN_012c2310`): 3000 f32 (+0x24); 3001 i32 (+0x2c, +0x50); 3002 f32 (+0x28);
  3003/3006 bool skipped; 3004/3005 f32 skipped; 3007 f32 (+0x18); 3008 i32 (+8); 3009 bool (+0); 3010 f32 (+4);
  1009 = i32 (skipped), i32 n, n × { i32 (skipped), tokens until 1008: 4000 bool, 4001 vec3, 4002 f32, 4003 str,
  4004 vec3, 4005 vec3, 4006 vec3, 4007 f32 (skipped) }, then one i32 (skipped). Leaf texture record 0x60 bytes;
  defaults 4001 (0.8,0.8,0.8), 4002 0.2, 4004 (0.5,1,0), 4005 (0.12,0.12,0), 4006 (10,10,0).
- **1011..1012 general wind** (`FUN_012dad20`): 5000/5001/5003 vec3 (dropped), 5002 vec3 (+0xc), 5004 vec3 (+0),
  5005 f32 (+0x18), 5006 bool (skipped).
- After 1001, if bytes remain and the next token is **7000**: leaf cluster LODs (`FUN_012caa10`,
  "CTreeEngine::ParseLeafCluster"): i32 count, then `7002 { 7004 billboard-leaf* } 7003` blocks until 7001; a
  billboard leaf (`FUN_012db440`) is tokens until 7005: 7006 8 bytes (low u32 kept), 7007/7008/7011 u8, 7012 u8/255,
  7010/7015 vec3, 7013 i32, 7016 f32. Not used by the shipped files.

Then `LoadTree`'s section switch, until EOF or an unknown token (which ends parsing without error):
| token | handler | payload |
|---|---|---|
| 8000 lighting | `FUN_012ce7b0` | until 8001: 8002/8004/8007/8008 i32, 8003/8005/8009 13×f32, 8006 f32 |
| 9000 lod | `FUN_012b3730` | 9002 i32, 9003/9004 f32, 9005 engine lod (`FUN_012cb230`, until 9006: 9007/9011 i32, 9008/9010/9012–9014 f32), 9009 and unknown → f32; ends 9001 |
| 10000 texture coords | `FUN_012b3d90` | 10002/10003/10004: i32 n + n×8 f32 (10003's odd values negated when a global flag is set), 10005 str, 10006/10007 bool; ends 10001 |
| 11000 new wind | `FUN_012b43a0` | (11002 i32)+ ; ends 11001 |
| 12000 collision | `FUN_012b34d0` | 12002 sphere / 12003 capsule / 12004 box: vec3 position + 1/2/3 f32; ends 12001 |
| 13000 fronds | `FUN_012c7c50` | 13002/13003/13004/13006/13009/14007/14008 i32, 13005 spline, 13007 bool, 13010–13013 f32, 13008 textures (i32 n, n × {i32, until 14001: 14002 str, 14003–14006 f32}); ends 13001 |
| 15000 texture controls | `FUN_012cc3c0` | per branch level: 15002 bool, 15003 f32; then 15001 |
| 16000 flare | `FUN_012ca490` | per level: 16002 f32, 16003 i32, 16004–16012 f32; then 16001 |
| 16013 / 16014 | inline | i32 / f32 |
| 18000 | `FUN_012d1da0` | until 18001: 18002–18004 vec3, 18005 str |
| 19000 | `FUN_012b42e0` | until 19001: 19002 str (other tokens have no payload) |
| 20000 | `FUN_012b3be0` | only if 10000 was read: until 20001: 20002 str, 20003/20004 bool, 20005 8×f32 |
| 21000 / 21001 / 22000 | inline | f32 / f32 / bool |
| 23000 light seam | `FUN_012cb100` | per level: 23002 f32, 23003 f32; then 23001 |
| 24000 leaf placement | `FUN_012cc050` | 24002 f32, 24003 i32, 24001 |
| 25000 suppl. frond | `FUN_012c8100` | until 25001: 25002 f32, 25003–25006 i32, 25007 bool |
| 26000 suppl. branch | `FUN_012cb9e0` | per level: 26002, until 26003: 26004–26007/26010/26011/26015–26017/26021 f32, 26008/26012 i32, 26009/26023 bool, 26013/26014/26018–26020/26022 spline; then 26001 |
| 27000 floor | `FUN_012ca930` | until 27001: 27002 bool, 27003/27005/27006 f32, 27004 i32 |
| 28000 leaf normal smoothing | `FUN_012cb050` | until 28001: 28002 bool, 28003 f32, 28004 i32 |
| 29000 cluster | `FUN_012ca420` | until 29001: 29002 i32 |
| 30000 standard shader | `FUN_012b3890` | 30002–30009 f32; ends 30001 |
| 40000 root support | `FUN_012daea0` | until 40001: 40002 i32, 40003–40005 f32, 40006 branch level, 40007 supplemental (until 40008: 15002/15003/16002–16012/23002/23003/26004–26023 tokens) |
| 50000 tex coord controls | `FUN_012cc130` | for each branch level and once more (the frond level): 7 × {50002, until 50003: 50004/50005/50008/50010/50013–50017 f32, 50006/50007/50009/50011/50012/50018 bool}; then 50001 |
| 60000 map bank | `FUN_012d21f0` | until 60001: 60002 maps (branch), 60003 + 60007 + i32 n + n maps (leaf), 60004 + 60008 + n maps (frond), 60005 maps (composite), 60006 str, 60009 maps (billboard); maps = 70000, until 70001: 70002–70008 str (7 layers) |
| 71000 mesh | `FUN_012cb330` | 71001 i32; 71002 mesh until 71014: 71003 str, 71004/71012 i32, 71013 i32 index, 71005 vertex until 71011 (71006–71009 vec3, 71010 2×f32); ends 71015 |
| 72000 leaf mesh | `FUN_012cae70` | (7000 until 7001: 72001 bool, 72002/72003 i32, others f32)*, ends 72004 |
| 73000 suppl. collision | `FUN_012b39d0` | only if 12000 was read: (73002, until 73003: 73004–73006 f32)*, ends 73001 |
| 74000 suppl. global | `FUN_012cbfe0` | until 74001: 74002 f32 |
| 75000 | `FUN_012b3b40` | until 75001: 75002/75003/75005 f32, 75004 bool |

## 3. Splines (CONFIRMED, `FUN_012c1710` parse, `FUN_012c0fe0` add point, `FUN_012c11a0` sample)
Text `BezierSpline a b c { n  (x y tx ty w)×n }`; tokens split by whitespace; numbers by SpeedTreeRT's own float
parser (`FUN_012c09f0`: single-precision `v*10+d`, fraction `+= d*f, f*=0.1`, `e` exponent). Stored as
`[+0]=c, [+4]=a, [+8]=b`. Tangent normalised. Cubic segments `P[i], P[i]+T[i]w[i], P[i+1]−T[i+1]w[i+1], P[i+1]`.
After loading, the curve is walked with step `(n−1)/499` (de Casteljau) into up to 500 points, then resampled
at uniform x (`i/500`) into a 500-entry table whose first/last entries are the end points. How Compute reads the
table (index rounding, and how a, b, c apply): TODO.

## 4. Open questions
- Meaning of most `tNNNN` fields (from Compute).
- 2001/2003: INFERRED size and size variance (Austrian pine: 400.02 and 100.0).

## 4. Random numbers (CONFIRMED)
- **Newran** (Robert Davies; strings "Newran: seed out of range", "Random number generator not initialised"), one global
  instance. Seed(int s) `FUN_012dc450`: `state = s`, then 128 times `state = minstd(state)`, `buf[i] = (float)state ·
  4.656613e-10`. `minstd(s) = s·16807 + (s / 127773)·(−0x7fffffff)` in wrapping 32-bit ints, `+0x7fffffff` if ≤ 0.
  Next `FUN_012dc2a0`: `a = minstd(state)`, `i = (int)((float)a · 5.9604645e-8)`, `r = buf[i]`, `state = minstd(a)`,
  `buf[i] = (float)state · 4.656613e-10`, return `r`. Uniform(lo, hi) `FUN_012da0f0` = `r·(hi−lo)+lo` (x87).
  `FUN_012da110(-1)` seeds from the clock — only reached if nothing seeded first (`FUN_012da0d0`), never in Compute.
- **C runtime `rand()`** (MSVC LCG 214013/2531011), `srand(16013)` per tree Compute, used only by the flares.
- Three counters walk a 1000-entry table of `Uniform(0,1)` drawn right after seeding: `DAT_01843a48` (child
  probability), `DAT_01842aa4` (bark roughness), `DAT_01843a4c` (leaf mesh rotation).

## 5. Generator (`crates/ntw_formats/src/speedtree/generate.rs`, CONFIRMED arithmetic unless marked)
`CSpeedTreeRT::Compute` `FUN_012b0ee0` → `CTreeEngine::Compute` `FUN_012c9a90`:
1. seed(2005); 1000 × Uniform(0,1) into the table; seed(2005); `size = Uniform(2006−2007, 2006+2007)`;
   `srand(16013)`; leaf texture absolute sizes `= 4005 × 2006`; seed(2005); wind counter = seed.
2. Trunk: `CBranch::Compute` `FUN_012d45a0` with frame `[c, s, 0; −s, c, 0; 0, 0, 1]` (rotation by 74002°),
   up `(0,0,1)`, wind weights (1,1), wind groups (seed, 16013), parent radius −1.
3. Branch LOD strips `FUN_012c9250`/`FUN_012d7920` (9007 LODs; 9008/9012 fraction range, 9013/9014 blend).
4. `CFrondEngine::Compute` `FUN_012c70e0` (13003: 0 = blades `FUN_012c48d0`, 1 = extrusion `FUN_012c56e0`), frond LODs
   `FUN_012c6480` (13009 LODs, 13010..13013).
5. Leaf geometry `CLeafGeometry::Init` `FUN_012cf8d0`/`FUN_012cfa30`.

`CBranch::Compute(seed, size, level, origin, t, frame, dir, w, groups, parentRadius, isRoot)`, per branch:
- mode: level < 13002 → tube; else (13007 fronds on) the 25002..25005 rule (all files: frond); 2 → nothing.
- wind groups: the wind level 11002 takes a new counter value for group A, 11002+1 for group B.
- flares (tube only, `FUN_012d7550`, C rand): per flare angle, width (16005±16006 °), exponent 16007, length
  16010±16011, exponent 16012, strength 16008±16009.
- Draws in order: length = S4(t)·size; rot = U(−180,180); startAngle S7(t); gravity S1(t); radius S5(t)·size
  (clamped to 0.85 × parent radius past the cluster level, and to S5.max·size); flex S2(t); S6(0) (frond radius scale).
  Segment/cross-section reductions 26005/26006. Splines: `eval = U(−var,var)[·y] + (max−min)·y + min`, table lookup at
  `i = (int)(x·499)` with linear interpolation (`FUN_012c1540`/`FUN_012c1650`/`FUN_012c18e0`).
- Node 0: frame = parent frame rotated by `rot` about `parentFrame·dir`, then by (S7 + d2, d1) (d = disturbance
  variance draws), then the gravity bend `(2·(S8(0)−0.5))·(|90−θ|/90 − 1)·gravity·θ` about `dir × g` (θ = angle to
  gravity in degrees), then the twist 26021·S26022 (sign alternates unless 26023). Direction = frame row 0.
- Nodes i = 1..segs−1 at `te = (i/(segs−1))^16002`: radius S6(te)·r, flex S3(te)·S2, bend
  `(a·−0.0349066 − |90−θ|·a·−0.000387851)·gravity·θ` (a = S8(te)−0.5, radians), disturbance, twist; position =
  previous + previous direction × (te·length − cumulative). Floor (27002..27006) flattens descending roots.
- Rings (`FUN_012d3f30`): sides `(cross−3)·clamp(S26013(te))+3`, vertex j at angle `2π·u` (u accumulated in float),
  radius = node radius + roughness `sin(26015·t)·(26004·ratio·size·S26018)·sin(26016·t)·cos(26016·θ)` (min max radius)
  + random roughness `(2r−1)·26017·size·ratio·S26018`, × flare factor. Output space `(−x, z, y)`.
- Normals (`FUN_012d6b10`): around × along from neighbouring ring vertices, blended toward the branch direction by
  `(sqrt(c/(n−1))·(1−p)+p)·S26014`, p = S26020(t on parent).
- Children (`LAB_012d61a7`): count = (int)(6012/size·length) (+ roots 40005 at level 40002); per child reseed
  `seed += 3` (branch levels), first child in 85–95 % of the range, position U(6010, 6011), probability S26019 vs the
  table, snap to joints 26009..26012, prune 26007/26008. The last level is the leaf level (`FUN_012d6fa0`).
- Leaves: distance S4(t)·size/6009 along the frame tilted 60°; texture slot U(0,1e6)%n (blossoms 3000..3002 with
  U(0,1e5)); spacing test 3007/3008 (1 = same parent branch, 2 = whole tree); rock group U(0,1e4).
- Frond spine end (`FUN_012c7a10`): length = Σ approxSqrt; texture U(0,1e5)%n; half width = 14003·len·0.5·14004;
  angle U(14005, 14006), negated for odd frond counts.

## 6. Game renderer side (Napoleon code + shipped shaders)
- **LOD distances** (CONFIRMED): tweak variables `tree_near_distance` = 30 and `tree_far_distance` = 200 (objects
  `0x1839be0`, `0x1839330`, read each frame by `FUN_0124a8e0`). The far distance is set from a graphics option
  (`FUN_011e1220`): option 1 → 100, 2 → 150, 3 → 200, other → 50 (INFERRED to be `gfx_tree_quality`).
  `RigidModels/Vegetation/Wind/SpeedWind.ini` is the wind setup.
- **Shaders ship as source** in `data.pack` `fx\vegetation*.fx` (branch, frond, leaf card, leaf mesh, billboard,
  `vegetation.fx_fragment`). Read them for behaviour only (never commit them):
  - per instance: scale, Y rotation, slope matrix, position, LOD, alpha ref, terrain shadow;
  - wind: 6 wind matrices, vertex `wind = index·10/6 + weight` per level (two weights);
  - LOD fade: `ComputeFadeValue = lerp(0.33, 1, clamp(lod − vertexHint))`, alpha test 84/255;
  - leaf cards: corners `(±0.5, ±0.5)` + pivot, × height, rotated to the camera (azimuth/pitch + per-leaf angle
    offsets), plus a rock angle;
  - billboards: 8 pictures, the two nearest blended by azimuth (tree azimuth + camera azimuth), alpha test 84.

## 7. Wind (SpeedWind2, CONFIRMED arithmetic; `crates/ntw_formats/src/speedtree/wind.rs`)
- `RigidModels/Vegetation/Wind/SpeedWind.ini` parsed by `FUN_012b8c40` (keys: NumWindMatricesAndLeafAngles 6 8,
  WindResponseAndLimit, MaxBendAngle, Branch/LeafExponent, GustStrengthMinMax, GustDurationMinMax, GustFrequency,
  the older GustStrengthFreqDuration, BranchOscillationX/Y, LeafRocking, LeafRustling `La Ls Ha Hs`).
- `SetWindStrengthAndDirection` `FUN_012b9680`; the game calls it from `FUN_01200be0` with its Y-up direction passed as
  SpeedTree `(x, z, y)` and a strength `base · (1 + k·f(t))` clamped 0..1 (the base and f are UNKNOWN →
  PROVISIONAL 0.25 along +X; `NAPOLEON_TREE_WIND` overrides).
- `Advance` `FUN_012b7c90` (called every frame with matrices on, leaf-angle matrices off): gusts with `rand()`, damped
  followers `FUN_012b7be0` (step capped at 0.03 s), strength^exponent, bend = MaxBend·branch·|dir|, oscillators
  `FUN_012b8990` (`value[i] = sin(i + phase)·amp`), matrix i = bend about `(−cos θ, sin θ, 0)` by `oscY[i] + bend`, then
  about `(−cos θ, −sin θ, 0)` by `oscX[i]`, θ = atan2(dir.x, dir.y). Shaders use `p' = p·M`, two levels per vertex
  (`vegetation.fx_fragment` WindEffect) and fade the effect out over 200 m.
- Vertex wind weights are the exe's clamped `1 − w` (`FUN_012beed0`), groups `(index % 6) + per-tree offset`
  (offset PROVISIONAL: from the tree position).

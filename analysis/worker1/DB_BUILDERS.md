# DB_BUILDERS: DB table row layouts recovered from Napoleon.exe loader code (Worker 1)

Status: TIE-BREAKER (section 0) and steps 1–3 complete; 320 tables mapped automatically; priority tables first.

Method:
- `re_tools dbmap` (Rust) maps each `*_tables` name to its row-reader callback. The output is `db_readers.tsv`.
- Ghidra `DbReaderScan.java` decompiles each reader. It lists every stream-read call in address order, the BUILDER struct offset it writes to, and any **table-version guard**, found by dominator analysis on branches that test the header-version global. The output is `ghidra_out/db_fields_raw.tsv`.
- `re_tools dbschema` (Rust) collapses those reads into the layouts below.

All per-field rows are **CONFIRMED** from code, with two exceptions:
- "destination offset inferred" marks a string whose destination is inferred to be +0x0 (the key).
- Read order follows instruction address. That matched the decompiled control flow in every reader I checked by hand (unit_stats_land, units, projectiles, fatigue_effects, factions).


## 0. TIE-BREAKER summary (exe ground truth)

Legend: s = string (u16 len + UTF-16LE), o = optional string (u8 flag, then a string if the flag is non-zero), b = bool (1 byte), 4 = 4-byte value (int32 or float32; the exe reader does not distinguish them), {..} = version guard.

**Byte-level pitfall:**
- An **empty string** is `00 00`, which looks like two `false` bools.
- An **absent optional string** is a single `00`, which looks like one `false` bool.

Most of the disagreements below come from this.

1. **building_levels_tables** (reader 0x00DD2470), 24 columns:
   `s s 4 s 4 4 4 4 4 4 4 4 4 4 4 4 s s 4 b 4 4 4 4`
   - The W3 guess `s s i b b i×14 b i×4` is **wrong at col 3**. Col 3 is a **string** (usually empty, so it reads as `b b`).
   - Cols 4–15 are **twelve 4-byte values**, not bools.
   - Cols 16–17 are **two strings**. When they are empty they occupy the same 4 bytes that W3 read as one i32.
   - Then come 4, b, 4, 4, 4, 4. The byte totals agree with W3's when cols 3/16/17 are empty strings.
   - Verdict: use the exe layout.
2. **factions_tables** (reader 0x00F70CB0), 48 columns in the latest version:
   `s 4 s s s s s s o{v>=1} b b b s s o o 4×18 s o 4 4 4 o s b b s s s{v>=2} s{v>=2} o{v>=3}`
   - **This matches W3 except for the last column.** It is an **optional string** (present only in version ≥ 3), not a bool. A `00` byte there is an absent optional string.
   - The 18 + 3 four-byte columns are f32 according to W3. The exe does not say.
3. **ancillaries_tables** `s s s b b b 4 4 4` **matches** W3 `sssbbbiii`.
   **character_traits_tables** `s 4 b 4 s` **matches** `sibis`.
   **government_types_tables** `s b b 4 s s` **matches** `sbbiss`.
4. Battle tables. **There are no melee_weapons, missile_weapons, armour or shield tables in Napoleon.** Their stats are columns of unit_stats_land.
   - **units_tables** (0x00E85B20):
     `s s s s 4 4{v>=1; else copy of prev} 4 4 4 4 o s s s o 4 s b b b 4 b 4{v>=2} o{v>=3} b{v>=4}`
   - **unit_stats_land_tables** (LAND_UNIT, 0x00E84B00), 89 columns:
     `s 4 4 4 s o o s s s s 4 s 4 o o o s s o o o o b o o 4 4 s s o 4 o s 4 4 4 4 s s s s 4×10 b×14 4 4 4 b×14 b{v>=1} b{v>=2} o{v>=3} o{v>=4} o{v>=4} b{v>=5}`
   - **projectiles_tables** (0x00F3EF70):
     `s s s s o o o s 4 o s 4×7 o o o b b 4 4 4 o 4 4 s s o o o o{v>=1}`
   - gun_types_tables: 8×s. unit_stats_land_experience_bonuses: `s 4×8`. unit_experience_thresholds: `s 4`.
   - Per-column offsets, read sites and foreign keys are in the tables below.

## 1. File / table container format (CONFIRMED, generic loader 0x00E730D0, one template instance per record type)

Each file inside the `db/<name>_tables/` folder of the VFS is read as follows:

```
u32 a                      // if a == 0xFFFEFDFC (bytes FC FD FE FF): u32 version follows
[u32 version]              //   stored in the global 0x01766C28; when the marker is absent the stream is rewound 4 bytes and version = 0
u8  flag                   // read and kept (purpose UNKNOWN; possibly an "empty/override" flag)
u32 row_count
row_count x ROW            // ROW = per-table reader below
```

- **There is no GUID header** (later Total War games have one). Version is the only schema switch.
- Readers that branch on the version are: unit_stats_land, units, unit_stats_naval, projectiles, projectiles_explosions, factions, technologies, regions, battles, start_pos_factions, start_pos_characters, and the others marked "Versioned" below.

**Primitive readers** (CONFIRMED):

| Function | Wire format | Type |
|---|---|---|
| 0x00DD7E50 `read_string(stream, dst)` | u16 length (in UTF-16 code units) followed by length×2 bytes of UTF-16LE | CA::UniString, 12 bytes in the BUILDER |
| 0x00687CF0 `stream->read(dst, 4)` | raw 4 bytes | int32 or float32; the reader does not distinguish them (see §3) |
| 0x00DBB6B0 `stream->read(dst, 1)` | 1 byte | bool / u8 |
| 0x00687CA0 `stream->read(dst, 2)` | 2 bytes | u16 |

**Optional string** (CONFIRMED pattern): `u8 flag`. If the flag is non-zero, a string follows. If it is zero, the field is set to the empty string (`DAT_0131005f`).

Rows marked "(no file bytes)" are derived values computed after reading:
- FUN_004F3720 parses the preceding string as a decimal int.
- FUN_004F1200 / FUN_004F0660 copy a string.

## 2. Notes for priority A (battle)

- Napoleon has **no separate `melee_weapons`, `missile_weapons`, `armour` or `shield` tables**. Those stats are columns of `unit_stats_land_tables`. Missile weapons are `projectiles_tables` + `gun_types_tables` + `gun_type_to_projectiles_tables`.
- **unit_stats_land** builds `EMPIREUTILITY::LAND_UNIT_RECORD::BUILDER`. The BUILDER stride in the entry list is 0x1C4. The linker is 0x00EDB840, which calls the record constructor 0x00E8CFF0. Foreign keys resolved by the linker (CONFIRMED from the "In table %S … X_RECORD" error strings and lookup helpers):

| BUILDER offset | FK target |
|---|---|
| +0x00 | `UNIT_RECORD` (the units_tables key) |
| +0x48, +0x8C, +0xD4, +0xE0 | `BATTLE_ENTITY_RECORD` (lookup 0x00EDB6A0) |
| +0x11C | `GUN_TYPE_RECORD` |
| +0x154 | `PROJECTILE_RECORD` |
| +0x80 | `MOUNT_RECORD` |
| +0x18, +0x24, +0x30, +0x214 | lookup 0x00EDB700, the `BATTLE_PERSONALITY_RECORD` table (INFERRED from that helper's error string; that four columns share it is odd) |

  The literal key `"puckle"` is special-cased. The linker substitutes `puckle_carriage` as an entity.
- The `version >= N` guards on unit_stats_land are **4, 2, 3, 4 and >4**. These appended columns exist only in later file versions.

## 3. int32 vs float32 (UNKNOWN per field)

The 4-byte reader copies raw bits, and the linker/record constructor copies most of these fields unchanged. As a result the exe gives no type for them at load time. Worker 2 should classify them from data: float bit patterns have exponent bytes 0x3F/0x40/0x41/0x42/0xBF. I can confirm individual fields on request by tracing their runtime consumers.



## 2b. Hand-checked special cases (CONFIRMED from the decompile unless marked)

- **fatigue_effects_tables** (reader 0x00F714E0). The row is 2 strings + 4×4B.
  - The two strings are read into locals and combined into the record key `s1 + ";" + s2`. The separator literal is at 0x013305F8 = ";". The concatenation order is INFERRED.
  - Then four 4-byte values follow, stored at +0x0C, +0x10, +0x14, +0x18.
  - The table automatically lists the second string's offset as "?" because it is not stored directly.
- **regions_tables** (reader 0x00F3F950):
  1. string key at +0x00
  2. string at +0x0C
  3. **three int32** read into locals, of which only the low byte is kept: the 1st goes to +0x1A, the 2nd to +0x19 and the 3rd to +0x18. These are almost certainly an R,G,B colour (INFERRED name). The type is CONFIRMED as int-used-as-byte, not float.
  4. `version != 0`: string at +0x1C
- **units_tables**: when version == 0, the field at +0x34 is *not read* and defaults to a copy of the field at +0x30. Fields guarded by `version >= 2`, `version >= 3` and `version > 3` default to 0 / empty string / false.
- **unit_stats_land_tables**: every version-guarded field defaults to 0 / false / empty string when absent.
- **technology_effects_junction_tables** uses the same reader as building_effects_junction_tables (0x00DD2250): `string key, string effect, 4B value`. Many `*_junction` tables share one generic reader; the "same reader" line under each table lists its siblings.
- Tables listed with 1 field `string @0x0` (shared reader 0x00DD2220 or 0x00FA5680) are **key-only** tables (enums/lists).

## Priority tables


### unit_stats_land_tables

- Row reader **0xe84b00** (callback 0xedc660, table loader 0xe730d0, name getter 0xe86640). Fields read from the file: 89. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe84c88 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe84c9f |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xe84cb3 |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xe84cc7 |
| 4 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe84cd5 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xe84ced |
| 6 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x30 |  | flag==0 -> empty string | 0xe84d43 |
| 7 | string | u16 len + len*UTF-16LE | 0x3c |  |  | 0xe84d95 |
| 8 | string | u16 len + len*UTF-16LE | 0x48 |  |  | 0xe84da9 |
| 9 | string | u16 len + len*UTF-16LE | 0x54 |  |  | 0xe84dbd |
| 10 | string | u16 len + len*UTF-16LE | 0x60 |  |  | 0xe84dd1 |
| 11 | int32 or float32 | 4 | 0x6c |  |  | 0xe84de8 |
| 12 | string | u16 len + len*UTF-16LE | 0x70 |  |  | 0xe84df9 |
| 13 | int32 or float32 | 4 | 0x7c |  |  | 0xe84e10 |
| 14 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x80 |  | flag==0 -> empty string | 0xe84e25 |
| 15 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x8c |  | flag==0 -> empty string | 0xe84e81 |
| 16 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x98 |  | flag==0 -> empty string | 0xe84edd |
| 17 | string | u16 len + len*UTF-16LE | 0xbc |  |  | 0xe84f38 |
| 18 | string | u16 len + len*UTF-16LE | 0xc8 |  |  | 0xe84f4f |
| 19 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xd4 |  | flag==0 -> empty string | 0xe84f67 |
| 20 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xe0 |  | flag==0 -> empty string | 0xe84fc3 |
| 21 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xec |  | flag==0 -> empty string | 0xe8501f |
| 22 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xf8 |  | flag==0 -> empty string | 0xe8507b |
| 23 | bool | 1 | 0x1e0 |  |  | 0xe850d9 |
| 24 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x11c |  | flag==0 -> empty string | 0xe850ee |
| 25 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x128 |  | flag==0 -> empty string | 0xe8514a |
| 26 | int32 or float32 | 4 | 0x134 |  |  | 0xe851a8 |
| 27 | int32 or float32 | 4 | 0x138 |  |  | 0xe851bf |
| 28 | string | u16 len + len*UTF-16LE | 0x13c |  |  | 0xe851d3 |
| 29 | string | u16 len + len*UTF-16LE | 0x148 |  |  | 0xe851ea |
| 30 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x154 |  | flag==0 -> empty string | 0xe85202 |
| 31 | int32 or float32 | 4 | 0x160 |  |  | 0xe85260 |
| 32 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x164 |  | flag==0 -> empty string | 0xe85275 |
| 33 | string | u16 len + len*UTF-16LE | 0x170 |  |  | 0xe852d0 |
| 34 | int32 or float32 | 4 | 0x17c |  |  | 0xe852ea |
| 35 | int32 or float32 | 4 | 0x180 |  |  | 0xe85301 |
| 36 | int32 or float32 | 4 | 0x184 |  |  | 0xe85318 |
| 37 | int32 or float32 | 4 | 0x188 |  |  | 0xe8532f |
| 38 | string | u16 len + len*UTF-16LE | 0x18c |  |  | 0xe85343 |
| 39 | string | u16 len + len*UTF-16LE | 0x198 |  |  | 0xe8535a |
| 40 | string | u16 len + len*UTF-16LE | 0x1a4 |  |  | 0xe85371 |
| 41 | string | u16 len + len*UTF-16LE | 0x1b0 |  |  | 0xe85388 |
| 42 | int32 or float32 | 4 | 0x1bc |  |  | 0xe853a2 |
| 43 | int32 or float32 | 4 | 0x1c0 |  |  | 0xe853b9 |
| 44 | int32 or float32 | 4 | 0x1c4 |  |  | 0xe853d0 |
| 45 | int32 or float32 | 4 | 0x1c8 |  |  | 0xe853e7 |
| 46 | int32 or float32 | 4 | 0x1cc |  |  | 0xe853fe |
| 47 | int32 or float32 | 4 | 0x1d0 |  |  | 0xe85415 |
| 48 | int32 or float32 | 4 | 0x1d4 |  |  | 0xe8542c |
| 49 | int32 or float32 | 4 | 0x1d8 |  |  | 0xe85443 |
| 50 | int32 or float32 | 4 | 0x1dc |  |  | 0xe8545a |
| 51 | int32 or float32 | 4 | 0x1e4 |  |  | 0xe85471 |
| 52 | bool | 1 | 0x1e8 |  |  | 0xe85488 |
| 53 | bool | 1 | 0x1e9 |  |  | 0xe8549f |
| 54 | bool | 1 | 0x1ea |  |  | 0xe854b6 |
| 55 | bool | 1 | 0x1eb |  |  | 0xe854cd |
| 56 | bool | 1 | 0x1ec |  |  | 0xe854e4 |
| 57 | bool | 1 | 0x1ed |  |  | 0xe854fb |
| 58 | bool | 1 | 0x1ee |  |  | 0xe85512 |
| 59 | bool | 1 | 0x1ef |  |  | 0xe85529 |
| 60 | bool | 1 | 0x1f0 |  |  | 0xe85540 |
| 61 | bool | 1 | 0x1f1 |  |  | 0xe85557 |
| 62 | bool | 1 | 0x1f2 |  |  | 0xe8556e |
| 63 | bool | 1 | 0x1f3 |  |  | 0xe85585 |
| 64 | bool | 1 | 0x1f4 |  |  | 0xe8559c |
| 65 | bool | 1 | 0x1f5 |  |  | 0xe855b3 |
| 66 | int32 or float32 | 4 | 0x1f8 |  |  | 0xe855ca |
| 67 | int32 or float32 | 4 | 0x1fc |  |  | 0xe855e1 |
| 68 | int32 or float32 | 4 | 0x200 |  |  | 0xe855f8 |
| 69 | bool | 1 | 0x204 |  |  | 0xe8560f |
| 70 | bool | 1 | 0x205 |  |  | 0xe85626 |
| 71 | bool | 1 | 0x206 |  |  | 0xe8563d |
| 72 | bool | 1 | 0x207 |  |  | 0xe85654 |
| 73 | bool | 1 | 0x208 |  |  | 0xe8566b |
| 74 | bool | 1 | 0x209 |  |  | 0xe85682 |
| 75 | bool | 1 | 0x20a |  |  | 0xe85699 |
| 76 | bool | 1 | 0x20b |  |  | 0xe856b0 |
| 77 | bool | 1 | 0x20c |  |  | 0xe856c7 |
| 78 | bool | 1 | 0x20d |  |  | 0xe856de |
| 79 | bool | 1 | 0x20e |  |  | 0xe856f5 |
| 80 | bool | 1 | 0x20f |  |  | 0xe8570c |
| 81 | bool | 1 | 0x210 |  |  | 0xe85723 |
| 82 | bool | 1 | 0x211 |  |  | 0xe8573a |
| 83 | bool | 1 | 0x212 | version != 0 |  | 0xe8575a |
| 84 | bool | 1 | 0x213 | version >= 2 |  | 0xe8577f |
| 85 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x214 | version >= 3 | flag==0 -> empty string | 0xe857a2 |
| 86 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x220 | version >= 4 | flag==0 -> empty string | 0xe85801 |
| 87 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x22c | version >= 4 | flag==0 -> empty string | 0xe8585d |
| 88 | bool | 1 | 0x238 | version > 4 |  | 0xe858eb |

### units_tables

- Row reader **0xe85b20** (callback 0xedc6b0, table loader 0xe778a0, name getter 0xe86aa0). Fields read from the file: 25. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe85b9b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe85bac |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe85bbd |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xe85bce |
| 4 | int32 or float32 | 4 | 0x30 |  |  | 0xe85be9 |
| 5 | int32 or float32 | 4 | 0x34 | version != 0 |  | 0xe85c06 |
| 6 | int32 or float32 | 4 | 0x38 |  |  | 0xe85c20 |
| 7 | int32 or float32 | 4 | 0x3c |  |  | 0xe85c34 |
| 8 | int32 or float32 | 4 | 0x40 |  |  | 0xe85c48 |
| 9 | int32 or float32 | 4 | 0x44 |  |  | 0xe85c5c |
| 10 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x48 |  | flag==0 -> empty string | 0xe85c71 |
| 11 | string | u16 len + len*UTF-16LE | 0x54 |  |  | 0xe85cc3 |
| 12 | string | u16 len + len*UTF-16LE | 0x60 |  |  | 0xe85cd7 |
| 13 | string | u16 len + len*UTF-16LE | 0x6c |  |  | 0xe85ceb |
| 14 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x78 |  | flag==0 -> empty string | 0xe85d03 |
| 15 | int32 or float32 | 4 | 0x84 |  |  | 0xe85d5b |
| 16 | string | u16 len + len*UTF-16LE | 0x88 |  |  | 0xe85d6f |
| 17 | bool | 1 | 0x94 |  |  | 0xe85d89 |
| 18 | bool | 1 | 0x95 |  |  | 0xe85da0 |
| 19 | bool | 1 | 0x96 |  |  | 0xe85db7 |
| 20 | int32 or float32 | 4 | 0x98 |  |  | 0xe85dce |
| 21 | bool | 1 | 0x9c |  |  | 0xe85de5 |
| 22 | int32 or float32 | 4 | 0xa0 | version >= 2 |  | 0xe85e05 |
| 23 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xa4 | version >= 3 | flag==0 -> empty string | 0xe85e2b |
| 24 | bool | 1 | 0xb0 | version > 3 |  | 0xe85e9d |

### projectiles_tables

- Row reader **0xf3ef70** (callback 0xf58f60, table loader 0xf33d50, name getter 0xf40a20). Fields read from the file: 35. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3f063 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3f074 |
| 2 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf3f085 |
| 3 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xf3f096 |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x18 |  | flag==0 -> empty string | 0xf3f0ae |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x3c |  | flag==0 -> empty string | 0xf3f105 |
| 6 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x48 |  | flag==0 -> empty string | 0xf3f15c |
| 7 | string | u16 len + len*UTF-16LE | 0x54 |  |  | 0xf3f1af |
| 8 | int32 or float32 | 4 | 0x60 |  |  | 0xf3f1ca |
| 9 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x64 |  | flag==0 -> empty string | 0xf3f1df |
| 10 | string | u16 len + len*UTF-16LE | 0x70 |  |  | 0xf3f231 |
| 11 | int32 or float32 | 4 | 0x7c |  |  | 0xf3f248 |
| 12 | int32 or float32 | 4 | 0x80 |  |  | 0xf3f25f |
| 13 | int32 or float32 | 4 | 0x84 |  |  | 0xf3f276 |
| 14 | int32 or float32 | 4 | 0x88 |  |  | 0xf3f28d |
| 15 | int32 or float32 | 4 | 0x8c |  |  | 0xf3f2a4 |
| 16 | int32 or float32 | 4 | 0x90 |  |  | 0xf3f2bb |
| 17 | int32 or float32 | 4 | 0x94 |  |  | 0xf3f2d2 |
| 18 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x98 |  | flag==0 -> empty string | 0xf3f2e7 |
| 19 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xa4 |  | flag==0 -> empty string | 0xf3f343 |
| 20 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xb0 |  | flag==0 -> empty string | 0xf3f39f |
| 21 | bool | 1 | 0xbc |  |  | 0xf3f3fd |
| 22 | bool | 1 | 0xbd |  |  | 0xf3f414 |
| 23 | int32 or float32 | 4 | 0xc0 |  |  | 0xf3f42b |
| 24 | int32 or float32 | 4 | 0xc4 |  |  | 0xf3f442 |
| 25 | int32 or float32 | 4 | 0xc8 |  |  | 0xf3f459 |
| 26 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xcc |  | flag==0 -> empty string | 0xf3f46e |
| 27 | int32 or float32 | 4 | 0xd8 |  |  | 0xf3f4cc |
| 28 | int32 or float32 | 4 | 0xdc |  |  | 0xf3f4e3 |
| 29 | string | u16 len + len*UTF-16LE | 0xec |  |  | 0xf3f4f7 |
| 30 | string | u16 len + len*UTF-16LE | 0xf8 |  |  | 0xf3f50e |
| 31 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xe0 |  | flag==0 -> empty string | 0xf3f526 |
| 32 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x104 |  | flag==0 -> empty string | 0xf3f582 |
| 33 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x110 |  | flag==0 -> empty string | 0xf3f5de |
| 34 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x11c | version != 0 | flag==0 -> empty string | 0xf3f643 |

### gun_types_tables

- Row reader **0xf3afa0** (callback 0xf58e90, table loader 0xe7c0d0, name getter 0xf40110). Fields read from the file: 8.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3b00f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3b018 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3b021 |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf3b02a |
| 4 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xf3b033 |
| 5 | string | u16 len + len*UTF-16LE | 0x3c |  |  | 0xf3b03c |
| 6 | string | u16 len + len*UTF-16LE | 0x48 |  |  | 0xf3b045 |
| 7 | string | u16 len + len*UTF-16LE | 0x54 |  |  | 0xf3b04e |

### gun_type_to_projectiles_tables

- Row reader **0xdd29f0** (callback 0xde26b0, table loader 0xdc8f80, name getter 0xf40160). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: anim_reference_poses_tables, campaign_anim_action_to_sets_tables, campaign_map_towns_and_ports_tables, start_pos_character_ancillaries_tables, start_pos_character_traits_tables, start_pos_naval_units_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2a1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2a24 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2a2d |

### unit_stats_land_experience_bonuses_tables

- Row reader **0xe85f40** (callback 0xedc6d0, table loader 0xe79d30, name getter 0xe86b90). Fields read from the file: 9.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe85f56 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe85f6d |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xe85f81 |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xe85f95 |
| 4 | int32 or float32 | 4 | 0x18 |  |  | 0xe85fa9 |
| 5 | int32 or float32 | 4 | 0x1c |  |  | 0xe85fbd |
| 6 | int32 or float32 | 4 | 0x20 |  |  | 0xe85fd1 |
| 7 | int32 or float32 | 4 | 0x24 |  |  | 0xe85fe5 |
| 8 | int32 or float32 | 4 | 0x28 |  |  | 0xe85ff9 |

### unit_experience_thresholds_tables

- Row reader **0xdf0990** (callback 0xe29820, table loader 0xdc7db0, name getter 0xe86a00). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_spawnings_tables, ministerial_positions_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdf09a3 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xdf09b3 |

### fatigue_effects_tables

- Row reader **0xf714e0** (callback 0xf882e0, table loader 0xe766d0, name getter 0xf724b0). Fields read from the file: 6.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xf7150f |
| 1 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xf71524 |
| 2 | int32 or float32 | 4 | 0xc |  |  | 0xf7159a |
| 3 | int32 or float32 | 4 | 0x10 |  |  | 0xf715ae |
| 4 | int32 or float32 | 4 | 0x14 |  |  | 0xf715c2 |
| 5 | int32 or float32 | 4 | 0x18 |  |  | 0xf715d6 |

### unit_stats_naval_tables

- Row reader **0xf05ba0** (callback 0xf21880, table loader 0xef37a0, name getter 0xf08b20). Fields read from the file: 121. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf05e70 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf05e87 |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xf05e9b |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xf05eaf |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x18 |  | flag==0 -> empty string | 0xf05ec4 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xf05f16 |
| 6 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x30 |  | flag==0 -> empty string | 0xf05f68 |
| 7 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x3c |  | flag==0 -> empty string | 0xf05fbe |
| 8 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x48 |  | flag==0 -> empty string | 0xf06014 |
| 9 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x54 |  | flag==0 -> empty string | 0xf0606a |
| 10 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x60 |  | flag==0 -> empty string | 0xf060c0 |
| 11 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x6c |  | flag==0 -> empty string | 0xf06116 |
| 12 | string | u16 len + len*UTF-16LE | 0x78 |  |  | 0xf06168 |
| 13 | string | u16 len + len*UTF-16LE | 0x84 |  |  | 0xf0617f |
| 14 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x90 |  | flag==0 -> empty string | 0xf06197 |
| 15 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x9c |  | flag==0 -> empty string | 0xf061f3 |
| 16 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xa8 |  | flag==0 -> empty string | 0xf0624f |
| 17 | int32 or float32 | 4 | 0xb4 |  |  | 0xf062ad |
| 18 | int32 or float32 | 4 | 0xb8 |  |  | 0xf062c4 |
| 19 | int32 or float32 | 4 | 0xbc |  |  | 0xf062db |
| 20 | int32 or float32 | 4 | 0xc0 |  |  | 0xf062f2 |
| 21 | int32 or float32 | 4 | 0xc4 |  |  | 0xf06309 |
| 22 | string | u16 len + len*UTF-16LE | 0xc8 |  |  | 0xf0631d |
| 23 | int32 or float32 | 4 | 0xd4 |  |  | 0xf06337 |
| 24 | int32 or float32 | 4 | 0xd8 |  |  | 0xf0634e |
| 25 | int32 or float32 | 4 | 0xdc |  |  | 0xf06365 |
| 26 | int32 or float32 | 4 | 0xe0 |  |  | 0xf0637c |
| 27 | int32 or float32 | 4 | 0xe4 |  |  | 0xf06393 |
| 28 | int32 or float32 | 4 | 0xe8 |  |  | 0xf063aa |
| 29 | int32 or float32 | 4 | 0xec |  |  | 0xf063c1 |
| 30 | int32 or float32 | 4 | 0xf0 |  |  | 0xf063d8 |
| 31 | int32 or float32 | 4 | 0xf4 |  |  | 0xf063ef |
| 32 | int32 or float32 | 4 | 0xf8 |  |  | 0xf06406 |
| 33 | int32 or float32 | 4 | 0xfc |  |  | 0xf0641d |
| 34 | int32 or float32 | 4 | 0x100 |  |  | 0xf06434 |
| 35 | int32 or float32 | 4 | 0x104 |  |  | 0xf0644b |
| 36 | int32 or float32 | 4 | 0x108 |  |  | 0xf06462 |
| 37 | int32 or float32 | 4 | 0x10c |  |  | 0xf06479 |
| 38 | int32 or float32 | 4 | 0x110 |  |  | 0xf06490 |
| 39 | int32 or float32 | 4 | 0x114 |  |  | 0xf064a7 |
| 40 | int32 or float32 | 4 | 0x118 |  |  | 0xf064be |
| 41 | int32 or float32 | 4 | 0x11c |  |  | 0xf064d5 |
| 42 | int32 or float32 | 4 | 0x120 |  |  | 0xf064ec |
| 43 | int32 or float32 | 4 | 0x124 |  |  | 0xf06503 |
| 44 | int32 or float32 | 4 | 0x128 |  |  | 0xf0651a |
| 45 | int32 or float32 | 4 | 0x12c |  |  | 0xf06531 |
| 46 | int32 or float32 | 4 | 0x130 |  |  | 0xf06548 |
| 47 | int32 or float32 | 4 | 0x134 |  |  | 0xf0655f |
| 48 | int32 or float32 | 4 | 0x138 |  |  | 0xf06576 |
| 49 | int32 or float32 | 4 | 0x13c |  |  | 0xf0658d |
| 50 | int32 or float32 | 4 | 0x140 |  |  | 0xf065a4 |
| 51 | int32 or float32 | 4 | 0x144 |  |  | 0xf065bb |
| 52 | int32 or float32 | 4 | 0x148 |  |  | 0xf065d2 |
| 53 | int32 or float32 | 4 | 0x14c |  |  | 0xf065e9 |
| 54 | int32 or float32 | 4 | 0x150 |  |  | 0xf06600 |
| 55 | int32 or float32 | 4 | 0x154 |  |  | 0xf06617 |
| 56 | int32 or float32 | 4 | 0x158 |  |  | 0xf0662e |
| 57 | int32 or float32 | 4 | 0x15c |  |  | 0xf06645 |
| 58 | int32 or float32 | 4 | 0x160 |  |  | 0xf0665c |
| 59 | int32 or float32 | 4 | 0x164 |  |  | 0xf06673 |
| 60 | bool | 1 | 0x168 |  |  | 0xf0668a |
| 61 | int32 or float32 | 4 | 0x16c |  |  | 0xf066a1 |
| 62 | int32 or float32 | 4 | 0x170 |  |  | 0xf066b8 |
| 63 | int32 or float32 | 4 | 0x174 |  |  | 0xf066cf |
| 64 | int32 or float32 | 4 | 0x178 |  |  | 0xf066e6 |
| 65 | int32 or float32 | 4 | 0x17c |  |  | 0xf066fd |
| 66 | int32 or float32 | 4 | 0x180 |  |  | 0xf06714 |
| 67 | int32 or float32 | 4 | 0x184 |  |  | 0xf0672b |
| 68 | int32 or float32 | 4 | 0x188 |  |  | 0xf06742 |
| 69 | int32 or float32 | 4 | 0x18c |  |  | 0xf06759 |
| 70 | int32 or float32 | 4 | 0x190 |  |  | 0xf06770 |
| 71 | int32 or float32 | 4 | 0x194 |  |  | 0xf06787 |
| 72 | int32 or float32 | 4 | 0x198 |  |  | 0xf0679e |
| 73 | int32 or float32 | 4 | 0x19c |  |  | 0xf067b5 |
| 74 | int32 or float32 | 4 | 0x1a0 |  |  | 0xf067cc |
| 75 | int32 or float32 | 4 | 0x1a4 |  |  | 0xf067e3 |
| 76 | int32 or float32 | 4 | 0x1a8 |  |  | 0xf067fa |
| 77 | int32 or float32 | 4 | 0x1ac |  |  | 0xf06811 |
| 78 | int32 or float32 | 4 | 0x1b0 |  |  | 0xf06828 |
| 79 | int32 or float32 | 4 | 0x1b4 |  |  | 0xf0683f |
| 80 | int32 or float32 | 4 | 0x1b8 |  |  | 0xf06856 |
| 81 | int32 or float32 | 4 | 0x1bc |  |  | 0xf0686d |
| 82 | int32 or float32 | 4 | 0x1c0 |  |  | 0xf06884 |
| 83 | int32 or float32 | 4 | 0x1c4 |  |  | 0xf0689b |
| 84 | int32 or float32 | 4 | 0x1c8 |  |  | 0xf068b2 |
| 85 | int32 or float32 | 4 | 0x1cc |  |  | 0xf068c9 |
| 86 | int32 or float32 | 4 | 0x1d0 |  |  | 0xf068e0 |
| 87 | int32 or float32 | 4 | 0x1d4 |  |  | 0xf068f7 |
| 88 | int32 or float32 | 4 | 0x1d8 |  |  | 0xf0690e |
| 89 | int32 or float32 | 4 | 0x1dc |  |  | 0xf06925 |
| 90 | int32 or float32 | 4 | 0x1e0 |  |  | 0xf0693c |
| 91 | int32 or float32 | 4 | 0x1f0 |  |  | 0xf06953 |
| 92 | int32 or float32 | 4 | 0x1f4 |  |  | 0xf0696a |
| 93 | int32 or float32 | 4 | 0x1f8 |  |  | 0xf06981 |
| 94 | int32 or float32 | 4 | 0x1fc |  |  | 0xf06998 |
| 95 | int32 or float32 | 4 | 0x200 |  |  | 0xf069af |
| 96 | int32 or float32 | 4 | 0x204 |  |  | 0xf069c6 |
| 97 | int32 or float32 | 4 | 0x208 |  |  | 0xf069dd |
| 98 | int32 or float32 | 4 | 0x20c |  |  | 0xf069f4 |
| 99 | int32 or float32 | 4 | 0x210 |  |  | 0xf06a0b |
| 100 | int32 or float32 | 4 | 0x22c |  |  | 0xf06a22 |
| 101 | int32 or float32 | 4 | 0x230 |  |  | 0xf06a39 |
| 102 | int32 or float32 | 4 | 0x234 |  |  | 0xf06a50 |
| 103 | int32 or float32 | 4 | 0x1e4 |  |  | 0xf06a67 |
| 104 | int32 or float32 | 4 | 0x1e8 |  |  | 0xf06a7e |
| 105 | int32 or float32 | 4 | 0x1ec |  |  | 0xf06a95 |
| 106 | int32 or float32 | 4 | 0x214 |  |  | 0xf06aac |
| 107 | int32 or float32 | 4 | 0x218 |  |  | 0xf06ac3 |
| 108 | int32 or float32 | 4 | 0x21c |  |  | 0xf06ada |
| 109 | int32 or float32 | 4 | 0x220 |  |  | 0xf06af1 |
| 110 | int32 or float32 | 4 | 0x224 |  |  | 0xf06b08 |
| 111 | int32 or float32 | 4 | 0x228 |  |  | 0xf06b1f |
| 112 | int32 or float32 | 4 | 0x238 |  |  | 0xf06b36 |
| 113 | int32 or float32 | 4 | 0x23c |  |  | 0xf06b4d |
| 114 | int32 or float32 | 4 | 0x240 |  |  | 0xf06b64 |
| 115 | bool | 1 | 0x244 |  |  | 0xf06b7b |
| 116 | bool | 1 | 0x245 |  |  | 0xf06b92 |
| 117 | bool | 1 | 0x246 |  |  | 0xf06ba9 |
| 118 | string | u16 len + len*UTF-16LE | 0x248 |  |  | 0xf06bbd |
| 119 | int32 or float32 | 4 | 0x254 | version != 0 |  | 0xf06be0 |
| 120 | string | u16 len + len*UTF-16LE | 0x258 | version > 1 |  | 0xf06c05 |

### mounts_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf40570). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### battle_entities_tables

- Row reader **0xe54370** (callback 0xe69210, table loader 0xe3fb90, name getter 0xe55590). Fields read from the file: 21.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe543a6 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe543af |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe543b8 |
| 3 | int32 or float32 | 4 | 0x24 |  |  | 0xe543c8 |
| 4 | int32 or float32 | 4 | 0x28 |  |  | 0xe543d5 |
| 5 | int32 or float32 | 4 | 0x2c |  |  | 0xe543e2 |
| 6 | int32 or float32 | 4 | 0x30 |  |  | 0xe543ef |
| 7 | int32 or float32 | 4 | 0x34 |  |  | 0xe543fc |
| 8 | int32 or float32 | 4 | 0x38 |  |  | 0xe54409 |
| 9 | int32 or float32 | 4 | 0x3c |  |  | 0xe54416 |
| 10 | int32 or float32 | 4 | 0x40 |  |  | 0xe54423 |
| 11 | int32 or float32 | 4 | 0x44 |  |  | 0xe54430 |
| 12 | int32 or float32 | 4 | 0x48 |  |  | 0xe5443d |
| 13 | string | u16 len + len*UTF-16LE | 0x4c |  |  | 0xe54444 |
| 14 | int32 or float32 | 4 | 0x58 |  |  | 0xe54454 |
| 15 | int32 or float32 | 4 | 0x5c |  |  | 0xe54461 |
| 16 | int32 or float32 | 4 | 0x60 |  |  | 0xe5446e |
| 17 | int32 or float32 | 4 | 0x64 |  |  | 0xe5447b |
| 18 | int32 or float32 | 4 | 0x68 |  |  | 0xe54488 |
| 19 | int32 or float32 | 4 | 0x6c |  |  | 0xe54495 |
| 20 | int32 or float32 | 4 | 0x70 |  |  | 0xe544a2 |

### factions_tables

- Row reader **0xf70cb0** (callback 0xf882c0, table loader 0xf6cac0, name getter 0xf72410). Fields read from the file: 48. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf70def |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf70e0c |
| 2 | string | u16 len + len*UTF-16LE | 0x10 |  |  | 0xf70e1e |
| 3 | string | u16 len + len*UTF-16LE | 0x1c |  |  | 0xf70e2f |
| 4 | string | u16 len + len*UTF-16LE | 0x28 |  |  | 0xf70e40 |
| 5 | string | u16 len + len*UTF-16LE | 0x34 |  |  | 0xf70e51 |
| 6 | string | u16 len + len*UTF-16LE | 0x40 |  |  | 0xf70e69 |
| 7 | string | u16 len + len*UTF-16LE | 0x4c |  |  | 0xf70e7d |
| 8 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x58 | version != 0 | flag==0 -> empty string | 0xf70e9e |
| 9 | bool | 1 | 0x64 |  |  | 0xf70ef3 |
| 10 | bool | 1 | 0x65 |  |  | 0xf70f07 |
| 11 | bool | 1 | 0x66 |  |  | 0xf70f1b |
| 12 | string | u16 len + len*UTF-16LE | 0x6c |  |  | 0xf70f2c |
| 13 | string | u16 len + len*UTF-16LE | 0x78 |  |  | 0xf70f40 |
| 14 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x84 |  | flag==0 -> empty string | 0xf70f58 |
| 15 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x90 |  | flag==0 -> empty string | 0xf70fb4 |
| 16 | int32 or float32 | 4 | 0x9c |  |  | 0xf71012 |
| 17 | int32 or float32 | 4 | 0xa0 |  |  | 0xf71029 |
| 18 | int32 or float32 | 4 | 0xa4 |  |  | 0xf71040 |
| 19 | int32 or float32 | 4 | 0xc0 |  |  | 0xf71057 |
| 20 | int32 or float32 | 4 | 0xc4 |  |  | 0xf7106e |
| 21 | int32 or float32 | 4 | 0xc8 |  |  | 0xf71085 |
| 22 | int32 or float32 | 4 | 0xa8 |  |  | 0xf7109c |
| 23 | int32 or float32 | 4 | 0xac |  |  | 0xf710b3 |
| 24 | int32 or float32 | 4 | 0xb0 |  |  | 0xf710ca |
| 25 | int32 or float32 | 4 | 0xcc |  |  | 0xf710e1 |
| 26 | int32 or float32 | 4 | 0xd0 |  |  | 0xf710f8 |
| 27 | int32 or float32 | 4 | 0xd4 |  |  | 0xf7110f |
| 28 | int32 or float32 | 4 | 0xb4 |  |  | 0xf71126 |
| 29 | int32 or float32 | 4 | 0xb8 |  |  | 0xf7113d |
| 30 | int32 or float32 | 4 | 0xbc |  |  | 0xf71154 |
| 31 | int32 or float32 | 4 | 0xd8 |  |  | 0xf7116b |
| 32 | int32 or float32 | 4 | 0xdc |  |  | 0xf71182 |
| 33 | int32 or float32 | 4 | 0xe0 |  |  | 0xf71199 |
| 34 | string | u16 len + len*UTF-16LE | 0xe4 |  |  | 0xf711ad |
| 35 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xfc |  | flag==0 -> empty string | 0xf711c5 |
| 36 | int32 or float32 | 4 | 0x108 |  |  | 0xf71223 |
| 37 | int32 or float32 | 4 | 0x10c |  |  | 0xf7123a |
| 38 | int32 or float32 | 4 | 0x110 |  |  | 0xf71251 |
| 39 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x114 |  | flag==0 -> empty string | 0xf71266 |
| 40 | string | u16 len + len*UTF-16LE | 0x120 |  |  | 0xf712c1 |
| 41 | bool | 1 | 0x67 |  |  | 0xf712d8 |
| 42 | bool | 1 | 0x68 |  |  | 0xf712ec |
| 43 | string | u16 len + len*UTF-16LE | 0x12c |  |  | 0xf71300 |
| 44 | string | u16 len + len*UTF-16LE | 0x138 |  |  | 0xf71317 |
| 45 | string | u16 len + len*UTF-16LE | 0x144 | version > 1 |  | 0xf71338 |
| 46 | string | u16 len + len*UTF-16LE | 0x150 | version > 1 |  | 0xf7134f |
| 47 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x160 | version > 2 | flag==0 -> empty string | 0xf71378 |

### regions_tables

- Row reader **0xf3f950** (callback 0xf58fa0, table loader 0xe4c220, name getter 0xf40ca0). Fields read from the file: 6. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3f97f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3f990 |
| 2 | int32 or float32 | 4 | local |  |  | 0xf3f9a8 |
| 3 | int32 or float32 | 4 | local |  |  | 0xf3f9bd |
| 4 | int32 or float32 | 4 | local |  |  | 0xf3f9d2 |
| 5 | string | u16 len + len*UTF-16LE | 0x1c | version != 0 |  | 0xf3f9e9 |

### building_levels_tables

- Row reader **0xdd2470** (callback 0xde2640, table loader 0xdbffd0, name getter 0xdd3430). Fields read from the file: 24.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd24b9 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd24c2 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd24d6 |
| 3 | string | u16 len + len*UTF-16LE | 0x1c |  |  | 0xdd24dd |
| 4 | int32 or float32 | 4 | 0x28 |  |  | 0xdd24f1 |
| 5 | int32 or float32 | 4 | 0x2c |  |  | 0xdd24fe |
| 6 | int32 or float32 | 4 | 0x30 |  |  | 0xdd250b |
| 7 | int32 or float32 | 4 | 0x34 |  |  | 0xdd2518 |
| 8 | int32 or float32 | 4 | 0x38 |  |  | 0xdd2525 |
| 9 | int32 or float32 | 4 | 0x3c |  |  | 0xdd2532 |
| 10 | int32 or float32 | 4 | 0x40 |  |  | 0xdd253f |
| 11 | int32 or float32 | 4 | 0x44 |  |  | 0xdd254c |
| 12 | int32 or float32 | 4 | 0x48 |  |  | 0xdd2559 |
| 13 | int32 or float32 | 4 | 0x4c |  |  | 0xdd2566 |
| 14 | int32 or float32 | 4 | 0x50 |  |  | 0xdd2573 |
| 15 | int32 or float32 | 4 | 0x54 |  |  | 0xdd2580 |
| 16 | string | u16 len + len*UTF-16LE | 0x58 |  |  | 0xdd2588 |
| 17 | string | u16 len + len*UTF-16LE | 0x68 |  |  | 0xdd2591 |
| 18 | int32 or float32 | 4 | 0x64 |  |  | 0xdd25a1 |
| 19 | bool | 1 | 0x74 |  |  | 0xdd25b5 |
| 20 | int32 or float32 | 4 | 0x78 |  |  | 0xdd25c9 |
| 21 | int32 or float32 | 4 | 0x7c |  |  | 0xdd25dd |
| 22 | int32 or float32 | 4 | 0x80 |  |  | 0xdd25f4 |
| 23 | int32 or float32 | 4 | 0x84 |  |  | 0xdd260b |

### building_effects_junction_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xdd3340). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### technologies_tables

- Row reader **0xf086c0** (callback 0xf219d0, table loader 0xf02080, name getter 0xf09610). Fields read from the file: 13. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf086f4 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf08709 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xf08720 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xf08734 |
| 4 | string | u16 len + len*UTF-16LE | 0x20 |  |  | 0xf08742 |
| 5 | int32 or float32 | 4 | 0x2c |  |  | 0xf08759 |
| 6 | int32 or float32 | 4 | 0x30 |  |  | 0xf0876d |
| 7 | int32 or float32 | 4 | 0x34 |  |  | 0xf08781 |
| 8 | int32 or float32 | 4 | 0x38 |  |  | 0xf08795 |
| 9 | bool | 1 | 0x3c |  |  | 0xf087a9 |
| 10 | bool | 1 | 0x3d |  |  | 0xf087bd |
| 11 | string | u16 len + len*UTF-16LE | 0x40 |  |  | 0xf087cb |
| 12 | int32 or float32 | 4 | 0x4c | version != 0 |  | 0xf087ed |

### technology_effects_junction_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf09570). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### campaign_variables_tables

- Row reader **0xf08360** (callback 0xf21990, table loader 0xdc7db0, name getter 0xf716f0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: diplomatic_relations_attitudes_tables, entity_training_levels_tables, state_gift_values_tables, taxes_levels_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf08373 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf0838c |

## All other tables (automatic, same method)


### abilities_tables

- Row reader **0xdf0600** (callback 0xe29800, table loader 0xdc3590, name getter 0xdf09c0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdf062f |
| 1 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xc |  | flag==0 -> empty string | 0xdf0647 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdf0692 |
| 3 | bool | 1 | 0x24 |  |  | 0xdf06a9 |

### achievements_tables

- Row reader **0xfa5680** (callback 0xfa5c40, table loader 0xfa4220, name getter 0xfa5730). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: terrain_tilesets_tables, trade_node_groups_tables, unit_info_card_abilities_strings_tables, unit_special_ability_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfa56a9 |

### advice_levels_tables

- Row reader **0xdf06c0** (callback 0xe29810, table loader 0xdee5c0, name getter 0xdf0a10). Fields read from the file: 21. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdf0747 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdf075c |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdf0777 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xdf078f |
| 4 | string | u16 len + len*UTF-16LE | 0x20 |  |  | 0xdf079d |
| 5 | string | u16 len + len*UTF-16LE | 0x2c |  |  | 0xdf07ae |
| 6 | string | u16 len + len*UTF-16LE | 0x38 |  |  | 0xdf07bf |
| 7 | int32 or float32 | 4 | 0x44 |  |  | 0xdf07da |
| 8 | int32 or float32 | 4 | 0x48 |  |  | 0xdf07ee |
| 9 | bool | 1 | 0x4c |  |  | 0xdf0802 |
| 10 | int32 or float32 | 4 | 0x50 |  |  | 0xdf0816 |
| 11 | bool | 1 | 0x54 |  |  | 0xdf082a |
| 12 | bool | 1 | 0x55 |  |  | 0xdf083e |
| 13 | string | u16 len + len*UTF-16LE | 0x58 |  |  | 0xdf084f |
| 14 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x64 |  | flag==0 -> empty string | 0xdf0867 |
| 15 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x70 |  | flag==0 -> empty string | 0xdf08bd |
| 16 | bool | 1 | 0x7c |  |  | 0xdf0912 |
| 17 | bool | 1 | 0x7d |  |  | 0xdf0926 |
| 18 | string | u16 len + len*UTF-16LE | 0x80 |  |  | 0xdf093a |
| 19 | string | u16 len + len*UTF-16LE | 0x8c | version != 0 |  | 0xdf095a |
| 20 | bool | 1 | 0x98 | version > 1 |  | 0xdf097e |

### advice_threads_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xdf0a60). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### advisors_tables

- Row reader **0xfa3c40** (callback 0xfa4100, table loader 0xfa27e0, name getter 0xfa3cc0). Fields read from the file: 2. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfa3c61 |
| 1 | string | u16 len + len*UTF-16LE | 0xc | version != 0 |  | 0xfa3c7d |

### agent_attribute_situations_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdcc540, name getter 0xf94c60). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### agent_attributes_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdcc540, name getter 0xf94c10). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### agent_culture_details_tables

- Row reader **0xf94860** (callback 0xf9dad0, table loader 0xdbedb0, name getter 0xf94cb0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf9489f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf948b4 |
| 2 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf948c5 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x30 |  | flag==0 -> empty string | 0xf948dd |

### agent_spawning_to_building_chains_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf94df0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### agent_spawning_to_government_types_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf94d50). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### agent_spawning_to_policies_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf94e40). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### agent_spawnings_tables

- Row reader **0xdf0990** (callback 0xe29820, table loader 0xdc7db0, name getter 0xf94da0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ministerial_positions_tables, unit_experience_thresholds_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdf09a3 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xdf09b3 |

### agent_to_agent_abilities_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf94e90). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### agent_to_agent_attributes_tables

- Row reader **0xf94b10** (callback 0xf9daf0, table loader 0xf8f6c0, name getter 0xf94ee0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf94b2f |
| 1 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf94b38 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xf94b48 |
| 3 | bool | 1 | 0x1c |  |  | 0xf94b55 |

### agent_to_bribe_actions_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf94f30). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### agent_to_building_levels_tables

- Row reader **0xf07a30** (callback 0xf21910, table loader 0xf908e0, name getter 0xf94f80). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: start_pos_fort_garrisons_tables, start_pos_land_units_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf07a5b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf07a64 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf07a6d |
| 3 | int32 or float32 | 4 | 0x24 |  |  | 0xf07a7d |

### agents_tables

- Row reader **0xf94940** (callback 0xf9dae0, table loader 0xf8e4f0, name getter 0xf94d00). Fields read from the file: 12. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf94987 |
| 1 | int32 or float32 | 4 | 0x18 |  |  | 0xf9499e |
| 2 | int32 or float32 | 4 | 0x1c |  |  | 0xf949b2 |
| 3 | int32 or float32 | 4 | 0x20 |  |  | 0xf949c6 |
| 4 | bool | 1 | 0x24 |  |  | 0xf949da |
| 5 | string | u16 len + len*UTF-16LE | 0x28 |  |  | 0xf949e8 |
| 6 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x34 |  | flag==0 -> empty string | 0xf94a00 |
| 7 | bool | 1 | 0x40 |  |  | 0xf94a51 |
| 8 | string | u16 len + len*UTF-16LE | 0x44 |  |  | 0xf94a62 |
| 9 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x50 |  | flag==0 -> empty string | 0xf94a7a |
| 10 | int32 or float32 | 4 | 0x5c |  |  | 0xf94acf |
| 11 | int32 or float32 | 4 | 0x60 | version != 0 |  | 0xf94aec |

### aide_de_camp_speeches_tables

- Row reader **0xf8c590** (callback 0xf8cb30, table loader 0xf8b0c0, name getter 0xf8c640). Fields read from the file: 3. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| - | (no file bytes) | 0 | 0x10 |  | derived/nested call writing (call:FUN_00e576e0) | 0xf8c5c3 |
| 0 | bool | 1 | 0x20 |  |  | 0xf8c5da |
| 1 | int32 or float32 | 4 | 0x24 | version != 0 |  | 0xf8c5fa |
| 2 | int32 or float32 | 4 | 0x28 | version != 0 |  | 0xf8c60b |
| - | (no file bytes) | 0 | 0x0 |  | derived: string copy into (call:FUN_004f0240) | 0xf8c625 |

### ancillaries_tables

- Row reader **0xf94b70** (callback 0xf9db00, table loader 0xf91b00, name getter 0xf95070). Fields read from the file: 9.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf94b9b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf94ba4 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf94bad |
| 3 | bool | 1 | 0x24 |  |  | 0xf94bbd |
| 4 | bool | 1 | 0x25 |  |  | 0xf94bca |
| 5 | bool | 1 | 0x26 |  |  | 0xf94bd7 |
| 6 | int32 or float32 | 4 | 0x28 |  |  | 0xf94be4 |
| 7 | int32 or float32 | 4 | 0x2c |  |  | 0xf94bf1 |
| 8 | int32 or float32 | 4 | 0x30 |  |  | 0xf94bfe |

### ancillary_included_subcultures_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf94fd0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### ancillary_info_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf95020). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### ancillary_to_ability_effects_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf950c0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### ancillary_to_attribute_effects_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf95110). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### ancillary_to_attribute_situation_effects_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf95160). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### ancillary_to_effects_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf951b0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### ancillary_to_excluded_ancillaries_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf95200). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### ancillary_to_included_agents_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf95250). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### ancillary_types_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xf952a0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### anim_reference_poses_tables

- Row reader **0xdd29f0** (callback 0xde26b0, table loader 0xdc5980, name getter 0xf952f0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: campaign_anim_action_to_sets_tables, campaign_map_towns_and_ports_tables, gun_type_to_projectiles_tables, start_pos_character_ancillaries_tables, start_pos_character_traits_tables, start_pos_naval_units_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2a1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2a24 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2a2d |

### battle_bridge_subculture_jcts_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xfb2270, name getter 0xfb8620). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### battle_cities_tables

- Row reader **0xfb55a0** (callback 0xfb5b20, table loader 0xfb4140, name getter 0xfb56e0). Fields read from the file: 9.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfb55cc |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xfb5638 |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xfb564c |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xfb5660 |
| 4 | int32 or float32 | 4 | 0x18 |  |  | 0xfb5674 |
| 5 | int32 or float32 | 4 | 0x1c |  |  | 0xfb5688 |
| 6 | int32 or float32 | 4 | 0x20 |  |  | 0xfb569c |
| 7 | int32 or float32 | 4 | 0x24 |  |  | 0xfb56b0 |
| 8 | int32 or float32 | 4 | 0x28 |  |  | 0xfb56c4 |

### battle_city_buildings_tables

- Row reader **0xfb70d0** (callback 0xfb7a20, table loader 0xfb5c40, name getter 0xfb7140). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfb70f1 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfb7102 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xfb7119 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xfb712d |

### battle_city_subculture_jct_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xfa27e0, name getter 0xfb7ba0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### battle_climate_groupings_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf95340). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### battle_climate_weather_descriptions_tables

- Row reader **0xe54260** (callback 0xe69200, table loader 0xe3e9c0, name getter 0xe55540). Fields read from the file: 11.
- The same reader (and therefore the same row layout) is used by: battle_type_setup_limits_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe5429b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe542ac |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe542bd |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xe542ce |
| 4 | int32 or float32 | 4 | 0x30 |  |  | 0xe542e9 |
| 5 | int32 or float32 | 4 | 0x34 |  |  | 0xe542fd |
| 6 | int32 or float32 | 4 | 0x38 |  |  | 0xe54311 |
| 7 | int32 or float32 | 4 | 0x3c |  |  | 0xe54325 |
| 8 | int32 or float32 | 4 | 0x40 |  |  | 0xe54339 |
| 9 | int32 or float32 | 4 | 0x44 |  |  | 0xe5434d |
| 10 | int32 or float32 | 4 | 0x48 |  |  | 0xe54361 |

### battle_groundcover_density_maps_tables

- Row reader **0xe544b0** (callback 0xe69220, table loader 0xdcb370, name getter 0xe555e0). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe544ed |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe544fe |
| 2 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xe5450f |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xe5451f |
| - | (no file bytes) | 0 | 0x24 |  | derived: string copy into (call:FUN_004f1200) | 0xe5452c |

### battle_groundcover_distribution_maps_tables

- Row reader **0xe54560** (callback 0xe69230, table loader 0xdc3590, name getter 0xe55630). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: mount_variants_tables, trees_climates_jct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe5458f |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe545a0 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xe545b7 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xe545c4 |

### battle_personalities_tables

- Row reader **0xe545f0** (callback 0xe69240, table loader 0xe40d60, name getter 0xe55680). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54633 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe54644 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe54655 |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xe54666 |
| 4 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xe5467e |

### battle_script_strings_tables

- Row reader **0xe548b0** (callback 0xe69260, table loader 0xdc5980, name getter 0xe55720). Fields read from the file: 3. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe548dc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe548ed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 | version != 0 |  | 0xe54907 |

### battle_sequences_tables

- Row reader **0xfaf1b0** (callback 0xfaf700, table loader 0xfa5d60, name getter 0xfbff60). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: mp_general_command_ratings_tables, special_edition_enums_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfaf1d9 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xfaf247 |

### battle_sky_types_tables

- Row reader **0xe54920** (callback 0xe69270, table loader 0xe43100, name getter 0xe55770). Fields read from the file: 8.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54979 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe5498a |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe5499b |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xe549ac |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x30 |  | flag==0 -> empty string | 0xe549c4 |
| 5 | string | u16 len + len*UTF-16LE | 0x3c |  |  | 0xe54a17 |
| 6 | bool | 1 | 0x48 |  |  | 0xe54a32 |
| 7 | bool | 1 | 0x49 |  |  | 0xe54a46 |

### battle_terrain_farm_walls_tables

- Row reader **0xfb1370** (callback 0xfb2150, table loader 0xfafe10, name getter 0xfb1560). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfb13ee |
| 1 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xfb142a |
| 2 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xfb149a |
| 3 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xfb14cf |
| 4 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xfb153f |

### battle_terrain_farms_tables

- Row reader **0xe54a60** (callback 0xe69280, table loader 0xe442d0, name getter 0xe557c0). Fields read from the file: 7.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54af4 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe54b08 |
| 2 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xe54b3a |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | ? |  | flag==0 -> empty string | 0xe54bae |
| 4 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xe54c01 |
| 5 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xe54c15 |
| 6 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xe54c29 |

### battle_terrain_set_climates_jcts_tables

- Row reader **0xe54c40** (callback 0xe69290, table loader 0xe454a0, name getter 0xe55810). Fields read from the file: 2.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54c6a |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe54c7d |
| - | (no file bytes) | 0 | 0xc |  | derived: string copy into (call:FUN_004f1200) | 0xe54c8d |

### battle_terrain_set_groupings_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xe55860). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### battle_terrain_sets_tables

- Row reader **0xe54cb0** (callback 0xe692a0, table loader 0xe46690, name getter 0xe558b0). Fields read from the file: 15.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54d4b |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe54d62 |
| 2 | string | u16 len + len*UTF-16LE | 0x10 |  |  | 0xe54d73 |
| 3 | string | u16 len + len*UTF-16LE | 0x1c |  |  | 0xe54d84 |
| 4 | string | u16 len + len*UTF-16LE | 0x28 |  |  | 0xe54d95 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x34 |  | flag==0 -> empty string | 0xe54dad |
| 6 | string | u16 len + len*UTF-16LE | 0x40 |  |  | 0xe54dff |
| 7 | string | u16 len + len*UTF-16LE | 0x4c |  |  | 0xe54e13 |
| 8 | string | u16 len + len*UTF-16LE | 0x58 |  |  | 0xe54e27 |
| 9 | string | u16 len + len*UTF-16LE | 0x64 |  |  | 0xe54e3b |
| 10 | string | u16 len + len*UTF-16LE | 0x70 |  |  | 0xe54e4f |
| 11 | string | u16 len + len*UTF-16LE | 0x7c |  |  | 0xe54e63 |
| 12 | string | u16 len + len*UTF-16LE | 0x88 |  |  | 0xe54e7a |
| 13 | string | u16 len + len*UTF-16LE | 0x94 |  |  | 0xe54e91 |
| 14 | string | u16 len + len*UTF-16LE | 0xa0 |  |  | 0xe54ea8 |
| - | (no file bytes) | 0 | 0x10 |  | derived: string copy into (call:FUN_004f1200) | 0xe54ed5 |

### battle_type_faction_presets_tables

- Row reader **0xe54f10** (callback 0xe692b0, table loader 0xe47880, name getter 0xe55900). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: commodities_tables, start_pos_royalty_names_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54f26 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe54f3d |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xe54f51 |

### battle_type_setup_limits_tables

- Row reader **0xe54260** (callback 0xe69200, table loader 0xe48c20, name getter 0xe559a0). Fields read from the file: 11.
- The same reader (and therefore the same row layout) is used by: battle_climate_weather_descriptions_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe5429b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe542ac |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe542bd |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xe542ce |
| 4 | int32 or float32 | 4 | 0x30 |  |  | 0xe542e9 |
| 5 | int32 or float32 | 4 | 0x34 |  |  | 0xe542fd |
| 6 | int32 or float32 | 4 | 0x38 |  |  | 0xe54311 |
| 7 | int32 or float32 | 4 | 0x3c |  |  | 0xe54325 |
| 8 | int32 or float32 | 4 | 0x40 |  |  | 0xe54339 |
| 9 | int32 or float32 | 4 | 0x44 |  |  | 0xe5434d |
| 10 | int32 or float32 | 4 | 0x48 |  |  | 0xe54361 |

### battle_type_unit_to_faction_presets_tables

- Row reader **0xe54f60** (callback 0xe692c0, table loader 0xe49e80, name getter 0xe559f0). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: character_trait_levels_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54f81 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe54f98 |
| 2 | string | u16 len + len*UTF-16LE | 0x10 |  |  | 0xe54fa6 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xe54fbd |

### battle_types_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xe55950). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### battle_weather_types_tables

- Row reader **0xe54fd0** (callback 0xe692d0, table loader 0xe4b050, name getter 0xe55a40). Fields read from the file: 8.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54fe6 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe54ffd |
| 2 | int32 or float32 | 4 | local |  |  | 0xe55012 |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xe5502d |
| 4 | int32 or float32 | 4 | 0x18 |  |  | 0xe55041 |
| 5 | int32 or float32 | 4 | 0x1c |  |  | 0xe55055 |
| 6 | int32 or float32 | 4 | 0x20 |  |  | 0xe55069 |
| 7 | bool | 1 | 0x24 |  |  | 0xe5507d |

### battlefield_building_categories_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xdc5980, name getter 0xe55400). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### battlefield_building_transformations_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xe554a0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### battlefield_buildings_tables

- Row reader **0xe54010** (callback 0xe691e0, table loader 0xe3c620, name getter 0xe55450). Fields read from the file: 8. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54065 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe54076 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe54087 |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xe54098 |
| 4 | int32 or float32 | 4 | 0x30 |  |  | 0xe540b3 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x34 |  | flag==0 -> empty string | 0xe540c8 |
| 6 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x4c |  | flag==0 -> empty string | 0xe5411e |
| 7 | int32 or float32 | 4 | 0x58 | version != 0 |  | 0xe5417c |

### battlefield_deployable_siege_items_tables

- Row reader **0xe541a0** (callback 0xe691f0, table loader 0xe3d7f0, name getter 0xe554f0). Fields read from the file: 6.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe541db |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe541f6 |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xe5420e |
| 3 | string | u16 len + len*UTF-16LE | 0x14 |  |  | 0xe5421c |
| 4 | string | u16 len + len*UTF-16LE | 0x20 |  |  | 0xe5422d |
| 5 | string | u16 len + len*UTF-16LE | 0x2c |  |  | 0xe5423e |

### battlefield_snow_props_tables

- Row reader **0xfa71c0** (callback 0xfa7c20, table loader 0xfa5d60, name getter 0xfa7270). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: cdir_unit_balance_groups_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfa71e9 |
| 1 | bool | 1 | 0xc |  |  | 0xfa7257 |

### battles_tables

- Row reader **0xe54690** (callback 0xe69250, table loader 0xe41f30, name getter 0xe556d0). Fields read from the file: 13. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe546d7 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe546e8 |
| 2 | bool | 1 | 0x18 |  |  | 0xe546ff |
| 3 | string | u16 len + len*UTF-16LE | 0x1c |  |  | 0xe5470d |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x28 |  | flag==0 -> empty string | 0xe54725 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x34 |  | flag==0 -> empty string | 0xe5477b |
| 6 | int32 or float32 | 4 | 0x40 |  |  | 0xe547d0 |
| 7 | int32 or float32 | 4 | 0x44 |  |  | 0xe547e4 |
| 8 | bool | 1 | 0x48 |  |  | 0xe547f8 |
| 9 | bool | 1 | 0x49 |  |  | 0xe5480c |
| 10 | bool | 1 | 0x4a |  |  | 0xe54820 |
| 11 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x4c | version != 0 | flag==0 -> empty string | 0xe5483e |
| 12 | int32 or float32 | 4 | 0x58 | version != 0 |  | 0xe54893 |

### battles_to_battle_sky_types_junctions_tables

- Row reader **0xfb3670** (callback 0xfb4020, table loader 0xfb2270, name getter 0xfd34c0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: small_vegetation_climates_jct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfb369c |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xfb36af |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xfb36bf |

### bribe_actions_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdcc540, name getter 0xe55a90). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### building_chain_to_slots_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xe55b30). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### building_chains_tables

- Row reader **0xe550d0** (callback 0xe692f0, table loader 0xe4c220, name getter 0xe55ae0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550ff |
| 1 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xc |  | flag==0 -> empty string | 0xe55117 |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | local |  | flag==0 -> empty string | 0xe55172 |
| - | (no file bytes) | 0 | 0x18 |  | derived: parse previous string as int into (call:FUN_004f3720) | 0xe551c2 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x1c |  | flag==0 -> empty string | 0xe551d7 |

### building_culture_gov_type_variants_tables

- Row reader **0xe55250** (callback 0xe69300, table loader 0xe4d3f0, name getter 0xe55b80). Fields read from the file: 7.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe552a7 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe552b8 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe552c9 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x30 |  | flag==0 -> empty string | 0xe552e1 |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x3c |  | flag==0 -> empty string | 0xe55337 |
| 5 | string | u16 len + len*UTF-16LE | 0x48 |  |  | 0xe55389 |
| 6 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x54 |  | flag==0 -> empty string | 0xe553a1 |

### building_culture_variants_tables

- Row reader **0xdd1ff0** (callback 0xde25f0, table loader 0xdbb7a0, name getter 0xdd32a0). Fields read from the file: 7.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd203f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2050 |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x18 |  | flag==0 -> empty string | 0xdd2068 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xdd20ba |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x30 |  | flag==0 -> empty string | 0xdd2110 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x3c |  | flag==0 -> empty string | 0xdd2166 |
| 6 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x48 |  | flag==0 -> empty string | 0xdd21bc |

### building_description_texts_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xdd32f0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### building_faction_variants_tables

- Row reader **0xdd2300** (callback 0xde2630, table loader 0xdbedb0, name getter 0xdd33e0). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd233f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2350 |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x18 |  | flag==0 -> empty string | 0xdd2368 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xdd23ba |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x30 |  | flag==0 -> empty string | 0xdd2410 |

### building_factionwide_effects_junctions_tables

- Row reader **0xdd22a0** (callback 0xde2620, table loader 0xdbdb90, name getter 0xdd3390). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: campaign_ai_manager_behaviour_junctions_tables, campaign_ai_personality_junctions_tables, campaigns_campaign_variables_junctions_tables, seasons_tables, start_pos_faction_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd22c1 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd22d2 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd22e9 |

### building_level_required_technology_junctions_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdc11a0, name getter 0xdd3480). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### building_research_thread_junction_tables

- Row reader **0xdd27f0** (callback 0xde2670, table loader 0xdc3590, name getter 0xdd3520). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: building_resources_junction_tables, commodity_slot_junction_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd281d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2826 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xdd2836 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2843 |

### building_resources_junction_tables

- Row reader **0xdd27f0** (callback 0xde2670, table loader 0xdc3590, name getter 0xdd3570). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: building_research_thread_junction_tables, commodity_slot_junction_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd281d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2826 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xdd2836 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2843 |

### building_units_allowed_tables

- Row reader **0xdd2870** (callback 0xde2680, table loader 0xdc4760, name getter 0xdd35c0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd289f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd28b0 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd28c7 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x1c |  | flag==0 -> empty string | 0xdd28dc |

### building_upgrades_junction_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xdd3610). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: empires_regions_junct_tables, events_effect_group_junct_tables, events_hist_chars_junct_tables, events_to_policies_junction_tables, events_view_group_junct_tables, groupings_continents_junct_tables, groupings_cultures_junct_tables, groupings_empires_junct_tables, groupings_factions_junct_tables, groupings_regions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### campaign_ai_manager_behaviour_junctions_tables

- Row reader **0xdd22a0** (callback 0xde2620, table loader 0xdc6b50, name getter 0xdd3660). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: building_factionwide_effects_junctions_tables, campaign_ai_personality_junctions_tables, campaigns_campaign_variables_junctions_tables, seasons_tables, start_pos_faction_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd22c1 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd22d2 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd22e9 |

### campaign_ai_managers_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xdd36b0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### campaign_ai_personalities_tables

- Row reader **0xdd29b0** (callback 0xde26a0, table loader 0xdc7db0, name getter 0xdd3750). Fields read from the file: 2.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd29c3 |
| 1 | bool | 1 | 0xc |  |  | 0xdd29dc |

### campaign_ai_personality_junctions_tables

- Row reader **0xdd22a0** (callback 0xde2620, table loader 0xdc6b50, name getter 0xdd3700). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: building_factionwide_effects_junctions_tables, campaign_ai_manager_behaviour_junctions_tables, campaigns_campaign_variables_junctions_tables, seasons_tables, start_pos_faction_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd22c1 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd22d2 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd22e9 |

### campaign_anim_action_to_sets_tables

- Row reader **0xdd29f0** (callback 0xde26b0, table loader 0xdc8f80, name getter 0xdd37a0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: anim_reference_poses_tables, campaign_map_towns_and_ports_tables, gun_type_to_projectiles_tables, start_pos_character_ancillaries_tables, start_pos_character_traits_tables, start_pos_naval_units_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2a1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2a24 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2a2d |

### campaign_anim_sets_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xdd37f0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### campaign_character_anim_set_agent_junctions_tables

- Row reader **0xdd2bd0** (callback 0xde26d0, table loader 0xdcb370, name getter 0xdd3890). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2c07 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2c18 |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x18 |  | flag==0 -> empty string | 0xdd2c30 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xdd2c82 |

### campaign_character_anim_sets_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xdd38e0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### campaign_character_anim_walk_anim_junctions_tables

- Row reader **0xdd2ce0** (callback 0xde26e0, table loader 0xdcb370, name getter 0xdd3930). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: effect_bonus_value_population_class_and_religion_junction_tables, movie_event_strings_tables, uniforms_tables, units_to_gov_type_permissions_tables, unrest_cause_to_demands_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2d1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2d2c |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2d3d |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xdd2d4e |

### campaign_character_anims_junctions_tables

- Row reader **0xdd2a40** (callback 0xde26c0, table loader 0xdca1a0, name getter 0xdd3840). Fields read from the file: 8.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2aac |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2ac1 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2ad2 |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xdd2ae3 |
| 4 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xdd2af4 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x3c |  | flag==0 -> empty string | 0xdd2b0c |
| 6 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x48 |  | flag==0 -> empty string | 0xdd2b63 |
| 7 | int32 or float32 | 4 | 0x54 |  |  | 0xdd2bbd |

### campaign_difficulty_handicap_effects_tables

- Row reader **0xf9f1a0** (callback 0xf9fc40, table loader 0xf9dcd0, name getter 0xf9f430). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | int32 or float32 | 4 | 0xc |  |  | 0xf9f1cb |
| 1 | bool | 1 | 0x10 |  |  | 0xf9f1df |
| 2 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xf9f1fe |
| 3 | int32 or float32 | 4 | 0x20 |  |  | 0xf9f26c |
| - | (no file bytes) | 0 | 0x14 |  | derived: string copy into (call:FUN_004f0660) | 0xf9f297 |

### campaign_ground_types_tables

- Row reader **0xdd2d70** (callback 0xde26f0, table loader 0xdcd710, name getter 0xdd3980). Fields read from the file: 5. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2d88 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xdd2d9f |
| 2 | bool | 1 | 0x10 |  |  | 0xdd2db3 |
| 3 | bool | 1 | 0x11 | version != 0 |  | 0xdd2dd3 |
| 4 | bool | 1 | 0x12 | version != 0 |  | 0xdd2de4 |

### campaign_map_famous_battles_tables

- Row reader **0xdd2e10** (callback 0xde2700, table loader 0xdce8e0, name getter 0xdd39d0). Fields read from the file: 6.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2e4b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2e5c |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2e6d |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xdd2e7e |
| 4 | int32 or float32 | 4 | 0x30 |  |  | 0xdd2e99 |
| 5 | int32 or float32 | 4 | 0x34 |  |  | 0xdd2ead |

### campaign_map_playable_areas_tables

- Row reader **0xdd2ec0** (callback 0xde2710, table loader 0xdcfab0, name getter 0xdd3a20). Fields read from the file: 9.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2eff |
| 1 | bool | 1 | 0xc |  |  | 0xdd2f16 |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x10 |  | flag==0 -> empty string | 0xdd2f2b |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x1c |  | flag==0 -> empty string | 0xdd2f7d |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x28 |  | flag==0 -> empty string | 0xdd2fcf |
| 5 | string | u16 len + len*UTF-16LE | 0x34 |  |  | 0xdd3021 |
| 6 | int32 or float32 | 4 | 0x40 |  |  | 0xdd3038 |
| 7 | int32 or float32 | 4 | 0x44 |  |  | 0xdd304c |
| 8 | int32 or float32 | 4 | 0x48 |  |  | 0xdd3060 |

### campaign_map_settlements_tables

- Row reader **0xdd3080** (callback 0xde2720, table loader 0xdd0c80, name getter 0xdd3a70). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd30bb |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd30cc |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd30dd |
| 3 | int32 or float32 | 4 | 0x30 |  |  | 0xdd30f8 |
| 4 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xdd3106 |

### campaign_map_slots_tables

- Row reader **0xdd3120** (callback 0xde2730, table loader 0xdd0c80, name getter 0xdd3b10). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd3157 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd316c |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd317d |
| 3 | int32 or float32 | 4 | 0x30 |  |  | 0xdd3194 |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xdd31a9 |

### campaign_map_slots_templates_rotations_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xdd3ac0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### campaign_map_tooltips_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdc11a0, name getter 0xdd3b60). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### campaign_map_towns_and_ports_tables

- Row reader **0xdd29f0** (callback 0xde26b0, table loader 0xdc5980, name getter 0xdd3bb0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: anim_reference_poses_tables, campaign_anim_action_to_sets_tables, gun_type_to_projectiles_tables, start_pos_character_ancillaries_tables, start_pos_character_traits_tables, start_pos_naval_units_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2a1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2a24 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2a2d |

### campaign_walk_anim_sets_tables

- Row reader **0xf702a0** (callback 0xf88220, table loader 0xf65ee0, name getter 0xf71740). Fields read from the file: 15.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf70340 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf70355 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf70366 |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf70377 |
| 4 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xf70388 |
| 5 | string | u16 len + len*UTF-16LE | 0x3c |  |  | 0xf703a0 |
| 6 | string | u16 len + len*UTF-16LE | 0x48 |  |  | 0xf703b4 |
| 7 | int32 or float32 | 4 | 0x54 |  |  | 0xf703cb |
| 8 | string | u16 len + len*UTF-16LE | 0x58 |  |  | 0xf703dc |
| 9 | int32 or float32 | 4 | 0x64 |  |  | 0xf703f3 |
| 10 | string | u16 len + len*UTF-16LE | 0x68 |  |  | 0xf70404 |
| 11 | string | u16 len + len*UTF-16LE | 0x74 |  |  | 0xf70418 |
| 12 | int32 or float32 | 4 | 0x80 |  |  | 0xf70432 |
| 13 | string | u16 len + len*UTF-16LE | 0x84 |  |  | 0xf70446 |
| 14 | string | u16 len + len*UTF-16LE | 0x90 |  |  | 0xf7045d |

### campaigns_campaign_variables_junctions_tables

- Row reader **0xdd22a0** (callback 0xde2620, table loader 0xfbc730, name getter 0xfbdba0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: building_factionwide_effects_junctions_tables, campaign_ai_manager_behaviour_junctions_tables, campaign_ai_personality_junctions_tables, seasons_tables, start_pos_faction_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd22c1 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd22d2 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd22e9 |

### campaigns_tables

- Row reader **0xf70140** (callback 0xf88210, table loader 0xf04420, name getter 0xf716a0). Fields read from the file: 6. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf7016f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf70180 |
| 2 | bool | 1 | 0x24 |  |  | 0xf70197 |
| 3 | int32 or float32 | 4 | 0x28 |  |  | 0xf701ab |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | local |  | flag==0 -> empty string | 0xf701c9 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x18 | version != 0 | flag==0 -> empty string | 0xf7022a |

### cdir_campaign_junctions_tables

- Row reader **0xf088f0** (callback 0xf219f0, table loader 0xfa27e0, name getter 0xfd0180). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: cdir_faction_junctions_tables, trait_categories_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf08914 |
| 1 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xc |  | flag==0 -> empty string | 0xf0892c |

### cdir_configs_tables

- Row reader **0xfcd150** (callback 0xfcd7a0, table loader 0xfcbd50, name getter 0xfcd300). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfcd18f |
| 1 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xc |  | flag==0 -> empty string | 0xfcd1a7 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xfcd1f5 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xfcd20d |
| - | (no file bytes) | 0 | 0xc |  | derived: string copy into (call:FUN_004f1200) | 0xfcd276 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xfcd2a7 |

### cdir_desire_priorities_tables

- Row reader **0xfcf410** (callback 0xfcf970, table loader 0xfce010, name getter 0xfcf4d0). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfcf43d |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfcf44e |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xfcf465 |
| - | (no file bytes) | 0 | 0xc |  | derived: string copy into (call:FUN_004f1200) | 0xfcf491 |

### cdir_faction_junctions_tables

- Row reader **0xf088f0** (callback 0xf219f0, table loader 0xfa27e0, name getter 0xfcfa90). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: cdir_campaign_junctions_tables, trait_categories_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf08914 |
| 1 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xc |  | flag==0 -> empty string | 0xf0892c |

### cdir_unit_balance_group_qualities_tables

- Row reader **0xfc9a60** (callback 0xfca030, table loader 0xfc8660, name getter 0xfc9b90). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfc9a9d |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfc9aae |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xfc9abf |
| 3 | int32 or float32 | 4 | 0x24 |  |  | 0xfc9ada |
| - | (no file bytes) | 0 | 0xc |  | derived: string copy into (call:FUN_004f1200) | 0xfc9b06 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xfc9b34 |

### cdir_unit_balance_groups_tables

- Row reader **0xfa71c0** (callback 0xfa7c20, table loader 0xfa5d60, name getter 0xfc80b0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: battlefield_snow_props_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfa71e9 |
| 1 | bool | 1 | 0xc |  |  | 0xfa7257 |

### cdir_unit_balances_tables

- Row reader **0xfcb550** (callback 0xfcbc30, table loader 0xfca150, name getter 0xfcb740). Fields read from the file: 7. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfcb583 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xfcb59a |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xfcb5ae |
| 3 | string | u16 len + len*UTF-16LE | 0x14 |  |  | 0xfcb5bc |
| 4 | int32 or float32 | 4 | 0x20 |  |  | 0xfcb5d3 |
| 5 | int32 or float32 | 4 | 0x24 |  |  | 0xfcb5e7 |
| 6 | int32 or float32 | 4 | 0x28 | version != 0 |  | 0xfcb604 |
| - | (no file bytes) | 0 | 0x14 |  | derived: string copy into (call:FUN_004f1200) | 0xfcb6b6 |

### cdir_unit_qualities_tables

- Row reader **0xfc7970** (callback 0xfc7f90, table loader 0xfc6570, name getter 0xfc7aa0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfc79ad |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xfc79be |
| 2 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xfc79cf |
| 3 | int32 or float32 | 4 | 0x30 |  |  | 0xfc79ea |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xfc7a16 |
| - | (no file bytes) | 0 | 0x24 |  | derived: string copy into (call:FUN_004f1200) | 0xfc7a44 |

### character_trait_levels_tables

- Row reader **0xe54f60** (callback 0xe692c0, table loader 0xe49e80, name getter 0xf71790). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: battle_type_unit_to_faction_presets_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54f81 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe54f98 |
| 2 | string | u16 len + len*UTF-16LE | 0x10 |  |  | 0xe54fa6 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xe54fbd |

### character_traits_tables

- Row reader **0xf70470** (callback 0xf88230, table loader 0xf670b0, name getter 0xf717e0). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf7048e |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf7049e |
| 2 | bool | 1 | 0x10 |  |  | 0xf704ab |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xf704b9 |
| 4 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf704bf |

### climate_to_tilesets_tables

- Row reader **0xfd1b30** (callback 0xfd2070, table loader 0xfd0730, name getter 0xfd2740). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: faction_rebellion_units_junctions_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfd1b5a |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfd1b6d |
| - | (no file bytes) | 0 | 0xc |  | derived: string copy into (call:FUN_004f1200) | 0xfd1b9c |

### climates_tables

- Row reader **0xf704d0** (callback 0xf88240, table loader 0xe766d0, name getter 0xf71830). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf704e6 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf704fd |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xf70511 |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xf70525 |
| 4 | bool | 1 | 0x18 |  |  | 0xf70539 |

### commodities_demand_drivers_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf71880). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### commodities_demand_junction_tables

- Row reader **0xf70550** (callback 0xf88250, table loader 0xf04420, name getter 0xf718d0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf7057d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf70586 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xf70596 |
| 3 | int32 or float32 | 4 | 0x28 |  |  | 0xf705a3 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xf705b0 |

### commodities_tables

- Row reader **0xe54f10** (callback 0xe692b0, table loader 0xdcd710, name getter 0xf71920). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_type_faction_presets_tables, start_pos_royalty_names_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54f26 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe54f3d |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xe54f51 |

### commodity_slot_junction_tables

- Row reader **0xdd27f0** (callback 0xde2670, table loader 0xdc3590, name getter 0xf71970). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: building_research_thread_junction_tables, building_resources_junction_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd281d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2826 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xdd2836 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2843 |

### commodity_unit_names_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf719c0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### cultures_subcultures_tables

- Row reader **0xf083a0** (callback 0xf219a0, table loader 0xf00eb0, name getter 0xf093e0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf083ce |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf083d7 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xf083e7 |
| 3 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xf083ff |

### cultures_tables

- Row reader **0xf705e0** (callback 0xf88260, table loader 0xf360f0, name getter 0xf71a10). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf70604 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf7061b |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x10 |  | flag==0 -> empty string | 0xf70630 |

### cursors_tables

- Row reader **0xf70690** (callback 0xf88270, table loader 0xf68280, name getter 0xf71a60). Fields read from the file: 10.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf706b1 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf706c2 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xf706d9 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xf706ed |
| 4 | int32 or float32 | 4 | 0x20 |  |  | 0xf70701 |
| 5 | int32 or float32 | 4 | 0x24 |  |  | 0xf70715 |
| 6 | bool | 1 | 0x28 |  |  | 0xf70729 |
| 7 | bool | 1 | 0x29 |  |  | 0xf7073d |
| 8 | int32 or float32 | 4 | 0x2c |  |  | 0xf70751 |
| 9 | int32 or float32 | 4 | 0x30 |  |  | 0xf70765 |

### diplomacy_attitudes_tables

- Row reader **0xfbc050** (callback 0xfbc610, table loader 0xfbac50, name getter 0xfbc130). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfbc07c |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xfbc0e8 |
| 2 | int32 or float32 | 4 | 0x14 |  |  | 0xfbc0fc |
| 3 | int32 or float32 | 4 | 0x10 |  |  | 0xfbc110 |

### diplomacy_factor_strings_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf71ab0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### diplomacy_negotiation_faction_override_strings_tables

- Row reader **0xf70780** (callback 0xf88280, table loader 0xdcc540, name getter 0xf71b00). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xf707d3 |
| 1 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xf707e8 |
| 2 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xf707fd |
| 3 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xf70812 |
| 4 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf70927 |

### diplomacy_negotiation_strings_tables

- Row reader **0xf70970** (callback 0xf88290, table loader 0xdcc540, name getter 0xf71b50). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xf709b7 |
| 1 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xf709cc |
| 2 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xf709e1 |
| 3 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf70a94 |

### diplomacy_strings_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf71ba0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### diplomatic_relations_attitudes_tables

- Row reader **0xf08360** (callback 0xf21990, table loader 0xdc7db0, name getter 0xf71bf0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: campaign_variables_tables, entity_training_levels_tables, state_gift_values_tables, taxes_levels_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf08373 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf0838c |

### diplomatic_relations_government_type_tables

- Row reader **0xf3fa20** (callback 0xf58fb0, table loader 0xf04420, name getter 0xf71c40). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: diplomatic_relations_religion_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3fa4f |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3fa60 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xf3fa77 |
| 3 | int32 or float32 | 4 | 0x28 |  |  | 0xf3fa8b |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xf3fab7 |

### diplomatic_relations_religion_tables

- Row reader **0xf3fa20** (callback 0xf58fb0, table loader 0xf04420, name getter 0xf40d90). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: diplomatic_relations_government_type_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3fa4f |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3fa60 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xf3fa77 |
| 3 | int32 or float32 | 4 | 0x28 |  |  | 0xf3fa8b |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xf3fab7 |

### disaster_to_ground_types_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf71ce0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### effect_bonus_value_agent_junction_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf71d30). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effect_bonus_value_basic_junction_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf71d80). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### effect_bonus_value_building_chain_junctions_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf71dd0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effect_bonus_value_commodity_junction_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf71e20). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effect_bonus_value_population_class_and_religion_junction_tables

- Row reader **0xdd2ce0** (callback 0xde26e0, table loader 0xf6a690, name getter 0xf71e70). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: campaign_character_anim_walk_anim_junctions_tables, movie_event_strings_tables, uniforms_tables, units_to_gov_type_permissions_tables, unrest_cause_to_demands_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2d1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2d2c |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2d3d |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xdd2d4e |

### effect_bonus_value_population_class_junction_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf71ec0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effect_bonus_value_projectile_junctions_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf71f10). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effect_bonus_value_religion_junction_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf71f60). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effect_bonus_value_resource_junction_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf71fb0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effect_bonus_value_shot_type_junctions_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf72000). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effect_bonus_value_unit_ability_junctions_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf72050). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effect_bonus_value_unit_category_junction_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf720a0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effect_bonus_value_unit_class_junction_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xf69450, name getter 0xf720f0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### effects_tables

- Row reader **0xf70ad0** (callback 0xf882a0, table loader 0xf2d200, name getter 0xf72140). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf70af4 |
| 1 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xc |  | flag==0 -> empty string | 0xf70b0c |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xf70b5d |

### empires_regions_junct_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf72190). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, events_effect_group_junct_tables, events_hist_chars_junct_tables, events_to_policies_junction_tables, events_view_group_junct_tables, groupings_continents_junct_tables, groupings_cultures_junct_tables, groupings_empires_junct_tables, groupings_factions_junct_tables, groupings_regions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### empires_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf721e0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### entity_training_levels_tables

- Row reader **0xf08360** (callback 0xf21990, table loader 0xdc7db0, name getter 0xf72230). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: campaign_variables_tables, diplomatic_relations_attitudes_tables, state_gift_values_tables, taxes_levels_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf08373 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf0838c |

### events_effect_group_junct_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf72280). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, empires_regions_junct_tables, events_hist_chars_junct_tables, events_to_policies_junction_tables, events_view_group_junct_tables, groupings_continents_junct_tables, groupings_cultures_junct_tables, groupings_empires_junct_tables, groupings_factions_junct_tables, groupings_regions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### events_hist_chars_junct_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf722d0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, empires_regions_junct_tables, events_effect_group_junct_tables, events_to_policies_junction_tables, events_view_group_junct_tables, groupings_continents_junct_tables, groupings_cultures_junct_tables, groupings_empires_junct_tables, groupings_factions_junct_tables, groupings_regions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### events_tables

- Row reader **0xf70b70** (callback 0xf882b0, table loader 0xf6b8f0, name getter 0xf723c0). Fields read from the file: 8. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf70baf |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf70bc0 |
| 2 | bool | 1 | 0x18 |  |  | 0xf70bd7 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xf70beb |
| 4 | string | u16 len + len*UTF-16LE | 0x20 |  |  | 0xf70bf9 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x2c |  | flag==0 -> empty string | 0xf70c11 |
| 6 | string | u16 len + len*UTF-16LE | 0x38 |  |  | 0xf70c63 |
| 7 | int32 or float32 | 4 | 0x44 | version != 0 |  | 0xf70c83 |

### events_to_policies_junction_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf72320). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, empires_regions_junct_tables, events_effect_group_junct_tables, events_hist_chars_junct_tables, events_view_group_junct_tables, groupings_continents_junct_tables, groupings_cultures_junct_tables, groupings_empires_junct_tables, groupings_factions_junct_tables, groupings_regions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### events_view_group_junct_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf72370). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, empires_regions_junct_tables, events_effect_group_junct_tables, events_hist_chars_junct_tables, events_to_policies_junction_tables, groupings_continents_junct_tables, groupings_cultures_junct_tables, groupings_empires_junct_tables, groupings_factions_junct_tables, groupings_regions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### faction_rebellion_units_junctions_tables

- Row reader **0xfd1b30** (callback 0xfd2070, table loader 0xfd0730, name getter 0xfd1b30). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: climate_to_tilesets_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfd1b5a |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfd1b6d |
| - | (no file bytes) | 0 | 0xc |  | derived: string copy into (call:FUN_004f1200) | 0xfd1b9c |

### faction_uniform_colours_tables

- Row reader **0xfc50e0** (callback 0xfc5800, table loader 0xfc3ce0, name getter 0xfc5230). Fields read from the file: 10.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfc510c |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xfc5178 |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xfc518c |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xfc51a0 |
| 4 | int32 or float32 | 4 | 0x18 |  |  | 0xfc51b4 |
| 5 | int32 or float32 | 4 | 0x1c |  |  | 0xfc51c8 |
| 6 | int32 or float32 | 4 | 0x20 |  |  | 0xfc51dc |
| 7 | int32 or float32 | 4 | 0x24 |  |  | 0xfc51f0 |
| 8 | int32 or float32 | 4 | 0x28 |  |  | 0xfc5204 |
| 9 | int32 or float32 | 4 | 0x2c |  |  | 0xfc5218 |

### famous_battle_pools_tables

- Row reader **0xf71480** (callback 0xf882d0, table loader 0xf670b0, name getter 0xf72460). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf7149e |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf714ae |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xf714bb |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xf714c9 |
| 4 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf714cf |

### fort_underlay_climate_jcts_tables

- Row reader **0xfacbe0** (callback 0xfad6b0, table loader 0xfab7e0, name getter 0xfaccd0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfacc1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfacc2c |
| 2 | bool | 1 | 0x18 |  |  | 0xfacc43 |
| 3 | string | u16 len + len*UTF-16LE | 0x1c |  |  | 0xfacc51 |
| - | (no file bytes) | 0 | 0xc |  | derived: string copy into (call:FUN_004f1200) | 0xfacc65 |

### government_types_tables

- Row reader **0xf71600** (callback 0xf882f0, table loader 0xf6eef0, name getter 0xf72550). Fields read from the file: 6.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf7162c |
| 1 | bool | 1 | 0xc |  |  | 0xf71643 |
| 2 | bool | 1 | 0xd |  |  | 0xf71657 |
| 3 | int32 or float32 | 4 | 0x10 |  |  | 0xf7166b |
| 4 | string | u16 len + len*UTF-16LE | 0x14 |  |  | 0xf71679 |
| 5 | string | u16 len + len*UTF-16LE | 0x20 |  |  | 0xf7168a |

### government_types_to_effects_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xf6dc90, name getter 0xf72500). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### governorships_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf725a0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### groupings_continents_junct_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf725f0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, empires_regions_junct_tables, events_effect_group_junct_tables, events_hist_chars_junct_tables, events_to_policies_junction_tables, events_view_group_junct_tables, groupings_cultures_junct_tables, groupings_empires_junct_tables, groupings_factions_junct_tables, groupings_regions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### groupings_cultures_junct_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf72640). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, empires_regions_junct_tables, events_effect_group_junct_tables, events_hist_chars_junct_tables, events_to_policies_junction_tables, events_view_group_junct_tables, groupings_continents_junct_tables, groupings_empires_junct_tables, groupings_factions_junct_tables, groupings_regions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### groupings_empires_junct_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf72690). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, empires_regions_junct_tables, events_effect_group_junct_tables, events_hist_chars_junct_tables, events_to_policies_junction_tables, events_view_group_junct_tables, groupings_continents_junct_tables, groupings_cultures_junct_tables, groupings_factions_junct_tables, groupings_regions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### groupings_factions_junct_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf726e0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, empires_regions_junct_tables, events_effect_group_junct_tables, events_hist_chars_junct_tables, events_to_policies_junction_tables, events_view_group_junct_tables, groupings_continents_junct_tables, groupings_cultures_junct_tables, groupings_empires_junct_tables, groupings_regions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### groupings_military_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf3ffd0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### groupings_regions_junct_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf40020). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, empires_regions_junct_tables, events_effect_group_junct_tables, events_hist_chars_junct_tables, events_to_policies_junction_tables, events_view_group_junct_tables, groupings_continents_junct_tables, groupings_cultures_junct_tables, groupings_empires_junct_tables, groupings_factions_junct_tables, groupings_subcultures_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### groupings_subcultures_junct_tables

- Row reader **0xdd2940** (callback 0xde2690, table loader 0xdc5980, name getter 0xf40070). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: building_upgrades_junction_tables, empires_regions_junct_tables, events_effect_group_junct_tables, events_hist_chars_junct_tables, events_to_policies_junction_tables, events_view_group_junct_tables, groupings_continents_junct_tables, groupings_cultures_junct_tables, groupings_empires_junct_tables, groupings_factions_junct_tables, groupings_regions_junct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd296d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2976 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xdd2986 |

### groupings_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf400c0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### historical_character_traits_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xfad7d0, name getter 0xfaebf0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### historical_characters_tables

- Row reader **0xf3b060** (callback 0xf58ea0, table loader 0xf263a0, name getter 0xf401b0). Fields read from the file: 8.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3b0a6 |
| 1 | bool | 1 | 0xc |  |  | 0xf3b0ba |
| 2 | string | u16 len + len*UTF-16LE | 0x10 |  |  | 0xf3b0c3 |
| 3 | string | u16 len + len*UTF-16LE | 0x1c |  |  | 0xf3b0cc |
| 4 | string | u16 len + len*UTF-16LE | 0x28 |  |  | 0xf3b0d5 |
| 5 | int32 or float32 | 4 | 0x34 |  |  | 0xf3b0e9 |
| 6 | int32 or float32 | 4 | 0x38 |  |  | 0xf3b0f7 |
| 7 | string | u16 len + len*UTF-16LE | 0x3c |  |  | 0xf3b0fd |

### loading_screens_tables

- Row reader **0xdd3210** (callback 0xde2740, table loader 0xdcb370, name getter 0xdd3c00). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd324b |
| 1 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xdd325c |
| 2 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd326d |
| 3 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd327e |

### message_event_strings_tables

- Row reader **0xf3b110** (callback 0xf58eb0, table loader 0xf27570, name getter 0xf40200). Fields read from the file: 9.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3b16a |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3b17f |
| 2 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf3b190 |
| 3 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3b1a1 |
| 4 | string | u16 len + len*UTF-16LE | 0x3c |  |  | 0xf3b1b2 |
| 5 | bool | 1 | 0x54 |  |  | 0xf3b1cd |
| 6 | bool | 1 | 0x55 |  |  | 0xf3b1e1 |
| 7 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xf3b1f2 |
| 8 | string | u16 len + len*UTF-16LE | 0x48 |  |  | 0xf3b206 |

### ministerial_effectiveness_modifiers_tables

- Row reader **0xf3b220** (callback 0xf58ec0, table loader 0xf28790, name getter 0xf40250). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | int32 or float32 | 4 | 0x0 |  |  | 0xf3b23f |
| 1 | string | u16 len + len*UTF-16LE | 0x4 |  |  | 0xf3b24d |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xf3b264 |

### ministerial_position_default_names_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf40390). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### ministerial_positions_by_gov_types_tables

- Row reader **0xf3b2e0** (callback 0xf58ee0, table loader 0xf2add0, name getter 0xf40340). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3b32b |
| 1 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3b334 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3b33d |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf3b346 |
| 4 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xf3b34f |

### ministerial_positions_tables

- Row reader **0xdf0990** (callback 0xe29820, table loader 0xdc7db0, name getter 0xf403e0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_spawnings_tables, unit_experience_thresholds_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdf09a3 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xdf09b3 |

### ministerial_positions_to_effects_tables

- Row reader **0xf3b280** (callback 0xf58ed0, table loader 0xf29a90, name getter 0xf402a0). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3b29e |
| 1 | int32 or float32 | 4 | 0x1c |  |  | 0xf3b2ae |
| 2 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3b2b5 |
| 3 | int32 or float32 | 4 | 0x18 |  |  | 0xf3b2c5 |
| 4 | int32 or float32 | 4 | 0x20 |  |  | 0xf3b2d2 |

### ministerial_positions_to_governorships_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf402f0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### mission_activities_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xf40430). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### mission_effects_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xf40480). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### mission_sources_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xf2d200, name getter 0xf40520). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### missions_tables

- Row reader **0xf3b370** (callback 0xf58ef0, table loader 0xf2c030, name getter 0xf404d0). Fields read from the file: 12.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3b3e8 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3b3fd |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3b40e |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf3b41f |
| 4 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xf3b430 |
| 5 | string | u16 len + len*UTF-16LE | 0x3c |  |  | 0xf3b448 |
| 6 | u8 (not stored) | 1 | - |  | read into a local | 0xf3b460 |
| 7 | int32 or float32 | 4 | 0x48 |  |  | 0xf3b47b |
| 8 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x4c |  | flag==0 -> empty string | 0xf3b498 |
| 9 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x58 |  | flag==0 -> empty string | 0xf3b4ee |
| 10 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x64 |  | flag==0 -> empty string | 0xf3b544 |
| 11 | int32 or float32 | 4 | 0x70 |  |  | 0xf3b599 |

### models_building_tables

- Row reader **0xdd2660** (callback 0xde2660, table loader 0xdc23c0, name getter 0xdd34d0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd26cd |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd26d6 |
| 2 | int32 or float32 | 4 | local |  |  | 0xdd26ee |
| 3 | int32 or float32 | 4 | 0x24 |  |  | 0xdd2706 |

### models_naval_tables

- Row reader **0xf3b6f0** (callback 0xf58f20, table loader 0xf307e0, name getter 0xf407a0). Fields read from the file: 88.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3b8c9 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3b8db |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3b8fe |
| 3 | int32 or float32 | 4 | local |  |  | 0xf3b925 |
| 4 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xf3ba36 |
| 5 | int32 or float32 | 4 | local |  |  | 0xf3be08 |
| 6 | int32 or float32 | 4 | local |  |  | 0xf3bf32 |
| 7 | int32 or float32 | 4 | local |  |  | 0xf3bf43 |
| 8 | int32 or float32 | 4 | local |  |  | 0xf3bf54 |
| 9 | int32 or float32 | 4 | local |  |  | 0xf3bf62 |
| 10 | int32 or float32 | 4 | local |  |  | 0xf3bf70 |
| 11 | int32 or float32 | 4 | local |  |  | 0xf3bf7e |
| 12 | int32 or float32 | 4 | local |  |  | 0xf3bf8c |
| 13 | int32 or float32 | 4 | local |  |  | 0xf3bf9a |
| 14 | int32 or float32 | 4 | local |  |  | 0xf3bfa8 |
| 15 | int32 or float32 | 4 | local |  |  | 0xf3bfb6 |
| 16 | int32 or float32 | 4 | local |  |  | 0xf3bfc4 |
| 17 | int32 or float32 | 4 | local |  |  | 0xf3c021 |
| 18 | int32 or float32 | 4 | local |  |  | 0xf3c200 |
| 19 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xf3c316 |
| 20 | int32 or float32 | 4 | local |  |  | 0xf3c381 |
| 21 | int32 or float32 | 4 | local |  |  | 0xf3c392 |
| 22 | int32 or float32 | 4 | local |  |  | 0xf3c3f6 |
| 23 | int32 or float32 | 4 | local |  |  | 0xf3c407 |
| 24 | int32 or float32 | 4 | local |  |  | 0xf3c471 |
| 25 | int32 or float32 | 4 | local |  |  | 0xf3c47f |
| 26 | int32 or float32 | 4 | local |  |  | 0xf3c48d |
| 27 | u8 (not stored) | 1 | - |  | read into a local | 0xf3c49e |
| 28 | int32 or float32 | 4 | local |  |  | 0xf3c643 |
| 29 | int32 or float32 | 4 | local |  |  | 0xf3c654 |
| 30 | int32 or float32 | 4 | local |  |  | 0xf3c662 |
| 31 | int32 or float32 | 4 | local |  |  | 0xf3c670 |
| 32 | int32 or float32 | 4 | local |  |  | 0xf3ca03 |
| 33 | int32 or float32 | 4 | local |  |  | 0xf3cb11 |
| 34 | int32 or float32 | 4 | local |  |  | 0xf3cb4f |
| 35 | int32 or float32 | 4 | local |  |  | 0xf3cb5d |
| 36 | int32 or float32 | 4 | local |  |  | 0xf3cbba |
| 37 | int32 or float32 | 4 | local |  |  | 0xf3cbd9 |
| 38 | int32 or float32 | 4 | local |  |  | 0xf3cbee |
| 39 | int32 or float32 | 4 | local |  |  | 0xf3cbfc |
| 40 | int32 or float32 | 4 | local |  |  | 0xf3cc0a |
| 41 | int32 or float32 | 4 | local |  |  | 0xf3cdd6 |
| 42 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xf3cf16 |
| 43 | int32 or float32 | 4 | local |  |  | 0xf3cf79 |
| 44 | int32 or float32 | 4 | local |  |  | 0xf3cf8a |
| 45 | int32 or float32 | 4 | local |  |  | 0xf3cf9b |
| 46 | int32 or float32 | 4 | local |  |  | 0xf3cfac |
| 47 | int32 or float32 | 4 | local |  |  | 0xf3cfba |
| 48 | int32 or float32 | 4 | local |  |  | 0xf3cfc8 |
| 49 | int32 or float32 | 4 | local |  |  | 0xf3cfd6 |
| 50 | int32 or float32 | 4 | local |  |  | 0xf3cfe4 |
| 51 | int32 or float32 | 4 | local |  |  | 0xf3d0bc |
| 52 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xf3d1e9 |
| 53 | int32 or float32 | 4 | local |  |  | 0xf3d262 |
| 54 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xf3d2c3 |
| 55 | int32 or float32 | 4 | local |  |  | 0xf3d5cc |
| 56 | int32 or float32 | 4 | local |  |  | 0xf3d629 |
| 57 | int32 or float32 | 4 | local |  |  | 0xf3d691 |
| 58 | int32 or float32 | 4 | local |  |  | 0xf3d69f |
| 59 | int32 or float32 | 4 | local |  |  | 0xf3d6ad |
| 60 | int32 or float32 | 4 | local |  |  | 0xf3db7f |
| 61 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xf3dc44 |
| 62 | int32 or float32 | 4 | local |  |  | 0xf3dcb2 |
| 63 | int32 or float32 | 4 | local |  |  | 0xf3dcc8 |
| 64 | int32 or float32 | 4 | local |  |  | 0xf3dcd6 |
| 65 | int32 or float32 | 4 | local |  |  | 0xf3dce4 |
| 66 | int32 or float32 | 4 | local |  |  | 0xf3dcf5 |
| 67 | int32 or float32 | 4 | local |  |  | 0xf3dd06 |
| 68 | int32 or float32 | 4 | local |  |  | 0xf3dd14 |
| 69 | int32 or float32 | 4 | local |  |  | 0xf3dd25 |
| 70 | int32 or float32 | 4 | local |  |  | 0xf3dd33 |
| 71 | int32 or float32 | 4 | local |  |  | 0xf3dd44 |
| 72 | int32 or float32 | 4 | local |  |  | 0xf3de70 |
| 73 | int32 or float32 | 4 | local |  |  | 0xf3dfc7 |
| 74 | string | u16 len + len*UTF-16LE | ? |  | destination offset inferred | 0xf3dfec |
| 75 | int32 or float32 | 4 | local |  |  | 0xf3e0bb |
| 76 | int32 or float32 | 4 | local |  |  | 0xf3e114 |
| 77 | int32 or float32 | 4 | local |  |  | 0xf3e171 |
| 78 | int32 or float32 | 4 | local |  |  | 0xf3e17f |
| 79 | int32 or float32 | 4 | local |  |  | 0xf3e18d |
| 80 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xf3e358 |
| 81 | int32 or float32 | 4 | local |  |  | 0xf3e3c5 |
| 82 | int32 or float32 | 4 | local |  |  | 0xf3e401 |
| 83 | int32 or float32 | 4 | local |  |  | 0xf3e40f |
| 84 | int32 or float32 | 4 | local |  |  | 0xf3e41d |
| 85 | int32 or float32 | 4 | local |  |  | 0xf3e42b |
| 86 | int32 or float32 | 4 | local |  |  | 0xf3e439 |
| 87 | int32 or float32 | 4 | local |  |  | 0xf3e447 |

### mount_variants_tables

- Row reader **0xe54560** (callback 0xe69230, table loader 0xdc3590, name getter 0xf405c0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_groundcover_distribution_maps_tables, trees_climates_jct_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe5458f |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe545a0 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xe545b7 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xe545c4 |

### movie_event_strings_tables

- Row reader **0xdd2ce0** (callback 0xde26e0, table loader 0xe7e470, name getter 0xf40610). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: campaign_character_anim_walk_anim_junctions_tables, effect_bonus_value_population_class_and_religion_junction_tables, uniforms_tables, units_to_gov_type_permissions_tables, unrest_cause_to_demands_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2d1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2d2c |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2d3d |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xdd2d4e |

### mp_general_command_ratings_tables

- Row reader **0xfaf1b0** (callback 0xfaf700, table loader 0xfa5d60, name getter 0xfd2cf0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: battle_sequences_tables, special_edition_enums_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfaf1d9 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xfaf247 |

### names_forts_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xdc8f80, name getter 0xf40660). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### names_groups_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdcc540, name getter 0xf406b0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### names_royalty_tables

- Row reader **0xf3b5b0** (callback 0xf58f00, table loader 0xf2e3d0, name getter 0xf40700). Fields read from the file: 6.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3b5dc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3b5ed |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xf3b604 |
| 3 | string | u16 len + len*UTF-16LE | 0x1c |  |  | 0xf3b612 |
| 4 | int32 or float32 | 4 | 0x28 |  |  | 0xf3b629 |
| 5 | int32 or float32 | 4 | 0x2c |  |  | 0xf3b63d |

### names_tables

- Row reader **0xf3b650** (callback 0xf58f10, table loader 0xf2f5f0, name getter 0xf40750). Fields read from the file: 7.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3b69a |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3b6a3 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3b6ac |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf3b6b5 |
| 4 | int32 or float32 | 4 | 0x30 |  |  | 0xf3b6c5 |
| 5 | bool | 1 | 0x34 |  |  | 0xf3b6d3 |
| 6 | string | u16 len + len*UTF-16LE | 0x38 |  |  | 0xf3b6d9 |

### particle_effects_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf407f0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### pdlc_tables

- Row reader **0xfba3e0** (callback 0xfbab30, table loader 0xfb8fe0, name getter 0xfba4e0). Fields read from the file: 4. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfba405 |
| 1 | int32 or float32 | 4 | 0xc | version != 0 |  | 0xfba425 |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x10 | version >= 2 | flag==0 -> empty string | 0xfba44c |
| 3 | int32 or float32 | 4 | 0x1c | version > 2 |  | 0xfba4b1 |

### policies_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdcc540, name getter 0xf40840). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### population_class_to_applicable_effects_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf408e0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### population_classes_tables

- Row reader **0xf3e6d0** (callback 0xf58f30, table loader 0xdc7db0, name getter 0xf40890). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3e6e6 |
| 1 | bool | 1 | 0xc |  |  | 0xf3e6fd |
| 2 | bool | 1 | 0xd |  |  | 0xf3e711 |
| 3 | bool | 1 | 0xe |  |  | 0xf3e725 |

### projectile_impacts_tables

- Row reader **0xf3e960** (callback 0xf58f50, table loader 0xf32b80, name getter 0xf409d0). Fields read from the file: 17.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3ea17 |
| 1 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xc |  | flag==0 -> empty string | 0xf3ea2f |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x18 |  | flag==0 -> empty string | 0xf3ea81 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xf3ead3 |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x30 |  | flag==0 -> empty string | 0xf3eb25 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x3c |  | flag==0 -> empty string | 0xf3eb83 |
| 6 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x48 |  | flag==0 -> empty string | 0xf3ebd9 |
| 7 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x54 |  | flag==0 -> empty string | 0xf3ec2f |
| 8 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x60 |  | flag==0 -> empty string | 0xf3ec85 |
| 9 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x6c |  | flag==0 -> empty string | 0xf3ecdb |
| 10 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x78 |  | flag==0 -> empty string | 0xf3ed31 |
| 11 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x84 |  | flag==0 -> empty string | 0xf3ed87 |
| 12 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x90 |  | flag==0 -> empty string | 0xf3ede3 |
| 13 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x9c |  | flag==0 -> empty string | 0xf3ee3f |
| 14 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xa8 |  | flag==0 -> empty string | 0xf3ee9b |
| 15 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xb4 |  | flag==0 -> empty string | 0xf3eef7 |
| 16 | string | u16 len + len*UTF-16LE | 0xc0 |  |  | 0xf3ef52 |

### projectile_shot_type_enum_tables

- Row reader **0xf3f6b0** (callback 0xf58f70, table loader 0xe49e80, name getter 0xf40a70). Fields read from the file: 6.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3f6d4 |
| 1 | bool | 1 | 0xc |  |  | 0xf3f6eb |
| 2 | bool | 1 | 0xd |  |  | 0xf3f6ff |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x10 |  | flag==0 -> empty string | 0xf3f714 |
| 4 | bool | 1 | 0x1c |  |  | 0xf3f765 |
| 5 | bool | 1 | 0x1d |  |  | 0xf3f779 |

### projectile_trails_tables

- Row reader **0xf3f790** (callback 0xf58f80, table loader 0xf34f20, name getter 0xf40ac0). Fields read from the file: 12.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3f7b1 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3f7c2 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xf3f7d9 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xf3f7ed |
| 4 | int32 or float32 | 4 | 0x20 |  |  | 0xf3f801 |
| 5 | int32 or float32 | 4 | 0x24 |  |  | 0xf3f815 |
| 6 | int32 or float32 | 4 | 0x28 |  |  | 0xf3f829 |
| 7 | int32 or float32 | 4 | 0x2c |  |  | 0xf3f83d |
| 8 | int32 or float32 | 4 | 0x30 |  |  | 0xf3f851 |
| 9 | int32 or float32 | 4 | 0x34 |  |  | 0xf3f865 |
| 10 | int32 or float32 | 4 | 0x38 |  |  | 0xf3f879 |
| 11 | int32 or float32 | 4 | 0x3c |  |  | 0xf3f88d |

### projectiles_explosions_tables

- Row reader **0xf3e740** (callback 0xf58f40, table loader 0xf319b0, name getter 0xf40930). Fields read from the file: 13. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3e787 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3e798 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3e7a9 |
| 3 | int32 or float32 | 4 | 0x24 |  |  | 0xf3e7c0 |
| 4 | int32 or float32 | 4 | 0x28 |  |  | 0xf3e7d4 |
| 5 | int32 or float32 | 4 | 0x2c |  |  | 0xf3e7e8 |
| 6 | int32 or float32 | 4 | 0x30 |  |  | 0xf3e7fc |
| 7 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x34 |  | flag==0 -> empty string | 0xf3e811 |
| 8 | int32 or float32 | 4 | 0x40 |  |  | 0xf3e866 |
| 9 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x44 |  | flag==0 -> empty string | 0xf3e87b |
| 10 | int32 or float32 | 4 | 0x50 |  |  | 0xf3e8d0 |
| 11 | int32 or float32 | 4 | 0x54 |  |  | 0xf3e8e4 |
| 12 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x58 | version != 0 | flag==0 -> empty string | 0xf3e902 |

### projectiles_missile_type_enum_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf40980). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### public_order_factors_tables

- Row reader **0xf3f8a0** (callback 0xf58f90, table loader 0xdc5980, name getter 0xf40b10). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3f8cf |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3f8e0 |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x18 |  | flag==0 -> empty string | 0xf3f8f8 |

### quotes_people_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xf40b60). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### quotes_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xe454a0, name getter 0xf40bb0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, slots_templates_models_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### random_localisation_strings_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf40c00). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### region_economics_factors_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xf40d40). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### region_unit_resources_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xf40cf0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### regions_continents_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf40c50). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### religion_conversion_mods_tables

- Row reader **0xf3faf0** (callback 0xf58fc0, table loader 0xdc3590, name getter 0xf40de0). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf3fb1f |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3fb30 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xf3fb47 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xf3fb73 |

### religions_tables

- Row reader **0xf3fbb0** (callback 0xf58fd0, table loader 0xf360f0, name getter 0xf40e30). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3fbd1 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf3fbe8 |
| 2 | string | u16 len + len*UTF-16LE | 0x10 |  |  | 0xf3fbf6 |

### resources_tables

- Row reader **0xf3fc10** (callback 0xf58fe0, table loader 0xf372c0, name getter 0xf40e80). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3fc47 |
| 1 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xc |  | flag==0 -> empty string | 0xf3fc5f |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf3fcaa |
| 3 | int32 or float32 | 4 | 0x24 |  |  | 0xf3fcc1 |
| 4 | string | u16 len + len*UTF-16LE | 0x28 |  |  | 0xf3fcd2 |

### sea_climate_details_tables

- Row reader **0xf3fcf0** (callback 0xf58ff0, table loader 0xf38490, name getter 0xf40f20). Fields read from the file: 1.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3fd07 |

### sea_surfaces_tables

- Row reader **0xf3fe40** (callback 0xf59000, table loader 0xf39660, name getter 0xf40f70). Fields read from the file: 19.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf3fe56 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf3fe6d |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xf3fe81 |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xf3fe95 |
| 4 | int32 or float32 | 4 | 0x18 |  |  | 0xf3fea9 |
| 5 | int32 or float32 | 4 | 0x1c |  |  | 0xf3febd |
| 6 | bool | 1 | 0x20 |  |  | 0xf3fed1 |
| 7 | int32 or float32 | 4 | 0x24 |  |  | 0xf3fee5 |
| 8 | int32 or float32 | 4 | 0x28 |  |  | 0xf3fef9 |
| 9 | int32 or float32 | 4 | 0x2c |  |  | 0xf3ff0d |
| 10 | int32 or float32 | 4 | 0x30 |  |  | 0xf3ff21 |
| 11 | int32 or float32 | 4 | 0x34 |  |  | 0xf3ff35 |
| 12 | int32 or float32 | 4 | 0x38 |  |  | 0xf3ff49 |
| 13 | int32 or float32 | 4 | 0x3c |  |  | 0xf3ff5d |
| 14 | int32 or float32 | 4 | 0x40 |  |  | 0xf3ff71 |
| 15 | int32 or float32 | 4 | 0x44 |  |  | 0xf3ff85 |
| 16 | int32 or float32 | 4 | 0x48 |  |  | 0xf3ff99 |
| 17 | int32 or float32 | 4 | 0x4c |  |  | 0xf3ffad |
| 18 | int32 or float32 | 4 | 0x50 |  |  | 0xf3ffc1 |

### seasons_tables

- Row reader **0xdd22a0** (callback 0xde2620, table loader 0xf2d200, name getter 0xf40ed0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: building_factionwide_effects_junctions_tables, campaign_ai_manager_behaviour_junctions_tables, campaign_ai_personality_junctions_tables, campaigns_campaign_variables_junctions_tables, start_pos_faction_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd22c1 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd22d2 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd22e9 |

### ship_names_tables

- Row reader **0xf06c50** (callback 0xf21890, table loader 0xdcb370, name getter 0xf08b70). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf06c87 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf06c9c |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf06cad |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xf06cc5 |

### slots_art_tables

- Row reader **0xf06d20** (callback 0xf218a0, table loader 0xef4970, name getter 0xf08bc0). Fields read from the file: 13.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf06d79 |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf06d8d |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xf06da5 |
| 3 | bool | 1 | 0x68 |  |  | 0xf06df6 |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x30 |  | flag==0 -> empty string | 0xf06e0b |
| 5 | bool | 1 | 0x69 |  |  | 0xf06e5c |
| 6 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x3c |  | flag==0 -> empty string | 0xf06e71 |
| 7 | bool | 1 | 0x6a |  |  | 0xf06ec6 |
| 8 | bool | 1 | 0x6b |  |  | 0xf06eda |
| 9 | int32 or float32 | 4 | 0x60 |  |  | 0xf06eee |
| 10 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x48 |  | flag==0 -> empty string | 0xf06f03 |
| 11 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x54 |  | flag==0 -> empty string | 0xf06f59 |
| 12 | int32 or float32 | 4 | 0x64 |  |  | 0xf06fae |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xf06fbf |

### slots_gdp_values_tables

- Row reader **0xf06ff0** (callback 0xf218b0, table loader 0xe7f690, name getter 0xf08c10). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf07015 |
| 1 | int32 or float32 | 4 | 0x18 |  |  | 0xf0702c |
| 2 | int32 or float32 | 4 | 0x1c |  |  | 0xf07040 |
| 3 | int32 or float32 | 4 | 0x20 |  |  | 0xf07054 |
| - | (no file bytes) | 0 | 0xc |  | derived: string copy into (call:FUN_004f0660) | 0xf07072 |

### slots_tables

- Row reader **0xf07130** (callback 0xf218c0, table loader 0xdcd710, name getter 0xf08cb0). Fields read from the file: 6. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf07147 |
| 1 | bool | 1 | 0xc |  |  | 0xf0715e |
| 2 | bool | 1 | 0xd |  |  | 0xf07172 |
| 3 | bool | 1 | 0xe |  |  | 0xf07186 |
| 4 | bool | 1 | 0xf |  |  | 0xf0719a |
| 5 | bool | 1 | 0x10 | version != 0 |  | 0xf071b7 |

### slots_templates_models_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xdc5980, name getter 0xf08c60). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, start_pos_victory_conditions_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### small_vegetation_climates_jct_tables

- Row reader **0xfb3670** (callback 0xfb4020, table loader 0xfb2270, name getter 0xfb3670). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: battles_to_battle_sky_types_junctions_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfb369c |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xfb36af |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xfb36bf |

### special_edition_enums_tables

- Row reader **0xfaf1b0** (callback 0xfaf700, table loader 0xfa5d60, name getter 0xfaf260). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: battle_sequences_tables, mp_general_command_ratings_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfaf1d9 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xfaf247 |

### stances_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdcc540, name getter 0xf71c90). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### start_pos_calendars_tables

- Row reader **0xf071d0** (callback 0xf218d0, table loader 0xef5b40, name getter 0xf08d00). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf071ee |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf071fe |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xf0720b |
| 3 | string | u16 len + len*UTF-16LE | 0x14 |  |  | 0xf07212 |
| 4 | int32 or float32 | 4 | 0x20 |  |  | 0xf07222 |

### start_pos_character_ancillaries_tables

- Row reader **0xdd29f0** (callback 0xde26b0, table loader 0xdc5980, name getter 0xf08d50). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: anim_reference_poses_tables, campaign_anim_action_to_sets_tables, campaign_map_towns_and_ports_tables, gun_type_to_projectiles_tables, start_pos_character_traits_tables, start_pos_naval_units_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2a1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2a24 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2a2d |

### start_pos_character_to_forts_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdcc540, name getter 0xf08df0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### start_pos_character_to_settlements_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdcc540, name getter 0xf08e40). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### start_pos_character_traits_tables

- Row reader **0xdd29f0** (callback 0xde26b0, table loader 0xdc5980, name getter 0xf08e90). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: anim_reference_poses_tables, campaign_anim_action_to_sets_tables, campaign_map_towns_and_ports_tables, gun_type_to_projectiles_tables, start_pos_character_ancillaries_tables, start_pos_naval_units_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2a1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2a24 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2a2d |

### start_pos_characters_tables

- Row reader **0xf07230** (callback 0xf218e0, table loader 0xef6d10, name getter 0xf08da0). Fields read from the file: 13. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf0728f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf072a0 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf072b1 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xf072c9 |
| 4 | int32 or float32 | 4 | 0x30 |  |  | 0xf0731e |
| 5 | string | u16 len + len*UTF-16LE | 0x34 |  |  | 0xf0732f |
| 6 | int32 or float32 | 4 | 0x40 |  |  | 0xf07346 |
| 7 | int32 or float32 | 4 | 0x44 |  |  | 0xf0735a |
| 8 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x48 |  | flag==0 -> empty string | 0xf0736f |
| 9 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x54 |  | flag==0 -> empty string | 0xf073c5 |
| 10 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x60 | version != 0 | flag==0 -> empty string | 0xf07424 |
| 11 | bool | 1 | 0x6c | version >= 2 |  | 0xf0748d |
| 12 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x70 | version >= 3 | flag==0 -> empty string | 0xf074b0 |

### start_pos_diplomacy_tables

- Row reader **0xf07520** (callback 0xf218f0, table loader 0xef7ee0, name getter 0xf08ee0). Fields read from the file: 7.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf07563 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf07574 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf07585 |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf07596 |
| 4 | bool | 1 | 0x30 |  |  | 0xf075b1 |
| 5 | bool | 1 | 0x31 |  |  | 0xf075c5 |
| 6 | string | u16 len + len*UTF-16LE | 0x34 |  |  | 0xf075d6 |

### start_pos_faction_effects_tables

- Row reader **0xdd22a0** (callback 0xde2620, table loader 0xfa83c0, name getter 0xfa97e0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: building_factionwide_effects_junctions_tables, campaign_ai_manager_behaviour_junctions_tables, campaign_ai_personality_junctions_tables, campaigns_campaign_variables_junctions_tables, seasons_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd22c1 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd22d2 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd22e9 |

### start_pos_factions_tables

- Row reader **0xf075f0** (callback 0xf21900, table loader 0xef90b0, name getter 0xf08f30). Fields read from the file: 32. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf0767a |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf0768f |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf076a0 |
| 3 | bool | 1 | 0x24 |  |  | 0xf076bb |
| 4 | int32 or float32 | 4 | 0x28 |  |  | 0xf076cf |
| 5 | int32 or float32 | 4 | 0x2c |  |  | 0xf076e3 |
| 6 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xf076f1 |
| 7 | string | u16 len + len*UTF-16LE | 0x3c |  |  | 0xf07702 |
| 8 | bool | 1 | 0x48 |  |  | 0xf07719 |
| 9 | string | u16 len + len*UTF-16LE | 0x4c |  |  | 0xf0772a |
| 10 | string | u16 len + len*UTF-16LE | 0x58 |  |  | 0xf0773e |
| 11 | string | u16 len + len*UTF-16LE | 0x64 |  |  | 0xf07752 |
| 12 | int32 or float32 | 4 | 0x70 |  |  | 0xf07769 |
| 13 | int32 or float32 | 4 | 0x74 |  |  | 0xf0777d |
| 14 | int32 or float32 | 4 | 0x78 |  |  | 0xf07791 |
| 15 | int32 or float32 | 4 | 0x7c |  |  | 0xf077a5 |
| 16 | int32 or float32 | 4 | 0x80 |  |  | 0xf077bc |
| 17 | int32 or float32 | 4 | 0x84 |  |  | 0xf077d3 |
| 18 | int32 or float32 | 4 | 0x88 |  |  | 0xf077ea |
| 19 | int32 or float32 | 4 | 0x8c |  |  | 0xf07801 |
| 20 | int32 or float32 | 4 | 0x90 |  |  | 0xf07818 |
| 21 | int32 or float32 | 4 | 0x94 |  |  | 0xf0782f |
| 22 | int32 or float32 | 4 | 0x98 |  |  | 0xf07846 |
| 23 | int32 or float32 | 4 | 0x9c |  |  | 0xf0785d |
| 24 | bool | 1 | 0xa0 | version != 0 |  | 0xf07883 |
| 25 | bool | 1 | 0xa1 | version != 0 |  | 0xf07894 |
| 26 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xa4 | version >= 2 | flag==0 -> empty string | 0xf078bf |
| 27 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xb0 | version >= 2 | flag==0 -> empty string | 0xf0791b |
| 28 | int32 or float32 | 4 | 0xbc | version > 2 |  | 0xf079af |
| 29 | int32 or float32 | 4 | 0xc0 | version > 2 |  | 0xf079c0 |
| 30 | int32 or float32 | 4 | 0xc4 | version > 2 |  | 0xf079d7 |
| 31 | int32 or float32 | 4 | 0xc8 | version > 2 |  | 0xf079ee |

### start_pos_fort_garrisons_tables

- Row reader **0xf07a30** (callback 0xf21910, table loader 0xdc3590, name getter 0xf08f80). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: agent_to_building_levels_tables, start_pos_land_units_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf07a5b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf07a64 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf07a6d |
| 3 | int32 or float32 | 4 | 0x24 |  |  | 0xf07a7d |

### start_pos_forts_tables

- Row reader **0xf07a90** (callback 0xf21920, table loader 0xefa280, name getter 0xf08fd0). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf07aaf |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf07ab8 |
| 2 | int32 or float32 | 4 | 0x1c |  |  | 0xf07ac8 |
| 3 | int32 or float32 | 4 | 0x20 |  |  | 0xf07ad5 |
| 4 | int32 or float32 | 4 | 0x24 |  |  | 0xf07ae2 |
| - | (no file bytes) | 0 | 0x18 |  | derived: parse previous string as int into (call:FUN_004f3720) | 0xf07aed |

### start_pos_land_units_tables

- Row reader **0xf07a30** (callback 0xf21910, table loader 0xdc3590, name getter 0xf09020). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: agent_to_building_levels_tables, start_pos_fort_garrisons_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf07a5b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf07a64 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf07a6d |
| 3 | int32 or float32 | 4 | 0x24 |  |  | 0xf07a7d |

### start_pos_naval_units_tables

- Row reader **0xdd29f0** (callback 0xde26b0, table loader 0xdc5980, name getter 0xf09070). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: anim_reference_poses_tables, campaign_anim_action_to_sets_tables, campaign_map_towns_and_ports_tables, gun_type_to_projectiles_tables, start_pos_character_ancillaries_tables, start_pos_character_traits_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2a1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2a24 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2a2d |

### start_pos_region_religions_tables

- Row reader **0xf07cc0** (callback 0xf21940, table loader 0xe49e80, name getter 0xf09160). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | int32 or float32 | 4 | 0xc |  |  | 0xf07ceb |
| 1 | string | u16 len + len*UTF-16LE | 0x10 |  |  | 0xf07cf9 |
| 2 | int32 or float32 | 4 | 0x1c |  |  | 0xf07d10 |
| - | (no file bytes) | 0 | 0x10 |  | derived: string copy into (call:FUN_004f1200) | 0xf07d49 |

### start_pos_regions_tables

- Row reader **0xf07b00** (callback 0xf21930, table loader 0xefb450, name getter 0xf09110). Fields read from the file: 18.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | int32 or float32 | 4 | 0x0 |  |  | 0xf07b5c |
| 1 | string | u16 len + len*UTF-16LE | 0x4 |  |  | 0xf07b6e |
| 2 | string | u16 len + len*UTF-16LE | 0x10 |  |  | 0xf07b7f |
| 3 | string | u16 len + len*UTF-16LE | 0x1c |  |  | 0xf07b90 |
| 4 | bool | 1 | 0x28 |  |  | 0xf07bab |
| 5 | int32 or float32 | 4 | 0x2c |  |  | 0xf07bbf |
| 6 | int32 or float32 | 4 | 0x30 |  |  | 0xf07bd3 |
| 7 | int32 or float32 | 4 | 0x34 |  |  | 0xf07be7 |
| 8 | int32 or float32 | 4 | 0x38 |  |  | 0xf07bfb |
| 9 | int32 or float32 | 4 | 0x3c |  |  | 0xf07c0f |
| 10 | bool | 1 | 0x40 |  |  | 0xf07c23 |
| 11 | bool | 1 | 0x41 |  |  | 0xf07c37 |
| 12 | int32 or float32 | 4 | 0x44 |  |  | 0xf07c4b |
| 13 | string | u16 len + len*UTF-16LE | 0x48 |  |  | 0xf07c59 |
| 14 | string | u16 len + len*UTF-16LE | 0x60 |  |  | 0xf07c6d |
| 15 | string | u16 len + len*UTF-16LE | 0x54 |  |  | 0xf07c81 |
| 16 | bool | 1 | 0x6c |  |  | 0xf07c98 |
| 17 | int32 or float32 | 4 | 0x70 |  |  | 0xf07cac |

### start_pos_regions_to_unit_resources_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdc11a0, name getter 0xf090c0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### start_pos_royalty_names_tables

- Row reader **0xe54f10** (callback 0xe692b0, table loader 0xefc670, name getter 0xf091b0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_type_faction_presets_tables, commodities_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe54f26 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe54f3d |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xe54f51 |

### start_pos_settlement_garrisons_tables

- Row reader **0xf07d90** (callback 0xf21950, table loader 0xe49e80, name getter 0xf09200). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf07dae |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf07dbe |
| 2 | string | u16 len + len*UTF-16LE | 0x10 |  |  | 0xf07dc5 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xf07dd5 |

### start_pos_settlements_tables

- Row reader **0xf07df0** (callback 0xf21960, table loader 0xefd8c0, name getter 0xf09250). Fields read from the file: 9.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf07e30 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf07e47 |
| 2 | u8 (not stored) | 1 | - |  | read into a local | 0xf07e6e |
| 3 | string | u16 len + len*UTF-16LE | ? |  | string read inlined | 0xf07e98 |
| 4 | int32 or float32 | 4 | 0x20 |  |  | 0xf0803a |
| 5 | int32 or float32 | 4 | 0x24 |  |  | 0xf0804e |
| 6 | int32 or float32 | 4 | 0x28 |  |  | 0xf08062 |
| 7 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x2c |  | flag==0 -> empty string | 0xf08077 |
| 8 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x38 |  | flag==0 -> empty string | 0xf080cd |

### start_pos_slots_tables

- Row reader **0xf08130** (callback 0xf21970, table loader 0xefeb10, name getter 0xf092a0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf0815f |
| 1 | int32 or float32 | 4 | 0x18 |  |  | 0xf08176 |
| 2 | int32 or float32 | 4 | 0x1c |  |  | 0xf0818a |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x20 |  | flag==0 -> empty string | 0xf0819f |
| - | (no file bytes) | 0 | 0xc |  | derived: string copy into (call:FUN_004f0660) | 0xf081f9 |

### start_pos_technologies_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdc11a0, name getter 0xf092f0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### start_pos_towns_and_ports_tables

- Row reader **0xf08240** (callback 0xf21980, table loader 0xeffce0, name getter 0xf09340). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf0826f |
| 1 | int32 or float32 | 4 | 0x18 |  |  | 0xf08286 |
| 2 | int32 or float32 | 4 | 0x1c |  |  | 0xf0829a |
| 3 | int32 or float32 | 4 | 0x20 |  |  | 0xf082ae |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0xf082c3 |
| - | (no file bytes) | 0 | 0xc |  | derived: string copy into (call:FUN_004f0660) | 0xf0831d |

### start_pos_victory_conditions_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xfa9da0, name getter 0xfab1e0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, town_wealth_growth_factors_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### state_gift_values_tables

- Row reader **0xf08360** (callback 0xf21990, table loader 0xdc7db0, name getter 0xf09390). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: campaign_variables_tables, diplomatic_relations_attitudes_tables, entity_training_levels_tables, taxes_levels_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf08373 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf0838c |

### subtitles_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xfa27e0, name getter 0xfd5d80). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### taxes_classes_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf09430). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, technology_threads_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### taxes_effects_jct_tables

- Row reader **0xf08530** (callback 0xf219b0, table loader 0xdc3590, name getter 0xf09480). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf0855f |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf08570 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xf08587 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xf085b3 |

### taxes_keys_tables

- Row reader **0xf085f0** (callback 0xf219c0, table loader 0xdcb370, name getter 0xf094d0). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf0862d |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf0863e |
| 2 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf0864f |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xf0867e |

### taxes_levels_tables

- Row reader **0xf08360** (callback 0xf21990, table loader 0xdc7db0, name getter 0xf09520). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: campaign_variables_tables, diplomatic_relations_attitudes_tables, entity_training_levels_tables, state_gift_values_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf08373 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xf0838c |

### technology_faction_junctions_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdc11a0, name getter 0xf095c0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### technology_required_building_levels_junctions_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdc11a0, name getter 0xf09660). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### technology_required_technology_junctions_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdc11a0, name getter 0xf096b0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### technology_threads_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xf09700). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, trigger_events_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### terrain_tilesets_tables

- Row reader **0xfa5680** (callback 0xfa5c40, table loader 0xfa4220, name getter 0xfd2190). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: achievements_tables, trade_node_groups_tables, unit_info_card_abilities_strings_tables, unit_special_ability_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfa56a9 |

### town_wealth_growth_factors_tables

- Row reader **0xe53fb0** (callback 0xe691d0, table loader 0xdc5980, name getter 0xf09750). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_bridge_subculture_jcts_tables, battlefield_building_categories_tables, effect_bonus_value_agent_junction_tables, effect_bonus_value_building_chain_junctions_tables, effect_bonus_value_commodity_junction_tables, effect_bonus_value_population_class_junction_tables, effect_bonus_value_projectile_junctions_tables, effect_bonus_value_religion_junction_tables, effect_bonus_value_resource_junction_tables, effect_bonus_value_shot_type_junctions_tables, effect_bonus_value_unit_ability_junctions_tables, effect_bonus_value_unit_category_junction_tables, effect_bonus_value_unit_class_junction_tables, names_forts_tables, quotes_tables, slots_templates_models_tables, start_pos_victory_conditions_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe53fdc |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe53fed |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe53ffe |

### trade_details_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xf097a0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### trade_node_groups_tables

- Row reader **0xfa5680** (callback 0xfa5c40, table loader 0xfa4220, name getter 0xfcd8c0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: achievements_tables, terrain_tilesets_tables, unit_info_card_abilities_strings_tables, unit_special_ability_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfa56a9 |

### trade_nodes_tables

- Row reader **0xfbf5c0** (callback 0xfbfdc0, table loader 0xfbe1c0, name getter 0xfbf5c0). Fields read from the file: 6. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfbf5ef |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfbf600 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xfbf617 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xfbf62b |
| 4 | int32 or float32 | 4 | 0x20 |  |  | 0xfbf63f |
| 5 | string | u16 len + len*UTF-16LE | 0x24 | version != 0 |  | 0xfbf656 |

### trade_theatre_commodities_tables

- Row reader **0xf08810** (callback 0xf219e0, table loader 0xf03250, name getter 0xf097f0). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf0883f |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf08850 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xf08867 |
| 3 | int32 or float32 | 4 | 0x28 |  |  | 0xf0887b |
| 4 | int32 or float32 | 4 | 0x2c |  |  | 0xf0888f |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xf088bb |

### trait_ability_effects_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf09840). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### trait_attribute_effects_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf09890). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_situation_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### trait_attribute_situation_effects_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf098e0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_level_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### trait_categories_tables

- Row reader **0xf088f0** (callback 0xf219f0, table loader 0xdcc540, name getter 0xf09930). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: cdir_campaign_junctions_tables, cdir_faction_junctions_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf08914 |
| 1 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xc |  | flag==0 -> empty string | 0xf0892c |

### trait_info_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdcc540, name getter 0xf09980). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### trait_level_effects_tables

- Row reader **0xdd2250** (callback 0xde2610, table loader 0xdbdb90, name getter 0xf099d0). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: agent_spawning_to_building_chains_tables, agent_spawning_to_government_types_tables, agent_spawning_to_policies_tables, agent_to_agent_abilities_tables, ancillary_to_ability_effects_tables, ancillary_to_attribute_effects_tables, ancillary_to_attribute_situation_effects_tables, ancillary_to_effects_tables, building_effects_junction_tables, government_types_to_effects_tables, technology_effects_junction_tables, trait_ability_effects_tables, trait_attribute_effects_tables, trait_attribute_situation_effects_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd226f |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2278 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xdd2288 |

### trait_to_antitraits_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf09a20). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### trait_to_excluded_cultures_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf09a70). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_included_agents_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### trait_to_included_agents_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xf09ac0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trigger_event_to_excluded_agent_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### trees_climates_jct_tables

- Row reader **0xe54560** (callback 0xe69230, table loader 0xdc3590, name getter 0xf09b10). Fields read from the file: 3.
- The same reader (and therefore the same row layout) is used by: battle_groundcover_distribution_maps_tables, mount_variants_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe5458f |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe545a0 |
| 2 | int32 or float32 | 4 | 0x24 |  |  | 0xe545b7 |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xe545c4 |

### trees_tables

- Row reader **0xf08990** (callback 0xf21a00, table loader 0xdcd710, name getter 0xf09b60). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf089a6 |
| 1 | bool | 1 | 0xc |  |  | 0xf089bd |
| 2 | bool | 1 | 0xe |  |  | 0xf089d1 |
| 3 | bool | 1 | 0xd |  |  | 0xf089e5 |
| 4 | int32 or float32 | 4 | 0x10 |  |  | 0xf089f9 |

### trigger_effects_tables

- Row reader **0xf08a10** (callback 0xf21a10, table loader 0xf04420, name getter 0xf09bb0). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xf08a3b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf08a44 |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf08a4d |
| 3 | int32 or float32 | 4 | 0x24 |  |  | 0xf08a5d |
| 4 | int32 or float32 | 4 | 0x28 |  |  | 0xf08a6a |

### trigger_event_to_excluded_agent_types_tables

- Row reader **0xe55090** (callback 0xe692e0, table loader 0xdc11a0, name getter 0xe866e0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: agent_attribute_situations_tables, agent_attributes_tables, agent_to_bribe_actions_tables, ancillary_included_subcultures_tables, ancillary_to_excluded_ancillaries_tables, ancillary_to_included_agents_tables, bribe_actions_tables, building_chain_to_slots_tables, disaster_to_ground_types_tables, effect_bonus_value_basic_junction_tables, ministerial_position_default_names_tables, ministerial_positions_to_governorships_tables, names_groups_tables, policies_tables, population_class_to_applicable_effects_tables, stances_tables, start_pos_character_to_forts_tables, start_pos_character_to_settlements_tables, trait_info_tables, trait_to_antitraits_tables, trait_to_excluded_cultures_tables, trait_to_included_agents_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe550af |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe550b8 |

### trigger_events_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xe86690). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, unit_category_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### uniform_to_faction_colours_tables

- Row reader **0xfc3480** (callback 0xfc3bc0, table loader 0xfc2060, name getter 0xfc3580). Fields read from the file: 11.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfc34a1 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xfc34b2 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xfc34c9 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xfc34dd |
| 4 | int32 or float32 | 4 | 0x20 |  |  | 0xfc34f1 |
| 5 | int32 or float32 | 4 | 0x24 |  |  | 0xfc3505 |
| 6 | int32 or float32 | 4 | 0x28 |  |  | 0xfc3519 |
| 7 | int32 or float32 | 4 | 0x2c |  |  | 0xfc352d |
| 8 | int32 or float32 | 4 | 0x30 |  |  | 0xfc3541 |
| 9 | int32 or float32 | 4 | 0x34 |  |  | 0xfc3555 |
| 10 | int32 or float32 | 4 | 0x38 |  |  | 0xfc3569 |

### uniforms_tables

- Row reader **0xdd2ce0** (callback 0xde26e0, table loader 0xfc05b0, name getter 0xfc19d0). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: campaign_character_anim_walk_anim_junctions_tables, effect_bonus_value_population_class_and_religion_junction_tables, movie_event_strings_tables, units_to_gov_type_permissions_tables, unrest_cause_to_demands_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2d1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2d2c |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2d3d |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xdd2d4e |

### unit_abilities_tables

- Row reader **0xe85970** (callback 0xedc680, table loader 0xe49e80, name getter 0xe86870). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe85994 |
| 1 | bool | 1 | 0xc |  |  | 0xe859ab |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x10 |  | flag==0 -> empty string | 0xe859c0 |
| 3 | bool | 1 | 0x1c |  |  | 0xe85a11 |

### unit_category_tables

- Row reader **0xdd2220** (callback 0xde2600, table loader 0xdbc9c0, name getter 0xe868c0). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: advice_threads_tables, ancillary_info_tables, battle_climate_groupings_tables, battle_types_tables, battlefield_building_transformations_tables, building_description_texts_tables, campaign_ai_managers_tables, campaign_anim_sets_tables, campaign_map_slots_templates_rotations_tables, commodities_demand_drivers_tables, commodity_unit_names_tables, diplomacy_factor_strings_tables, diplomacy_strings_tables, empires_tables, governorships_tables, groupings_military_tables, groupings_tables, mounts_tables, particle_effects_tables, projectiles_missile_type_enum_tables, random_localisation_strings_tables, regions_continents_tables, taxes_classes_tables, technology_threads_tables, trigger_events_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | destination offset inferred | 0xdd2233 |

### unit_class_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xe86910). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### unit_class_to_population_class_priorities_tables

- Row reader **0xe85a30** (callback 0xedc690, table loader 0xe75500, name getter 0xe86960). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe85a46 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe85a5d |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xe85a71 |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xe85a85 |

### unit_class_to_unit_ability_junctions_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdc11a0, name getter 0xe869b0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### unit_info_card_abilities_strings_tables

- Row reader **0xfa5680** (callback 0xfa5c40, table loader 0xfa4220, name getter 0xfa7d40). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: achievements_tables, terrain_tilesets_tables, trade_node_groups_tables, unit_special_ability_types_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfa56a9 |

### unit_movement_modifiers_tables

- Row reader **0xe85aa0** (callback 0xedc6a0, table loader 0xe766d0, name getter 0xe86a50). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe85ab6 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe85acd |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xe85ae1 |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xe85af5 |
| 4 | int32 or float32 | 4 | 0x18 |  |  | 0xe85b09 |

### unit_regiment_names_tables

- Row reader **0xe85ec0** (callback 0xedc6c0, table loader 0xe78a70, name getter 0xe86af0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe85eec |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe85efd |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe85f0e |
| 3 | int32 or float32 | 4 | 0x24 |  |  | 0xe85f25 |

### unit_required_technology_junctions_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdc11a0, name getter 0xe86b40). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### unit_special_abilities_tables

- Row reader **0xfc5ed0** (callback 0xfc6450, table loader 0xfbac50, name getter 0xfc5fb0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfc5efc |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xfc5f68 |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xfc5f7c |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xfc5f90 |

### unit_special_ability_types_tables

- Row reader **0xfa5680** (callback 0xfa5c40, table loader 0xfa4220, name getter 0xfc5920). Fields read from the file: 1.
- The same reader (and therefore the same row layout) is used by: achievements_tables, terrain_tilesets_tables, trade_node_groups_tables, unit_info_card_abilities_strings_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  | string read inlined | 0xfa56a9 |

### unit_stats_naval_crew_tables

- Row reader **0xe86010** (callback 0xedc6e0, table loader 0xe7af00, name getter 0xe86be0). Fields read from the file: 16.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe8606e |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe8608b |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xe860a5 |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xe860bf |
| 4 | int32 or float32 | 4 | 0x18 |  |  | 0xe860d9 |
| 5 | string | u16 len + len*UTF-16LE | 0x1c |  |  | 0xe860eb |
| 6 | bool | 1 | 0x28 |  |  | 0xe86106 |
| 7 | int32 or float32 | 4 | 0x2c |  |  | 0xe8611e |
| 8 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xe8612c |
| 9 | string | u16 len + len*UTF-16LE | 0x3c |  |  | 0xe8613d |
| 10 | string | u16 len + len*UTF-16LE | 0x48 |  |  | 0xe8614e |
| 11 | string | u16 len + len*UTF-16LE | 0x54 |  |  | 0xe86166 |
| 12 | string | u16 len + len*UTF-16LE | 0x60 |  |  | 0xe8617a |
| 13 | int32 or float32 | 4 | 0x6c |  |  | 0xe86191 |
| 14 | int32 or float32 | 4 | 0x70 |  |  | 0xe861a5 |
| 15 | int32 or float32 | 4 | 0x74 |  |  | 0xe861b9 |

### unit_stats_naval_crew_to_factions_tables

- Row reader **0xe861d0** (callback 0xedc6f0, table loader 0xe7c0d0, name getter 0xe86c30). Fields read from the file: 8.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe8623a |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe8624f |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe86260 |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xe86271 |
| 4 | string | u16 len + len*UTF-16LE | 0x30 |  |  | 0xe86282 |
| 5 | string | u16 len + len*UTF-16LE | 0x3c |  |  | 0xe8629a |
| 6 | string | u16 len + len*UTF-16LE | 0x48 |  |  | 0xe862ae |
| 7 | string | u16 len + len*UTF-16LE | 0x54 |  |  | 0xe862c2 |

### unit_stats_naval_experience_bonuses_tables

- Row reader **0xe862e0** (callback 0xedc700, table loader 0xe7d2a0, name getter 0xe86c80). Fields read from the file: 7.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe862f6 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xe8630d |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xe86321 |
| 3 | int32 or float32 | 4 | 0x14 |  |  | 0xe86335 |
| 4 | int32 or float32 | 4 | 0x18 |  |  | 0xe86349 |
| 5 | int32 or float32 | 4 | 0x1c |  |  | 0xe8635d |
| 6 | int32 or float32 | 4 | 0x20 |  |  | 0xe86371 |

### unit_to_unit_abilities_junctions_tables

- Row reader **0xe86380** (callback 0xedc710, table loader 0xdc5980, name getter 0xe86cd0). Fields read from the file: 2.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe863ac |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xe863bf |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xe863ee |

### units_to_exclusive_faction_permissions_tables

- Row reader **0xe85910** (callback 0xedc670, table loader 0xdbdb90, name getter 0xe86730). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe85931 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe85942 |
| 2 | bool | 1 | 0x18 |  |  | 0xe85959 |

### units_to_gov_type_permissions_tables

- Row reader **0xdd2ce0** (callback 0xde26e0, table loader 0xe742a0, name getter 0xe867d0). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: campaign_character_anim_walk_anim_junctions_tables, effect_bonus_value_population_class_and_religion_junction_tables, movie_event_strings_tables, uniforms_tables, unrest_cause_to_demands_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2d1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2d2c |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2d3d |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xdd2d4e |

### units_to_gov_types_conversion_jcts_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdcc540, name getter 0xe86780). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### units_to_groupings_military_permissions_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xdc11a0, name getter 0xe86820). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_special_editions_juncs_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### units_to_special_editions_juncs_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0xfad7d0, name getter 0xfaf820). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, warscape_equipment_items_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### unrest_cause_to_demands_tables

- Row reader **0xdd2ce0** (callback 0xde26e0, table loader 0xe7e470, name getter 0xe86d20). Fields read from the file: 4.
- The same reader (and therefore the same row layout) is used by: campaign_character_anim_walk_anim_junctions_tables, effect_bonus_value_population_class_and_religion_junction_tables, movie_event_strings_tables, uniforms_tables, units_to_gov_type_permissions_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd2d1b |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2d2c |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xdd2d3d |
| 3 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xdd2d4e |

### videos_subtitles_junctions_tables

- Row reader **0xfa17d0** (callback 0xfa2470, table loader 0xfa02d0, name getter 0xfa18c0). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfa1801 |
| 1 | int32 or float32 | 4 | 0xc |  |  | 0xfa1818 |
| 2 | int32 or float32 | 4 | 0x10 |  |  | 0xfa182c |
| 3 | string | u16 len + len*UTF-16LE | 0x14 |  |  | 0xfa183a |

### videos_tables

- Row reader **0xfd5600** (callback 0xfd5b60, table loader 0xfd4200, name getter 0xfd56a0). Fields read from the file: 3. **Versioned** (the fields marked with a guard are read only when the file header version satisfies the guard).

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xfd5625 |
| 1 | string | u16 len + len*UTF-16LE | 0xc | version != 0 |  | 0xfd5642 |
| 2 | int32 or float32 | 4 | 0x18 | version != 0 |  | 0xfd5656 |

### warscape_animated_lod_tables

- Row reader **0x011c43e0** (callback 0x1225180, table loader 0x11b8ed0, name getter 0x11c4b80). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x8 |  |  | 0x011c4424 |
| 1 | int32 or float32 | 4 | 0x14 |  |  | 0x011c443b |
| 2 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0x011c444c |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0x011c4617 |

### warscape_animated_tables

- Row reader **0x011c46a0** (callback 0x1225190, table loader 0x11ba0f0, name getter 0x11c4bd0). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: warscape_rigid_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x8 |  |  | 0x011c46dd |
| 1 | string | u16 len + len*UTF-16LE | 0x14 |  |  | 0x011c46ee |

### warscape_equipment_items_tables

- Row reader **0xdd2620** (callback 0xde2650, table loader 0x11bb310, name getter 0x11c4c20). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: ancillary_types_tables, battle_city_subculture_jct_tables, battle_terrain_set_groupings_tables, building_level_required_technology_junctions_tables, campaign_character_anim_sets_tables, campaign_map_tooltips_tables, historical_character_traits_tables, mission_activities_tables, mission_effects_tables, mission_sources_tables, quotes_people_tables, region_economics_factors_tables, region_unit_resources_tables, start_pos_regions_to_unit_resources_tables, start_pos_technologies_tables, subtitles_tables, technology_faction_junctions_tables, technology_required_building_levels_junctions_tables, technology_required_technology_junctions_tables, trade_details_tables, unit_class_tables, unit_class_to_unit_ability_junctions_tables, unit_required_technology_junctions_tables, units_to_gov_types_conversion_jcts_tables, units_to_groupings_military_permissions_tables, units_to_special_editions_juncs_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xdd263e |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xdd2651 |

### warscape_equipment_themes_tables

- Row reader **0x011c4700** (callback 0x12251a0, table loader 0x11bc5d0, name getter 0x11c4c70). Fields read from the file: 6.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0x011c4747 |
| 1 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0xc |  | flag==0 -> empty string | 0x011c475f |
| 2 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x18 |  | flag==0 -> empty string | 0x011c47b1 |
| 3 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x24 |  | flag==0 -> empty string | 0x011c4803 |
| 4 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x30 |  | flag==0 -> empty string | 0x011c4859 |
| 5 | optional_string | u8 flag; if flag!=0: u16 len + len*UTF-16LE | 0x3c |  | flag==0 -> empty string | 0x011c48af |

### warscape_naval_lod_tables

- Row reader **0xe86430** (callback 0xedc720, table loader 0xe4c220, name getter 0xe86d70). Fields read from the file: 4.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe86463 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe86474 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xe8648b |
| 3 | string | u16 len + len*UTF-16LE | 0x1c |  |  | 0xe86499 |
| - | (no file bytes) | 0 | 0x1c |  | derived: string copy into (call:FUN_004f1200) | 0xe86545 |

### warscape_rigid_lod_range_tables

- Row reader **0x011c4910** (callback 0x12251b0, table loader 0x11bd7a0, name getter 0x11c4cc0). Fields read from the file: 1.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | int32 or float32 | 4 | 0x8 |  |  | 0x011c493c |

### warscape_rigid_lod_tables

- Row reader **0x011c4950** (callback 0x12251c0, table loader 0x11be9c0, name getter 0x11c4d10). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x8 |  |  | 0x011c49a1 |
| 1 | string | u16 len + len*UTF-16LE | 0x14 |  |  | 0x011c49b2 |
| 2 | string | u16 len + len*UTF-16LE | 0x20 |  |  | 0x011c49c3 |
| - | (no file bytes) | 0 | 0x14 |  | derived: string copy into (call:FUN_004f0660) | 0x011c4a2a |
| - | (no file bytes) | 0 | 0x20 |  | derived: string copy into (call:FUN_004f1200) | 0x011c4a77 |

### warscape_rigid_tables

- Row reader **0x011c46a0** (callback 0x1225190, table loader 0x11ba0f0, name getter 0x11c4d60). Fields read from the file: 2.
- The same reader (and therefore the same row layout) is used by: warscape_animated_tables

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x8 |  |  | 0x011c46dd |
| 1 | string | u16 len + len*UTF-16LE | 0x14 |  |  | 0x011c46ee |

### warscape_trees_tables

- Row reader **0xf08a80** (callback 0xf21a20, table loader 0xdcb370, name getter 0xf09c00). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xf08abd |
| 1 | string | u16 len + len*UTF-16LE | 0x18 |  |  | 0xf08ace |
| 2 | string | u16 len + len*UTF-16LE | 0x24 |  |  | 0xf08adf |
| - | (no file bytes) | 0 | 0x18 |  | derived: string copy into (call:FUN_004f1200) | 0xf08aef |

### warscape_underlay_textures_tables

- Row reader **0x011c4b00** (callback 0x12251d0, table loader 0x11bfbe0, name getter 0x11c4db0). Fields read from the file: 3.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x8 |  |  | 0x011c4b3d |
| 1 | string | u16 len + len*UTF-16LE | 0x14 |  |  | 0x011c4b4e |
| 2 | int32 or float32 | 4 | 0x20 |  |  | 0x011c4b65 |

### wind_levels_tables

- Row reader **0xe865b0** (callback 0xedc730, table loader 0xe7f690, name getter 0xe86dc0). Fields read from the file: 5.

| # | type | bytes in file | BUILDER offset | version guard | note | read site |
|---|---|---|---|---|---|---|
| 0 | string | u16 len + len*UTF-16LE | 0x0 |  |  | 0xe865e2 |
| 1 | string | u16 len + len*UTF-16LE | 0xc |  |  | 0xe865f3 |
| 2 | int32 or float32 | 4 | 0x18 |  |  | 0xe86607 |
| 3 | int32 or float32 | 4 | 0x1c |  |  | 0xe8661b |
| 4 | int32 or float32 | 4 | 0x20 |  |  | 0xe8662f |

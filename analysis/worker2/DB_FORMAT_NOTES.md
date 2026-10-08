# NTW binary DB table format (Worker 2, authoritative byte-level spec)

All DB tables live in `data.pack` at `db\<table>_tables\<table>` (one file per table in NTW; 310 tables). No other pack contains `db\` entries. local_en_patch.pack is the only patch pack, and it holds just text and advisor audio.

## Header (CONFIRMED from bytes, all 310 tables)
```
optional: FC FD FE FF  u32 version     present only if version > 0 (22 tables: 16 at v1, 2 at v2, 2 at v3, units v4, unit_stats_land v5)
u8   0x01                              marker, 1 in every table
u32  row_count
rows...                                no per-row framing, no column names, no GUID (NTW predates GUIDs)
```
Example: `units` = `fc fd fe ff 04 00 00 00 | 01 | ba 01 00 00` = v4, 442 rows. Tables without the FC FD FE FF block are version 0.
An empty table is 5 bytes: `01 00 00 00 00`.

## Field encodings (CONFIRMED)
| code | type | encoding |
|---|---|---|
| `str`  | string | u16 count of UTF-16 code units, then UTF-16LE text, no terminator |
| `ostr` | optional string | u8 flag; 0 = absent (1 byte total); 1 = followed by a `str` |
| `bool` | bool | u8 0/1 |
| `i32`  | int | 4 bytes LE |
| `f32`  | float | 4 bytes LE IEEE754 |
| `i16`  | short | 2 bytes (fallback only; not yet observed in a solved table) |

i32 and f32 are indistinguishable structurally, so a 4-byte column is classified from its values: int if every value satisfies |i| < 10^7, otherwise float if every value is 0 or 1e-5 <= |f| <= 1e7. **A column that is all zeros is reported as i32 but may really be f32 (UNKNOWN).** Likewise, columns of 0/1 bytes may be bool, or the start of a run that is ambiguous. Treat Worker 1's BUILDER-derived types as authoritative where they conflict.

## How schemas are inferred (since the files carry no schema)
`data_tools` searches for a type sequence that (1) parses every row, (2) ends exactly at EOF, and (3) gives "plausible" numeric columns. The method: guess the row-1 start, find the reachable row ends by exact DP over cursor tuples, extend row by row, then solve all known rows in lockstep. The preferred order is non-empty str > ostr(with a value) > 4-byte number > bool > empty str > absent ostr. Column NAMES are never in the files; any names given elsewhere are INFERRED.

## Tool usage (read-only; CLI is kept stable)
Binary: `analysis\worker2\data_tools\target\release\data_tools.exe`
- `data_tools db-list` : every table with version, marker, rows, bytes (see `db_list.tsv`)
- `data_tools db <table> [nrows] [schema]` : decode. The schema is a comma list of `s,o,b,n,i,f,h` (str, ostr, bool, n32-auto, i32, f32, i16). If omitted, it is inferred. The first line prints `valid=true/false` (structural check on all rows plus EOF).
- `data_tools db-col <table> <col_index> [schema]` : histogram of one column
- `data_tools db-infer [table...]` : infer. With no args it writes `db_schemas.tsv` + `db_examples.txt` for all tables.
- `data_tools ls|hex|cat|grep <pack> ...` : pack access (see data_tools\README.md)

## db_schemas.tsv conventions
Columns: `table, version, rows, bytes, status(ok|FAIL), schema (raw: n32 unclassified), classified (n32 -> i32/f32)`. Type codes as above. Column indices are 0-based in `db-col`.

## Known anomalies
- `unit_special_ability_types` (22 rows declared, 704 bytes) is truncated: its last string runs past EOF. Every row is a single `str`.
- `models_building`, `models_naval`: not inferred (skipped); use the DB_BUILDERS.md layouts.
- **Exe layouts win.** Worker 1 `worker1\DB_BUILDERS.md` gives the authoritative field order. `data_tools db` uses the embedded exe layouts (`db::known_schema`) for unit_stats_land, units, projectiles, factions, building_levels and several campaign tables. Parsing to EOF is NOT sufficient: an empty `str` (00 00) looks like 2 bools, and an absent `ostr` (00) looks like 1 bool.
- `valid=` in `data_tools db` output means structure (all rows to EOF) AND numeric plausibility (ints < 2,000,000 and not byte-shifted). Hash-valued int columns (factions #1, cultures #1) therefore show `valid=false` although the structure is correct.

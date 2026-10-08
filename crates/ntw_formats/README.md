# ntw_formats

Readers for the file formats of *Napoleon: Total War*. The crate uses only the Rust standard library (no dependencies).

The game reads **the player's own installed files**, always read-only. This crate
contains no Creative Assembly data, and nothing in it can write to a `.pack` or to
the install folder. The one writer (ESF) produces bytes in memory.

All numbers in these formats are little-endian (least significant byte first).

## Modules

### `esf`: the binary tree format
Used by `startpos.esf` (campaign start positions), save games, the campaign map
files (`regions.esf`, `pathfinding.esf`, ...) and a few battle-map lists in packs.

An ESF file is a tree:
- **values** are numbers, strings, coordinates and packed arrays (`EsfNode::U32(7)`, `EsfNode::I32Array(..)`, ...);
- **records** are named groups of children with a `version` byte (`EsfRecord`);
- **record arrays** are named lists of items, each item being a list of children (`EsfRecordArray`).

Values have no names, so you find them by position. The record `version` tells you which
layout a record uses, so always check it.

```rust
use ntw_formats::esf::EsfFile;
let esf = EsfFile::open("startpos.esf")?;
let faction = esf.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/WORLD/FACTION_ARRAY[0]/FACTION").unwrap();
let id = faction.get_i32(0);           // first child is the faction's object id
let bytes = esf.to_bytes()?;           // identical to the original file
```

- The reader is **lossless**: `EsfFile::from_bytes(b)?.to_bytes()? == b`. This is verified on
  every shipped `.esf`, all 8 local saves and the 440 ESF entries inside packs.
- Bad input returns an `EsfError` with the byte offset. The reader never panics. Every absolute
  end offset is checked against its parent block, and nesting depth is capped.
- `Fixed20(i32)` is a campaign-map coordinate: `raw / 2^20` map units (`.to_f32()`).

### `pack`: `.pack` archives and the `Vfs`
A pack is an uncompressed archive: a header, an index of `(size, path)` pairs, then the
files back to back. `PackFile::open` reads only the header and index. `read_entry`
seeks to one file and reads just that file, so a 4 GB pack is never loaded whole.

`Vfs` layers many packs into one view. `Vfs::open_install(data_dir)` mounts them in the
game's order: **boot, release, patch, movie, mod** (alphabetical within a group). When
two packs contain the same path, the later one wins. For example, `local_en_patch.pack`
overrides `local_en.pack`. Paths are case-insensitive and accept `/` or `\`.

```rust
use ntw_formats::pack::Vfs;
let vfs = Vfs::open_install(r"C:\...\Napoleon Total War\data")?;
let units = vfs.read("db/units_tables/units")?;
```

### `db`: binary DB tables
The game's data tables (`db\<name>_tables\<name>` in `data.pack`). Header: an optional
`FC FD FE FF` + u32 version, a u8 flag, a u32 row count, then the rows. The file does **not**
describe its columns, so you pass a `Schema` (`Str`, `OptStr`, `Bool`, `I32`, `F32`, `U16`,
each optionally present only from a given table version). The reader follows the schema
exactly and fails if any bytes are left over. It never guesses, because an empty string
`00 00` looks just like two `false` bools. This crate defines no real table schemas; they
belong in `ntw_data`.

### `loc`: localisation text
`text\*.loc` in the language packs: `FF FE "LOC\0"`, u32 version, u32 count, then
`{key, text, flag}` entries. `Localisation::from_vfs(&vfs)` loads every visible `.loc`
into one key-to-text lookup.

## Tests
- `cargo test -p ntw_formats` runs the unit tests. They use small byte buffers built inside
  the tests, plus tiny packs written to the OS temp folder. Truncation and corruption are
  tested for every format.
- `cargo test -p ntw_formats -- --ignored` runs the integration tests in
  `tests/real_install.rs` against a real install (read-only). Set `NTW_DATA_DIR` to point
  at a different `data` folder.

## Sources
The format specs come from the project research in `analysis/`: ESF from Worker 3's report §2,
packs from Worker 2's `DB_FORMAT_NOTES.md` and `pack.rs`, DB from Worker 1's `DB_BUILDERS.md` §1,
and `.loc` from Worker 3's `DB_CAMPAIGN_TABLES.md` §1.

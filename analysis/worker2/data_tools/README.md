# data_tools (Worker 2), read-only NTW data analysis CLI

Build: `cargo build --release` (std only, no external crates). Binary: `target\release\data_tools.exe`.
It never writes to the game folder. Packs are opened read-only and only the header, the index, and the requested entries are read (seek + read).

| subcommand | purpose |
|---|---|
| `pack-index [pack...]` | write `..\pack_indexes\<pack>.txt` (size, abs offset, path) + `..\pack_summary_rust.txt`; all packs if none given |
| `ls <pack> [substr]` | list entries whose path contains substr (pack = `data`, `data.pack`, or a full path) |
| `hex <pack> <path> [n] [skip]` | hex dump n bytes (decimal) of one entry starting at skip |
| `cat <pack> <path> [n] [skip]` | same, as text |
| `grep <pack> <path-substr> <string>` | find an ASCII or UTF-16LE string in matching entries (skips entries > 64 MB) |
| `db-list` | every DB table: version, marker, rows, bytes |
| `db <table> [nrows] [schema]` | decode a DB table; schema = comma list of `s,o,b,n,i,f,h` (str, optstr, bool, 4-byte auto, i32, f32, i16). If omitted, the embedded exe layout (`db::known_schema`) is used when there is one, otherwise the schema is inferred |
| `db-col <table> <col> [schema]` | value histogram of one column (0-based) |
| `db-infer [table...]` | infer schemas; with no args writes `..\db_schemas.tsv` + `..\db_examples.txt` |
| `loc <pack> <path> [n]` / `loc-find` | stubs (.loc moved to Worker 3) |

Modules: `src/pack.rs` (PFH0 format + reader), `src/db.rs` (DB table format, schema inference, decoding).
The format spec is in the module docs and in `..\DB_FORMAT_NOTES.md`.

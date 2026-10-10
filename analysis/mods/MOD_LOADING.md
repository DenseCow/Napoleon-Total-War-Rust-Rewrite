# Mod loading (original NTW mods + our mods folder)

Where I am (2026-10-10, worker mod-loading2, review round 1 fixes): the precedence graph is traced
and ported whole (§2.1), the AI's raw tables read the merged view with each table's traced key
(§3.1), skipped script lines and flag mistakes warn. Next: §9.

Code: `crates/ntw_formats/src/pack/{vfs.rs,mods.rs}` (layers, priority, plan builder, script parser),
`crates/ntw_formats/src/loc.rs`, `crates/ntw_data/src/{database.rs,record.rs,kv.rs}` (merged tables),
`crates/napoleon/src/config.rs` (flags). Tests: unit tests in `pack/{vfs,mods}.rs`,
`crates/ntw_data/tests/mods.rs` (two `#[ignore]` real-install tests).
Probe: `cargo run -p ntw_formats --example mod_probe -- packs|loose|dirs` (read-only).

## 1. Install facts (from the files, CONFIRMED)
- `user.script.txt` lives in `%APPDATA%\The Creative Assembly\Napoleon\scripts\` (exe strings
  `The Creative Assembly\`, `scripts\`, file-kind table entry 8 `.script.txt` at 0x0145a680). The game
  writes `preferences.script.txt` there as UTF-16LE with BOM; a hand-written `user.script.txt` is often
  UTF-8 with BOM, CRLF, `mod "NTW3/Temp/music.pack";`.
- Shipped packs (all PFH0, no dependencies): boot.pack type 0; battleterrain, buildings, data,
  local_en, rigidmodels, sound, variantmodels, variantmodels2 type 1; local_en_patch type 2; media
  type 4. 1,419 loose files in `data\`, none of them also in a pack. One file per vanilla
  `db\<t>_tables\` folder. `local_en_patch.pack` ships complete `localisation.loc` / `ui.loc`.

## 2. Which copy of a file wins (all CONFIRMED in Napoleon.exe)
Names below are the ones now set in Ghidra.
- **Rule** (`VFS_AddFileToDirectoryNode` 0x0105bd40 → `VFS_ShouldPackOverride` 0x0108ed00): when a
  pack adds a path that is already indexed, the new copy replaces the old one only if
  `type(old pack) < type(new pack)` by **header type** (boot 0, release 1, patch 2, mod 3, movie 4;
  `VFS_GetPackTypeByName` 0x0108e290). Equal types: the copy mounted **first** stays. A pack named by
  a `mod` line keeps its header type (a type-1 pack in a mod line cannot beat `data.pack`).
- Exception: when both packs are nodes of the precedence graph built by `set_pack_file_precedence` /
  `set_pack_file_dependency` (`g_PackPrecedenceGraph` 0x01768250), the graph order decides
  (`VFS_ComparePackPrecedenceGraph` 0x0108ebb0): the **later node wins**, see §2.1.
- **Loose files never beat packs.** `VFS_OpenFileForRead` 0x0106a890 opens the indexed pack copy if
  any pack has the path; only otherwise it tries each working directory with `CreateFileW`. Folder
  listings tag loose files with `g_szLooseFileMarker` (0x01767f58), whose type is -1.
- **Mount order** (only decides equal-type ties):
  1. `VFS_InitMountBootPack` 0x01062600: scan, then the single boot pack (errors: two boot packs,
     boot pack with dependencies).
  2. `VFS_MountReleaseMoviePatchPacks` 0x01084010: release, then movie (bink), then patch packs, each
     in **pack-scan hash-map slot order**; types 0 and 3 are skipped, so mod-type packs in `data\`
     never load on their own.
  3. `VFS_LoadMods` 0x01082240 (from the DB opener 0x00e20760, "Done m_vfs.load_mods"): the names
     from `mod` lines in script order (so the **first `mod` line wins** among type-3 packs); with
     `import_all_mods` (`g_bImportAllMods` 0x01767682, set by 0x010782f0) the mod lines are ignored
     and every type-3 pack of the scan is mounted in slot order.
- **Pack scan** (`VFS_ScanPackFiles` 0x01095100): `*.pack` in each working directory in order, file
  name ASCII-lowercased (0x004f4850), first directory wins per name. The map (`VFS_PackScanMapInsert`
  0x010970d0): key = lowercase file name, hash = djb2 (5381, ×33 + UTF-16 unit, wrapping u32),
  `& 0x7fffffff`, mod capacity (71 at start, `VFS_Construct` 0x01051340), linear probing with wrap;
  only when no slot is free it grows to 2n+1 and re-inserts in old slot order. Iteration is slot
  order. Our `scan_slot_order` reproduces this.
- **Working directories** (`g_pWorkingDirectories` 0x01767d4c): `data\` first;
  `add_working_directory` (0x0105c780 → `VFS_AddWorkingDirectory` 0x0105ca70) appends a folder,
  used as written (relative to the process folder, i.e. the install root). Its packs join the scan
  (release/patch/movie load automatically) and its loose files are searched after `data\`'s.
- **`mod` line** (`Console_CmdMod` 0x01085910 → `VFS_AddModToStartupList` 0x0105c4c0): the name is
  normalized (`/`→`\`, ASCII lowercase, repeated `\` collapsed, 0x00edee40); an absolute `X:\...`
  path adds its folder as a working directory and keeps the file name. `VFS_MountPack` 0x01082690
  looks the bare file name up in the scan, else opens the path relative to the working directories.
- **`exclude_pack_file`** (0x0106bf60 → list at graph+0x2c): an excluded pack is not mounted;
  `VFS_MountPack` returns 2 for it.

### 2.1 The precedence graph (CONFIRMED by static trace, 2026-10-10)
Graph object `g_PackPrecedenceGraph` 0x01768250: +0x00 vector of pairs `{lhs, rhs}` (both
commands), +0x10 dependency pairs, +0x20 excluded packs, +0x30 node order (vectors: count +8,
data +0xc).
- `set_pack_file_precedence lhs rhs` (`Console_CmdSetPackFilePrecedence` 0x0109aa10) and
  `set_pack_file_dependency lhs rhs` (`Console_CmdSetPackFileDependency` 0x0109a910 →
  `VFS_SetPackDependencyPair` 0x01064d00) both call `VFS_AddPackPrecedencePair` 0x0108e8f0 with
  pair `{lhs, rhs}` (0x0104c700 stores arg1 at +0, arg2 at +0x10). The dependency command also
  stores the pair in the dependency list when the add succeeded.
- Add: the pair list plus the new pair is sorted (`VFS_SortPackPrecedencePairsTopologically`
  0x0105e8e0). The sort repeatedly takes the first pair (list order) whose lhs is no pair's rhs,
  emits that lhs, removes all its pairs and remembers their rhs (first-seen order); when no pair
  is left the remembered nodes follow. A cycle gives an empty result: the pair is **dropped**
  (return 0, ignored by the console command) and the old order kept. Otherwise the pair is stored
  and the node order replaced.
- ORIGINAL BUG (fixed): adding a pair already in the list erases it (0x0108ea19 → erase
  0x0106bbb0, for bAdd=1 too), without rebuilding the order, so a repeated line cancels the pair
  at the next rebuild. Ours keeps it.
- Compare (`VFS_ShouldPackOverride(old, new)` 0x0108ed00 → 0x0108ebb0): both packs must be nodes
  (else 2 → header types). The list `[old, new]` is sorted by
  `VFS_SortPacksByPrecedenceWithDependencies` 0x0109ecf0 (dependency closure, then node order),
  and the first entry equal to old answers 1 (= new replaces old), equal to new 0. So **the later
  node wins, rhs beats lhs**, for any two nodes (no pair between them needed), and a node pack
  replaces itself (`[p, p]` → 1): two copies of one path in one node pack, or two DB files of one
  node pack, the later one wins without the `bob_` rule. Caller arg order checked at 0x0105be6d
  (holder copy, new pack) and in the DB loader (vtable +0x68 with (holder, new)).
- The help text says "lhs will be searched before rhs for files"; the code makes rhs win. Kept 1:1
  (mods are tuned to the exe), not an ORIGINAL BUG: for `set_pack_file_dependency` rhs winning is
  the natural reading ("lhs ... used by rhs").
- `VFS_MountPack` 0x01082690 sorts `[name]` with 0x0109ecf0: the dependency closure (each
  dependency pair whose rhs is listed adds its lhs; restart) in node order, the rest after; it
  mounts every entry but the last first. A needed pack that is excluded makes 0x0109ecf0 return 0
  and `VFS_MountPack` return 6 → "Could not load the mods." (ORIGINAL BUG, fixed: that pack is
  skipped with a warning).
- Names: the script words are stored raw and compared case-sensitively (0x004f3410) with the
  pack's name; ours matches them like `mod` names (lowercase file name). INFERRED until the pack
  object's name (vfunc +8 in 0x0105bd40) is traced (BACKLOG trace line).
- Code: `pack/precedence.rs` (`PackGraph`), `Vfs::beats` takes each pack layer's node position at
  mount time; `ModResolver::add` uses `PackGraph::mount_order`.

## 3. DB tables (CONFIRMED, `DB_LoadTableFolderMergedByKey` 0x00e778a0)
- Every file of `db\<table>_tables\` is read, in `VFS_ListFolderFiles` 0x01042c80 order: first the
  **loose** files found in the working directories (0x010430c0, first directory wins per name), then
  the pack files of the folder in **first-mount order of each name** (one copy per name: the winner).
- Rows are keyed; on a key already loaded: the row from the higher-priority file wins
  (`VFS_IsHigherPriorityPack` 0x0108eda0 = §2 rule; loose = -1). **Equal priority: the new row
  replaces the old one only if the new file's name starts with `bob_`** (`wszDbBobFilePrefix`
  0x0139a5f4, case-sensitive compare 0x004f1f30 at 0x00e77c55); otherwise the first row stays.
- So: a mod pack (type 3) beats vanilla rows; among mods the first `mod` line wins; a same-named
  file replaces the whole table; a differently named file adds rows and overrides by key.
- Not ported (BACKLOG): the exe emits the final rows in key hash-map slot order (hash 0x0045c1a0,
  seed 0xf73a51c9), and dedupes keys inside one file; we keep file order and keep in-file duplicates
  (PROVISIONAL in `record.rs`), because each table's key reader still needs checking.

### 3.1 Merge keys of the AI's raw tables (CONFIRMED, 2026-10-10)
Each table loader is its own template instance of 0x00e778a0 (same `bob_` literal and vtable +0x68
priority calls). Its row reader returns the record; what is hashed:
| table | loader | reader | key |
|---|---|---|---|
| campaign_ai_managers | 0xdbc9c0 | 0xdd2220 | col 0 (record+0) |
| campaign_ai_personalities | 0xdc7db0 | 0xdd29b0 | col 0 |
| building_chains | 0xe4c220 | 0xe550d0 | col 0 |
| campaign_ai_personality_junctions, campaign_ai_manager_behaviour_junctions | 0xdc6b50 | 0xdd22a0 | `col0;col1` built by the loader (0x00dc74cd, `;` at 0x013305f8) |
| cdir_unit_qualities | 0xfc6570 | 0xfc7970 | `col0_col1_col2` built by the reader into record+0 |
| cdir_unit_balances | 0xfca150 | 0xfcb550 | `col0_<u32 col1>_<u32 col2>_col3` at record+0x2c (u32 decimal writer 0x004f2fc0) |
Code: `ntw_ai::tables::keys`, read through `ntw_data::load_merged_rows` (same files, order and row
rule as every typed table). Vanilla has one file per table, so the rows are unchanged there.

### 3.2 One table path: keys of the 35 generically read tables (CONFIRMED, 2026-10-10)
Every table is read through one reader, `ntw_formats::db_folder` (`read_files` = the §3 file list,
`merge_keyed` = the row rule; `ntw_data::load_table` / `load_merged_rows` and the `RawTable`s all
call it). No other code names a `db\..._tables` path: `crates/napoleon/tests/table_path_guard.rs`
fails if game code does (examples and integration tests, which probe the shipped files, are not
scanned). Before this, ~35 readers (UI scripts, mounts, uniforms, trees, names, battle buildings,
model viewer) read only `db\<t>_tables\<t>` and missed additive and `bob_` files.

How the keys were found: the folder name is passed with a row-reader thunk by a small constructor
that calls the table's loader instance (e.g. 0x00dd32f0 pushes `building_description_texts_tables`
and thunk 0x00de2600 → reader 0x00dd2220). The loader calls the reader (`CALL [ESP+..]`), which
returns the record; the loader hashes record+0 (0x0045c1a0) unless it builds the key itself (string
concatenation 0x004f1200 = copy this + append arg). Readers that fill record+0 themselves build it
the same way. Table (loader / row reader / key), `RowKey` in `db_folder::tables`:
| table | loader | reader | key |
|---|---|---|---|
| mount_variants | 0xdc3590 | 0xe54560 | col0+col1 (reader builds record+0) |
| warscape_animated, warscape_rigid | 0x11ba0f0 | 0x11c46a0 | col0 (record+0 read by 0x011e7770, text taken by 0x004f5a00) |
| warscape_animated_lod | 0x11b8ed0 | 0x11c43e0 | `norm(col1)_%f(col2)_col3` built by the reader in place of col0 |
| warscape_rigid_lod | 0x11be9c0 | 0x11c4950 | `norm(col1)_col2_col3` (`%S` 0x0131411c) |
| uniforms | 0xfc05b0 | 0xdd2ce0 | col1+col3 (loader, record+0xC + record+0x24) |
| uniform_to_faction_colours | 0xfc2060 | 0xfc3480 | col0+col1 (loader) |
| faction_uniform_colours | 0xfc3ce0 | 0xfc50e0 | col0 |
| warscape_trees | 0xdcb370 | 0xf08a80 | col0+col1 (reader) |
| ship_names | 0xdcb370 | 0xf06c50 | col0 |
| names | 0xf2f5f0 | 0xf3b650 | col6, the id (loader hashes record+0x38) |
| models_building | 0xdc23c0 | 0xdd2660 | col0 |
| battlefield_buildings | 0xe3c620 | 0xe54010 | col0 |
| unit_movement_modifiers | 0xe766d0 | 0xe85aa0 | col0 |
| battle_type_setup_limits | 0xe48c20 | 0xe54260 | col0+col1+col2+col3 (loader) |
| battle_type_faction_presets | 0xe47880 | 0xe54f10 | col2 as `%d` (0x0131530c, formatter 0x004f1460) |
| battle_type_unit_to_faction_presets | 0xe49e80 | 0xe54f60 | col0 |
| wind_levels | 0xe7f690 | 0xe865b0 | col0 |
| battles_to_battle_sky_types_junctions | 0xfb2270 | 0xfb3670 | col0+col1 (reader) |
| battle_sky_types | 0xe43100 | 0xe54920 | col0 |
| battle_types | 0xdbc9c0 | 0xdd2220 | col0 |
| battles | 0xe41f30 | 0xe54690 | col0 |
| ministerial_positions_by_gov_types | 0xf2add0 | 0xf3b2e0 | col0+col1+col2+col3 (loader; the reader stores col0 at +0xC, col1 at +0) |
| ministerial_positions | 0xdc7db0 | 0xdf0990 | col0 |
| state_gift_values, diplomatic_relations_attitudes | 0xdc7db0 | 0xf08360 | col0 |
| religions, cultures | 0xf360f0 | 0xf3fbb0, 0xf705e0 | col0 |
| building_culture_variants | 0xdbb7a0 | 0xdd1ff0 | col0+col1 (loader) |
| technology_required_technology_junctions | 0xdc11a0 | 0xdd2620 | col0+col1 (loader) |
| warscape_equipment_themes | 0x11bc5d0 | 0x11c4700 | col0 |
| warscape_equipment_items | 0x11bb310 | 0xdd2620 | `col0_col1` (loader, 0x011bbc93) |
| battle_personalities | 0xe40d60 | 0xe545f0 | col0 |
| battle_entities | 0xe3fb90 | 0xe54370 | col0 |
| public_order_factors, town_wealth_growth_factors | 0xdc5980 | 0xf3f8a0, 0xe53fb0 | col0 |
`norm` is the VFS path normalisation the lod readers call (VFS vfunc +0xB8 = 0x01065830 over
0x00EDEE40): `/`→`\`, ASCII lower case, a doubled `\` dropped from the third character on, then a
leading `data\` and a leading `\` cut (it also cuts a working-directory prefix, which only an absolute
path has). Real-install check (`merged_raw_tables_equal_the_single_file_read`, ntw_formats
`real_install`): each of the 35 tables has one vanilla file and the merged read equals the old
single-file read. `Vfs::table_files` now lists a `db\` folder from an index built at mount time
instead of scanning the ~87k-path index per table.

## 4. Text (.loc) (CONFIRMED)
The exe opens only `text/localisation.loc` (0x013a85c0, opened at 0x00e20760) and `text/ui.loc`
(0x01395cb8), by fixed path through the VFS: the §2 winner replaces the whole file; other `.loc` names
are ignored. Our extension: extra `text\*.loc` files in the `mods\` folder are merged on top.

## 5. Script parsing
- CONFIRMED: commands run through `Console_ExecuteCommandStream` 0x00db80f0 (the only user of the
  command map 0x0163c228): skip whitespace, read a word, look it up, call the handler. Arguments are
  read by 0x010f5d10: `"` toggles quoting anywhere in a token; whitespace = space, tab, CR, LF, U+00A0,
  U+200B, U+FFFE, U+2028, U+2029 (table at 0x01466b54).
- ORIGINAL BUG (fixed): the runner stops at the first unknown command, so one comment or typo line
  silently drops every later `mod` line (0x00db80f0). We skip just that statement with a warning (every
  statement we do not run warns, since the original may run it; a loading command without its
  arguments says what it needs).
- ORIGINAL BUG (fixed): a `mod` line naming a missing, unreadable or excluded pack makes
  `VFS_MountPack` return >1 and `VFS_LoadMods` throws "Could not load the mods." (0x01082240), which
  stops start-up. We skip that pack with a warning naming it.
- INFERRED: `;` ends a statement (debugger read: §6 item 1). The exe's tokenizer has no `;` rule (0x010f5d10, 0x010d34e0 stop
  char 0x0182d02c is 0), yet mods ship `mod x.pack;` lines. To settle: launch the original with a
  `user.script.txt` holding `mod test.pack;`, break at 0x0105c4c0 and read the wide string of its
  argument.
- INFERRED: encodings: UTF-16LE/BE by BOM or zero second byte, else UTF-8 (BOM optional).

## 6. Open (INFERRED / PROVISIONAL): unattended debugger run
All six happen during start-up, so one launch settles them with no player input. Translate every
address with `debugger_static_to_dynamic`; stop the game once the main menu is up.

**Setup (nothing written into the install):**
- Make `%TEMP%\nr_mod_probe\` with type-3 packs built by our test helper:
  - `a.pack` and `b.pack`, both holding `text\nr_probe.txt`;
  - `c.pack` (a different file);
  - `d.pack`, whose header lists `c.pack` as a dependency.
- Back up the user's `%APPDATA%\The Creative Assembly\Napoleon\scripts\user.script.txt` if it exists,
  write the test script below, and restore the backup afterwards:
  ```
  add_working_directory <TEMP>\nr_mod_probe\
  set_pack_file_precedence b.pack a.pack
  mod <TEMP>\nr_mod_probe\a.pack;
  mod "<TEMP>\nr_mod_probe\b.pack";
  mod d.pack;
  ```

String objects: a 12-byte string is {u32 length, u32, u16* text}. Read the text from the pointer at +8.

1. **`;` terminator** (§5): break at entry of 0x0105C4C0 (`VFS_AddModToStartupList`). The name is
   passed by value at [ESP+4], text pointer at [ESP+0xC]; log that wide string.
   - Expected: 3 hits.
   - If a logged name ends in `;`, the exe keeps it: tag `;` as a word character, so `mod x.pack;`
     fails in the original.
   - If not, CONFIRMED: `;` is dropped.
2. **Script before the scan**: log the hit order of 0x0105CA70 entry ([ESP+4] points to a 12-byte
   string; log its text) and 0x01095100 entry.
   - Expected: `data\`, then `nr_mod_probe\`, before the first 0x01095100 hit.
   - If 0x01095100 comes first, folders added by the script are not scanned: drop that from `plan_layers`.
3. **Precedence direction**: settled by static trace (§2.1: rhs wins). Optional cross-check: break
   at 0x0108ED17 (after the call to 0x0108EBB0 in `VFS_ShouldPackOverride`), log EAX and the two
   names (args at [ESP+4] old, [ESP+8] new) for `text\nr_probe.txt`. Expected with
   `set_pack_file_precedence b.pack a.pack`: old = a.pack, new = b.pack, EAX = 0 (a, the rhs, stays).
4. **Header dependencies**: log every entry of 0x01082690 (`VFS_MountPack`; [ESP+4] points to the
   name string).
   - Expected: `c.pack` mounted just before `d.pack`, with no `mod c.pack` line. That CONFIRMS that
     header dependencies are mounted first (the `set_pack_file_dependency` closure is CONFIRMED
     statically, §2.1).
5. **DB row order** (§3): arm a breakpoint at 0x00E786F7 only while the units loader runs, i.e.
   between the entry of 0x00E86AA0 and its return at 0x00E86AC9. At each hit, ECX is the key-map
   entry: log its key (12-byte string at ECX) and the row index [ECX+0xC].
   - Expected: one hit per units row, in emission order.
   - Compare with the file's row order. If it differs, the order is hash order (the PROVISIONAL
     today): port it, hash 0x0045C1A0, seed 0xF73A51C9.
6. **In-file duplicate keys** (§3): count hits of 0x00E782E3 (the "drop the new row" path) per
   table, noting the table's folder name string at the entry of 0x00E778A0.
   - Vanilla has one file per table, so any hit is an in-file duplicate that the exe drops and we
     keep. That table then needs the dedupe (and its key reader checked).
   - No hits on vanilla: the keep-all is equivalent on vanilla; mark it CONFIRMED-equivalent and
     dedupe anyway for mods.

## 7. Our mods folder (ours, not in the original)
`<exe dir>\mods\` holds packs and loose folders; `load_order.txt` lists them, first line on top
(without it, every entry in name order). These layers rank above every original layer; among
themselves the first listed wins. Flags: `--mods <dir>`, `--no-mods` (no user script, no mods
folder), `--user-script <file>`, `--list-mods` (prints the report and exits). A
`--mods` or `--user-script` without its value warns and uses the default.

## 8. How the code maps to this
- `Vfs::beats` is the one priority rule (§2): graph node order when both packs are nodes (§2.1),
  then `Layer::rank` (header type;
  loose -1; our `mods\` folder `i32::MAX`); equal ranks keep the first mounted. The index applies it
  at mount time; `Vfs::db_row_replaces` applies it plus the `bob_` rule to DB rows (§3).
- `plan_layers` (mods.rs) is the one place the mount order lives; `scan_slot_order` is the pack-scan
  map order. `Vfs::table_files` is `VFS_ListFolderFiles`; `ntw_formats::db_folder` is the one table
  reader (§3.2): its `merge_keyed` merges rows for `Table::merged`, `KvTable::merged`,
  `load_merged_rows` (the AI's raw tables, §3.1) and the `RawTable`s.
- Every reader goes through `Vfs::open_install` (a plan cached per data folder, language and mod
  setting); the campaign supertexture tiles (`campaign/detail.rs`) now read through it too.
- Real-install check (`cargo test -p ntw_data --test mods -- --ignored`): with no mods all 86,977
  pack paths come from the same pack as before and the database and text are identical, so the
  hash order changes nothing on vanilla (no two vanilla packs of one type share a path).

## 9. Next
- Run the §6 unattended debugger plan (start-up only).
- DB row order and in-file duplicate keys (§3 last bullet).
- Test with real mods (BACKLOG §11 item 4).
- Trace the pack object's name (vfunc +8 in `VFS_AddFileToDirectoryNode` 0x0105bd40) to settle how
  `set_pack_file_*` words match pack names (§2.1, INFERRED).
- `VFS_MountPack` applies the dependency closure to every mount, also boot/release/patch/movie
  packs; ours applies it to `mod` lines only (depends on §6 item 2, script before scan).
- In-file duplicate keys: in a graph-node pack the later row replaces (§2.1), not just in `bob_`
  files (with the §3 dedupe item).

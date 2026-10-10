# NapoleonRust's own campaign save format

Code: `crates/ntw_campaign/src/own_save.rs` (+ `own_save/tests.rs`), the seam that opens it
`crates/ntw_campaign/src/source.rs`, the writer call `crates/napoleon/src/campaign/play.rs`
`quick_save` (F5). Design: DESIGN.md §3.5.1; why: MODDING_AUDIT.md §3.2 (saving used to re-parse
and patch the start position's ESF tree, so a campaign with no ESF behind it could not be saved).

Save compatibility with the original is out of scope (user decision): the original cannot read
these saves, and we do not write the original's `.save` from the game. `ntw_campaign::save` (the
ESF writer) stays for tools and tests and is the starting point of the later exporter. The
original's `.save` files still **load** (read-only, through the importer).

## Layout

All integers little-endian.

| Bytes | Content |
|---|---|
| 16 | magic `NAPOLEONRUST\0SAV` |
| 4 | format version (`FORMAT_VERSION`, now 1) |
| 4 | `n`: byte length of the header section |
| n | header: zlib of the RON text of `CampaignInfo` |
| rest | body: zlib of the RON text of `SaveData` |

- The header comes first and alone so the Load Game page reads it without the body
  (`own_save::read_info`); the body has no length field, it is the rest of the file.
- RON (Rusty Object Notation) is text that names every field; zlib at `Compression::fast()`.
- Detection is by the magic (`own_save::is_own_save`), not the file name: `ntw_campaign::read` and
  `read_info` route our saves to `own_save`, everything else (start positions, the original's saves)
  to the ESF importer. The file name stays `.save` (F5 writes `quick_save.save`).

## What is stored

**Header (`CampaignInfo`)**: kind (save), timestamp, build id `NapoleonRust` and our version, the
`SaveHeader` (human faction key, portrait, turn number, year, season name, flag, date, and one
territory picture per theatre rendered for the human's regions), the campaign key, the map key and
the player setup. `own_save::save_header` builds it from the loaded campaign's facts plus the
model's turn and date.

**Body (`SaveData`)**: the human faction key; the whole `CampaignModel`; the rebel faction id; the
scripts' `save_value` slots in order (what `load_value` gives back); the scripts' restricted units
(the restricted building levels are in the model).

**Not stored** (fields marked `serde(skip)` in `ntw_sim`), and why:

| Field | Why |
|---|---|
| `CampaignModel::rules` | game data, rebuilt from the DB on load (`attach_rules`) |
| `CampaignModel::terrain` | the map's movement grid, rebuilt from the campaign's map on load |
| `last_autoresolve` | the last battle report, not state |
| `script_rngs` | the trait / ancillary script RNGs; the original does not save them either |
| `negotiations` | the open negotiation slot; not saved by the original |
| `World::recruitment_sources` | the ESF writer's bookkeeping, meaningless without an ESF tree |
| `World::agents_acted` | agents' used actions this turn; UNKNOWN whether the original saves it |
| `World::sabotaged` | CONFIRMED not saved: the army loader `0x00870FD0` clears it |
| `World::network_sight` | rebuilt at each turn start |
| `CharacterDetails::wounded` | the duel wound (character +0x512), not saved by the original |

The map is not in the save: the header names the campaign, and `source::open(Start::Save)` asks that
campaign's source for its map (movement grid, region links, trade nodes, display data). A save whose
campaign no source has is `SourceError::NotFound(<campaign key>)`.

**Territory pictures.** A theatre's picture is rendered only from a map picture and a lookup picture
of the same size with complete pixel data (`header_map::TheatrePictures::new`, as the old ESF header
writer required). A theatre whose pictures are missing, unreadable or do not fit (a map mod with a
smaller `<x>_lookup.tga`) is logged once when the campaign opens and has no picture in the header;
saving never reads past a picture.

**Names of new characters and unit officers.** The ESF writer used to name the characters and
unit officers created in play at save time (SAVE_COMPAT.md §21), so an own save would have lost
them. The original names a character when it creates him (the naming routine `0x009940A0` is called
by the spawners, §21), and now so does the model, end to end (`ntw_sim::campaign::names`, the same
rule): character names are in `CharacterDetails`, unit officer names in `CampaignUnit::officer_name`
(imported from `COMMANDER_DETAILS`), the faction's name decks in `World::name_allocators` (imported
from `NAME_ALLOCATION_DETAILS`; every draw advances them), the pools in `CampaignRules::names`
(rebuilt on load from the `names` table and localisation, `ntw_campaign::names::attach`). A colonel,
captain or promoted general made from a unit carries its officer's name unless it is a historical
character's, then draws from the decks (CONFIRMED in the exe: colonel `0x008B7EF0`, captain `0x008B7F60`,
land promotion `0x008E1C20` through `0x008B7EF0`, naval promotion `0x008E2260` pass unit +0x7c to
the naming routine `0x009940A0`). A unit raised with its character
carries his name (INFERRED from the original's saves: colonels and their units, generals and their
bodyguards, 39 of 39 in `auto_save`); anyone else draws from the decks. Names and decks are
saved with the model, and the ESF writer only writes what the model holds.

## No caps on counts

Every collection (factions, regions, characters, forces, units, buildings, queues, script slots,
restricted units, territory pictures...) is a RON sequence or map written as long as it is; ids are
the model's own 32-bit newtypes as they are (the original's id widths: `FactionId`, `CharacterId`,
`UnitId` i32; `RegionId`, `ForceId`, `FortId` u32), written as numbers, not packed.
The one fixed-width field is the header length (u32: a header up to 4 GiB); a larger one is a
write error (`FormatError::Encode`), never a truncated file.

## Versions and migration

- A field added to the model with `#[serde(default)]` loads from an older save with no version
  change (RON names fields, so a missing field takes its default).
- A change an older save cannot express that way (a renamed or reshaped field) bumps
  `FORMAT_VERSION` and adds a conversion step in `own_save::sections` / `read` that rewrites the
  older text before decoding. Version 1 is the first, so no step exists yet.
- A save from a newer version is `FormatError::Newer { found }`; truncated or damaged bytes are
  `Truncated` / `Decode` errors. None of them panics (`own_save/tests.rs::bad_saves_are_errors`).

## Measured (release build, eur_napoleon after 3 turns as France)

From `cargo test -p ntw_campaign --release --test own_save_install -- --nocapture` (2026-10-10):
1,447,153 bytes; written in 41 ms; opened with its map (map files, movement grid) in 617 ms, of
which the body decodes in 118 ms and the header alone in 20 ms (the territory pictures dominate it).

## Tests

- `own_save/tests.rs`: a made-up campaign that never was an ESF file round-trips field for field;
  the not-saved fields are not saved; bad bytes are errors.
- `tests/own_save_install.rs` (real install, skips without it): a new eur_napoleon opened through
  the seam, played 3 turns, saved and opened again gives the same model (minus the not-saved
  fields), the same rules and movement grid, header, script slots and restricted units, and a
  re-save is byte-identical; the original campaigns are found through the seam; an unknown
  campaign is `NotFound`; the importer's map display data equals the map files.
- `source.rs` tests: a save of a campaign no source has is `NotFound`, garbage is a load error.

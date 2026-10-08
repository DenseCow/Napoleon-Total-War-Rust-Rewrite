# S1_MISSIONS_UI: mission records and UI layout leftovers (backlog §1)

Worker: the AI slot, moved to §1 on 2026-10-03 (branch `work/s1-missions-ui`, worktree
`%USERPROFILE%\Documents\NR-ai6`, Ghidra copy `%USERPROFILE%\Documents\NR-ai-ghidra` via
`analysis/ai/run_ghidra.ps1`). Tags: **CONFIRMED** (read from the exe or a file), **INFERRED**,
**UNKNOWN**. No decompiled code here, only specs in our own words.

## Where I am / what's next
- **READY TO MERGE (2026-10-03).** origin/main merged in (main's own NaN-safe save diff kept;
  the mission test skips NapoleonRust-written saves like main's tests). `cargo build --workspace`,
  `cargo test --workspace` (0 failures) and `cargo clippy --workspace --all-targets` (no new
  warnings) pass; the game runs (main menu, options, `--campaign eur_napoleon`) without panics.
  Optional follow-ups: the OPEN items below. The AI work is paused separately on `work/ai6`
  (AI_RESEARCH.md §0). Temporary files go in `target/tmp`.
- DONE: (a) **mission records**: the full ESF layout from the exe's writer and reader (below),
  reader + writer `ntw_campaign::missions` with round-trip tests; the user's newest
  `auto_save.save` turned out to hold **one real mission**, which reads with the decoded layout
  and writes back identically (`tests/real_install.rs::mission_managers_read_and_write_back`).
- DONE: (b) **UI layouts**: state +0x60/+0x64 = TextXOffset/TextYOffset (CONFIRMED) and the
  exe's text layout and placement rules (CONFIRMED), applied in the front-end renderer, in
  `SetStateText`'s measure and in `Get/SetStateTextDetails` + `SetStateTextXOffset`; align and
  HBehaviour value names CONFIRMED; the +0xF0 pair is editor-only (it was wrongly added to text
  positions); DrawMode's use CONFIRMED (three draw variants), its visual effect still UNKNOWN.
  Checked in the running game (main menu, options, Napoleon's Battles / Arcole) against a build of
  main: same layout, labels 1-2 px lower where TextYOffset is set.
- SOLVED 2026-10-04 (UI_LAYOUT_FORMAT.md): DrawMode 0 scaled, 1 unscaled 1:1, 2 full screen. OPEN: 
  state +0xD0/+0xD4 (no run-time reader found); `text_behaviour.1`;
  (the re-linking of saved mission targets: SOLVED 2026-10-04, see (a)).
- Missions are loaded into the model (`World::missions`, targets re-linked; 2026-10-04). PROVISIONAL: no rule runs
  them (`ntw_script` records `trigger_custom_mission` calls) and the save writer keeps the original record untouched.
- Tooling: `AiDecomp.java` gained `mkfn:0xADDR` (creates the missing function around an address in
  memory, then decompiles it; used for the `trigger_custom_mission` handler, which Ghidra had not
  made a function). Use `target/tmp` for output: the session scratchpad is
  shared with other workers.

## (a) Mission records (CONFIRMED unless tagged)
ESF writer primitives used to read the writers: a record opens with `0x80`, name index u16,
version u8, size u32 (`0x00DE77D0`); a record array with `0x81`, name, version, size, count
(`0x00DE79F0`), each item with a size (`0x00DE7950`); a value is its type byte then its bytes
(`0x00DEB4D0` + `0x00DEB3C0`): 1 bool, 4 i32, 8 u32, 0x0E utf16 (u16 length + 2 bytes per char,
`0x00DEB510`), 0x0F ascii (u16 length + bytes, `0x00DEB4E0`), 0x48 u32 array (`0x00444C70`).
The record versions come from tiny getters (`0x0047AF90` = 1, `0x004CDC40` = 2, `0x004CDC50` = 3).

| record | writer / reader | layout |
|---|---|---|
| `CAMPAIGN_MISSION_MANAGER` v1 | `0x0099F9F0` (called by the `FACTION` writer `0x00892A80`) / `0x0098A480` | bool (owner `+0x16C`, UNKNOWN; false everywhere), `MISSIONS[]` v0: one `CAMPAIGN_MISSION` per item |
| `CAMPAIGN_MISSION` v2 | `0x0099F820` / `0x00989CF0` | u32 `+0xDC`, u32 `+0xE0`, i32 + i32 (`+0xE8/+0xEC`, written by `0x00910CF0`), bool `+0xF0`, bool `+0x100`, ascii `+0xF4`, utf16 `+0xC4` (read only when version > 1), then `{OBJECTIVES}`, `{LOCALISATION_OVERRIDES}`, `{REWARDS}` (the reader finds those by name) |
| `CAMPAIGN_MISSION_OBJECTIVES` v3 | `0x0099EF40` / `0x0098AA30` | u32 kind, u32 settlement, u32 fort, u32 faction, utf16 `building_levels` key, utf16 `units` key, [v2+: u32 port, utf16 `technologies` key, u32 character], [v3: u32 region], u32[] regions |
| `CAMPAIGN_MISSION_LOCALISATION_OVERRIDES` v1 | `0x0099EE40` / `0x00989FD0` | utf16 heading, utf16 description, utf16 reward text |
| `CAMPAIGN_MISSION_REWARDS` v3 | `0x0099F310` / `0x0098B310` | u32 money, u32 takeover faction, `GRANT_UNIT_DATA[]` v0 {utf16 unit key, u32, u32}, `GRANT_AGENT_DATA[]` v0 {utf16 agent type, u32 region}, u32 army experience, u32 navy experience, utf16 enable-recruitment unit key. v2 has one inline {utf16, u32, u32} unit grant instead of the arrays; v1 none |

Field meanings:
- **Kind** (objective u32 #0), from `trigger_custom_mission`'s handler `0x00977DF0` (type string →
  builder `0x00A21xxx` → objective setter `0x0098xxxx`) and the default texts of `0x00A20CD0`:
  0 capture_city (settlement), 1 protectorate_region_capture (the u32[] of regions), 2
  build_building (building level), 3 recruit_unit (unit), 4 forge_alliance (faction), 5
  capture_fort (fort), 6 make_peace (faction), 7 blockade_port (port), 8 infiltrate_garrison
  (settlement, fort and port of the garrison's holder), 9 research_technology (technology), 10
  assassinate_character (character), 11 make_trade_agreement (faction), 12 end_rebellion, 13
  restore_public_order, 14 liberate_region (region), 15 sabotage_enemy_building (none); an
  objective starts at 16 before a setter runs.
- **The u32 targets are object ids** (in memory the setters store the looked-up objects; the ids are
  pointer-like: the sample's settlement is `0x237E9AA8`). **Re-linking (CONFIRMED 2026-10-04, pathfinding
  worker):** after a load, the campaign post-load pass `0x0095FE80` runs the faction fix-up `0x008E07F0` for every
  faction; when the faction has a mission manager (+0x6F4) it calls `0x00A18670`, which for each of the manager's
  missions (+0x160 count) runs `0x00A18650`: `0x00A186A0` maps the objective's settlement (+0x14), fort (+0x18),
  faction (+0x1C), the region list (+0xC/+0x10), port (+0x28), character (+0x30) and region (+0x34), and
  `0x00A18720` the rewards' takeover faction (+0x4), both values of every `grant_unit` item (+0xC, +0x10 of 0x14-byte
  items) and the region of every `grant_agent` item (+0xC of 0x10-byte items), all through the global id → object
  map `0x0105AC60` (an open-addressing hash of {id, object}; the same map re-links the obstacles' characters and the
  relationship war ally); then `0x00A20CD0` rebuilds the texts. The ids are the objects' saved ids: a settlement's is
  `SETTLEMENT` #4 (= its `SIEGEABLE_GARRISON_RESIDENCE` #1), a region's `REGION` #4, a faction's and a character's
  their record ids (CONFIRMED form; the sample save that would show the settlement match is no longer in the user's
  folder). Ported: `ntw_campaign::missions::to_model` + `settlement_regions`, called by the world loader; the model
  keeps them in `World::missions` (`ntw_sim::campaign::details::CampaignMission`, targets `MissionTarget::{Found,
  Unresolved}`; forts and ports stay raw ids). Test: `tests/missions_install.rs` (a mission with real eur ids put
  into France's record loads with every target found). No rule runs the missions yet (PROVISIONAL).
- `+0xDC`: the ctor's int argument from the script (INFERRED the turn limit; 0 in the sample).
  `+0xE0`: 0 at creation (INFERRED turns elapsed; 3 in the sample). The two i32 and the bool
  `+0xF0`: the target's position and its "set" flag (CONFIRMED pair, `0x00A22280`; units
  UNKNOWN). `+0x100`: 0 at creation, UNKNOWN. ascii `+0xF4`: **the script's mission key**
  (CONFIRMED, sample `eur_take_vienna`). utf16 `+0xC4`: the objective label (CONFIRMED, sample
  "Capture city:").
- **Rewards** come from the script's reward string, parsed by `0x00A16870` (comma separated):
  `money:<n>`, `takeover_faction:<faction>`, `grant_unit:<unit>#<settlement>` (two u32 from
  `0x00A04C70`, UNKNOWN), `grant_experience_army:<n>`, `grant_experience_navy:<n>`,
  `enable_recruitment:<unit>`, `grant_agent:<type>#<region>`.
- **Texts**: empty override strings make the game use per-kind defaults
  (`mission_activities_description_*` / `mission_text_text_main_mission_*_text` / `tut_*`), and
  the reward text is built from the rewards (`0x00A20CD0`).
- Sample (user's `auto_save.save`, France): `eur_take_vienna`, capture_city, heading
  `mission_text_text_eur_france_capture_vienna_heading`, 2000 money; 35 managers in the 16 files.

Code: `crates/ntw_campaign/src/missions.rs` (`read_manager` / `write_manager`,
`read_mission` / `write_mission`, `find_managers`, `MissionKind`), tests in the module (round
trip through records and ESF bytes, the written layout, older versions, bad input) and
`tests/real_install.rs`.

## (b) UI layouts
Full spec in `analysis/frontend/UI_LAYOUT_FORMAT.md`, "Update (s1-missions-ui ...)". Summary:
- The state class: the loader callback `0x00DA9510` builds a 0x144-byte state with vtable
  `0x01393C44` (7 slots; slot 1 `0x01033320` re-docks the image metrics on resize) over the base
  read by `0x01021410`; the text fields sit at +0x58 VAlign, +0x5C HAlign, +0x60 TextXOffset,
  +0x64 TextYOffset, +0x68/+0x6C the layout box, +0x70/+0x74 DisplayWidth/Height, +0x78 the line
  list (0x1C bytes per line: width +0x10, y +0x14), +0x88 never-split flag, +0x8C "laid out",
  +0x90 HBehaviour, +0xBC font, +0xC0 leading, +0xC4 tracking, +0xC8 colour (CONFIRMED from the
  reader and `0x010258A0` / `0x01028DB0` / `0x01035CE0` / `0x01036B20`).
- TextXOffset / TextYOffset (CONFIRMED): names from `0x0102B480` and the Lua text-details calls;
  effect from the layout and draw (see the spec). Values in the data: X 0..28, Y 0..50 on text
  states (the one Y = 1239772 is a stray value in one layout).
- Align and HBehaviour (CONFIRMED): name tables `0x01464558` (`top bottom left right centre`) and
  `0x01464570` (`SplitByCharacter SplitByWord NeverSplit`); `SetStateTextDetails` takes names,
  `GetStateTextDetails` returns the numbers.
- +0xF0 (`editor_pos`): not read by the text code; MSVC fill values in 350 states; INFERRED
  `StateEditorDisplayPosx/y`.
- DrawMode (+0x140): context +0x34 (`0x01027D20` at `0x01027F84`, into `0x01022DA0`); image draw
  `0x01028510` switches on it (arg 20) between sprite-batch virtuals +0x5C / +0x64 / +0x6C, the
  text draw between device render-state pairs; effect UNKNOWN, INFERRED weak `no_clip`.
- +0xD0 / +0xD4: UNKNOWN (only the loader and constructor touch them).

Code: `ntw_formats::ui_layout` (`UiState::text_x_offset / text_y_offset / editor_pos`,
`ALIGN_*`, `SPLIT_BY_*` / `NEVER_SPLIT`, `text_area`, `text_wraps`, `text_line_x`,
`text_block_y`, unit test), `napoleon::frontend::render` (text placement),
`ntw_script::ui::host` (`SetStateText` measure width, `GetStateTextDetails`,
`SetStateTextDetails`, `SetStateTextXOffset`, test `state_text_details_use_the_text_offsets`).

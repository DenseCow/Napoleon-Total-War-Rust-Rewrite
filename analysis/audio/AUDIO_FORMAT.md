# Audio: files, formats and how the original finds its sounds

Audio worker notes, 2026-10-03. Tags: **CONFIRMED** = seen in the exe or proven on every shipped
file; **INFERRED** = strong reading of the evidence; **UNKNOWN**. Exe addresses are Napoleon.exe
1.3.0 (Ghidra project `analysis/worker1/ghidra_project`). No decompiled code is reproduced here.

Code: `crates/ntw_formats/src/sound/` (readers, decoding), `crates/napoleon/src/audio/` (playback).
Research tool: `cargo run -p ntw_formats --example sound_probe --release -- <events|banks|names|vocab|params|formats|strings>`.
Ghidra helper: `analysis/audio/run_ghidra.ps1` + `analysis/audio/ghidra_scripts/AudioDecomp.java`
(worker 1's DecompTargets plus `vt:`, `xref:` and `ptrs:` commands). Output goes to a scratch folder, never git.

## 1. Survey of the install

| Where | What | Count / size |
|---|---|---|
| `sound.pack` (4.26 GB, type release) | all effects, music, unit voices, campaign voices, the packed sound database, its CSV/XML sources | 37,890 files |
| `sound.pack` `sounds_packed\sound_events` | **the event table the game plays from** (binary) | 3,019,772 B |
| `sound.pack` `sounds_packed\sound_bank_database` | **sound settings + sound banks** (binary) | 52,965 B |
| `sound.pack` `sounds\events\sound_events_*.csv` (14) | source of `sound_events` (names, readable params) | |
| `sound.pack` `sounds\banks\sound_bank_*.xml` (35) | source of the banks (condition names) | |
| `sound.pack` `sounds\sound_settings.xml` | source of the 154 settings | |
| `sound.pack` `sounds\nap_*.csv` (5), `movie_volumes.csv`, `sound_categories.csv` | sources of the campaign emitters, movie volumes, categories | |
| `sound.pack` `sounds\scripts\*.script` | CA's pack-builder scripts (`create ... sounds_music.pack`, `add_directory ...`); not used at run time | 22 |
| `sound.pack` `animations\**\*.anim_sound_event` | sound cues on animation timelines | 2,617 |
| `sound.pack` `sfx\...`, `ntw\...`, `music\...`, `front_end_music\...`, `campaign\...`, `<lang>\actor_*\...` | the sound files | 5,001 wav + 32,197 mp3 |
| `local_en.pack`, `local_en_patch.pack` | advisor speech `advisor\*.mp3`, battle-script speech (`bt_*.mp3`) | 1,776 + 110 mp3 |
| `data.pack` | DLC voices (`fr\pdlc\...`) | 490 mp3 |
| `battleterrain.pack` `presets\<map>\sound_emitter_list.xml` | per-battle-map ambience emitters (almost all empty) | 54 |

Codecs (CONFIRMED by reading every file's header, `sound_probe formats`, and decoding all 40,308 files
in `tests/sound_install.rs::decode_every_sound_file`):
- MPEG-1 Layer III `.mp3` (5 start with an ID3 tag, the rest with the sync word);
- RIFF `.wav`, format tag 1 (PCM): mono/stereo, 16-bit (one file 24-bit), 11,025 to 96,000 Hz;
- RIFF `.wav`, format tag 0x11 (IMA ADPCM): the 215 `*_adpcm*.wav` files.
All decode with the pure-Rust `symphonia` 0.5 crate (features mp3, wav, pcm, adpcm).

The exe uses Miles Sound System (`mss32.dll`, 52 `AIL_*` imports) plus `miles\*.asi` decoders
(mssmp3, mssogg) and DirectSound; no FMOD/Wwise/XACT (worker 1 report, CONFIRMED).

## 2. What the exe reads at start-up (CONFIRMED, `0x0048C1B0`)

After "Creating sound": the sound manager is created (`0x010021A0` -> constructor `0x01000540`,
vtable `0x013EE610`; output 44,100 Hz unless the setting is 22,050). Then:
1. If the Vfs has `sounds_packed\sound_bank_database` (and the rebuild switch is off), it reads
   `sounds_packed\sound_events` through manager vtable slot 2 (`0x01004B40` -> `0x0100FDA0`) and
   `sounds_packed\sound_bank_database` through `0x00481020` -> `0x00DF3600` -> `0x00DF3640`.
2. Otherwise (missing file, or the developer switch `build_sound_packs`) it rebuilds both from
   `sounds\events\*.csv`, `sounds\banks\*.xml` and `sound_settings.xml` and writes them to
   `packed_sounds_output_path`. This is a developer path; **players' games always run from the two
   packed files**, so ours do too.
3. Preference `generate_audio_pack_file` (debug) can also write a pack.

Volumes from the preferences are applied by `0x004837D0` (called from the main loop): it passes
preferences `sound_master/music/speech/sfx_volume` (0-100) to manager slot `+0xD8` with groups
5/0/2/1 and the matching `*_enabled` flags to slot `+0xE0`. CONFIRMED call shape; the gain law
inside Miles is UNKNOWN.

## 3. `sounds_packed\sound_events` (reader `0x0100FDA0`, CONFIRMED + exact-EOF parse)

```
u32 header                              0x3F800000 (1.0f; meaning UNKNOWN)
u32 n; n x { str name; f32 volume }     53 categories ("uncategorised", "unit_voices", "ui", "advisor", ...)
u32 6; 6 x u32 category                 special categories: advisor, unit_group_move, unit_group_melee,
                                         unit_voices, weather, environment_ambience (purpose UNKNOWN)
u32 n; n x 35 x f32                     962 parameter sets (0x0100AA70: 35 typed values, factory 0x0100BE90)
u32 n; n x event                        3,471 events
   event = u32 category; [str name if the category is named]; u32 params; u32 m; m x str file
u32 n; n x { str map; u32 m; m x emitter }   5 campaign maps of ambience emitters
   emitter = u32 id, u32 event, f32 min_dist, f32 max_dist, f32 x, f32 y, f32 z
u32 401; 401 x u32 event                the built-in slots (0xFFFFFFFF = none)
u32 n; n x { str movie; f32 volume }    14 movie volume multipliers
EOF
```
`str` = u16 length + UTF-16LE. Strings are lower-cased by the builder.

### 3.1 Named categories (CONFIRMED)
The reader reads a name only for events whose category is one of `ui`, `interface`, `advisor`,
`building_destroyed`, `unit_voices`, `mouse_over`; those go into a name -> event map (`0x010127E0`).
In the shipped file: 1,291 `ui` events (named after UI component ids, e.g. `grand_campaign`,
`button_quit`, and campaign message pictures `admiral-eu.tga`), 898 `unit_voices`, 465
`building_destroyed`.

### 3.2 Built-in slots (CONFIRMED)
The exe refers to its own sounds by an enum of 401 values. `0x0100DA70` builds the table of names
(`BUTTON_CLICK` = 0 ... `Music_Campaign_Spain` = 400, `NUM_SOUND_EVENT_ENUMS` = 401) and the file
maps each slot to an event. The names are in `ntw_formats::sound::slots::SLOT_NAMES` (a fact table,
like a DB schema). 49 slots are empty in the shipped data (e.g. `BUTTON_CLICK`, `DEFAULT_UI_SOUND`).
Examples: `MUSIC_FRONTEND` (375) -> `front_end_music\ntw_mus01.mp3`; `DRUM_FIRE` (133);
`CANNON_LIGHT_FUSE` (227); `THUNDER` (31).

### 3.3 Parameters (CONFIRMED by data)
The 35 values are the 35 CSV columns after `name, category`, in order, each an `f32`. For the
3,463 CSV rows matched to their events, every numeric column agrees on all but <= 3 rows, except
`PROBABILITY_REDUCTION_PER_NUM_SAME_SOUND_EVENTS...` (CSV -1 stored as 0). Text columns:
`playback` random=0 / random_cycle=0|1; `group` music=0, sfx=1, interface=3 (speech=2 INFERRED);
`game_mode_stay_in_memory` none=0, battle=2, campaign=3, land=4, naval=5; `speaker_output`
default=0; `FADE_TYPE` linear=0, equal_power=1. See `ntw_formats::sound::Param`.
Meaning of the values (pitch units, falloff law) is INFERRED from the CSV headers and
`sound_settings.xml` comments; see §6. Loop blocks are byte offsets (CONFIRMED, §8 "Loop points").

### 3.4 Event names (INFERRED)
Only named categories keep a name. `EventNames::from_csvs` names the rest from the shipped CSVs by
content (same category + same file list; groups of identical events paired in file order), because
the builder reordered the rows (UNKNOWN order). Result: 3,461 of 3,471 events named, 32 CSV rows
unmatched. Names are used for diagnostics and as a convenience lookup; playback never depends on a
name being right (slots, named categories and bank entries carry event indices).

## 4. `sounds_packed\sound_bank_database` (reader `0x00DF3640`, CONFIRMED + exact-EOF parse)

```
u32 header                    0x3F800000
154 x f32                     settings: the values of sound_settings.xml, in its element order
                              (152 of 154 names check out by value; 2 duplicate tags)
for t in 0..109:              bank factory 0x00E15D20 returns no bank for t = 3 and t > 28
   u32 n; n x entry
entry = u32 event, then L condition lists, each u32 m + m values (u32, or u8 for 3 lists)
EOF
```

### 4.1 What a bank is
A bank answers "which event for this situation". Each entry is an event plus, per condition, the list
of values it applies to (empty = any). E.g. projectile fire: `[gun_type][shot_type][audio_distance][?]`.

### 4.2 Per-type layouts (CONFIRMED from the entry readers, vtable slot 2 of each bank class)
Element readers `0x00E26D00`-style read a u32 and add it if not already in the list (values are
de-duplicated); `0x00E27110` reads a u8. Layouts (`ntw_formats::sound::banks::BANK_LAYOUTS`):
0,1,2: 4 lists; 4: 3; 5: [u32,u8,u8]; 6-9,17,18: 4; 10,11: 2; 12: 1; 13: 5; 14,16: 6 u32 + 1 u8;
15: 4; 19: 5; 20: 1; 21-28: 2. Entry counts: 1,381 in 28 banks.

### 4.3 Which bank is which (INFERRED, by pairing with the XML sources)
| type | source XML | condition lists |
|---|---|---|
| 0 | projectile_impact | shot_type, audio_material, ?, ? |
| 1 | projectile_fire | gun_type, shot_type, audio_distance, ? |
| 2 | projectile_explosion | explosion_type, ?, audio_distance, ? |
| 4 | projectile_idle | ?, shot_type, ? |
| 5 | region_buildings | subculture, ambience, capital |
| 6,7,9 | ambient_birds, ambient_forest_birds, ambient_environment | subculture, ?, precipitation_type, time_of_day |
| 8 | ambient_land | |
| 11 | building_ambience | |
| 12 | sea | sea_surface |
| 13 | group_movement | battle_entity_type, ground_type, size, speed, ? |
| 14 | naval_unit_voice_events | event, unit, unit_category, unit_class, ... |
| 15 | campaign_voices | agent, ?, event, special_edition_mask |
| 16 | unit_voice_events | event, unit, ?, unit_class, ?, unit_voice, general |
| 17, 18 | ambient_battle_forest, ambient_water_wildlife | |
| 19 | music_states | music_state, ?, subculture, random_number_selection, ? |
| 20 | projectile_hit_tree_canopy | shot_type |
| 21, 22 | bodyfall heavy / light | battle_entity_type, ground_type |
| 23-28 | individual footsteps (man walk/run/turn, horse, camel, elephant) | ground_type, footwear |
| 10 | (empty) | |

### 4.4 Condition names (INFERRED method, `ntw_formats::sound::bank_xml`)
The XML gives each entry's conditions by name in the same order the builder wrote the numbers, so
pairing XML entries with packed entries (by event name, or by the count of distinct values; entries
the builder dropped are skipped) yields name -> number tables, e.g. gun_type `musket_flintlock` = 11,
shot_type `bullet` = 12, audio_distance `close/medium/far` = 0/1/2, music_state `music_front_end` = 0,
`music_campaign` = 4, subculture `sc_european_north` = 13. Callers use names; a mod that ships a new
XML with its packed data gets its names too. Without the XML everything still works by number.

### 4.5 Choosing an entry (PROVISIONAL)
Every bank starts with a catch-all entry (e.g. `SILENT`, no conditions). We pick, among the matching
entries, the one meeting the most queried conditions; ties go to the earlier entry. The exe's real
rule is UNKNOWN (it is in the bank classes' query methods, not yet decompiled).

## 5. Other files
- `.anim_sound_event`: `u32 version (1); u32 n; n x { f32 seconds; u32 cue }` (CONFIRMED, all 2,617
  parse exactly). The cue numbers are an exe enum (UNKNOWN; musket volleys use cue 30 at 0.186 s).
- `battleterrain\presets\<map>\sound_emitter_list.xml` (`SOUND_EMITTER_LIST`/`simple_sound_emitters`
  strings in the exe): 52 of 54 are 273-byte empty lists; not read yet.
- Advisor speech: `advisor\*.mp3` in the language packs; `advisor\CreativeAssembly.%S.mp3` for
  battle scripts. Volume settings `SS_<LANG>_*_ADVISOR_VOLUME`. Not wired yet.
- Movies: volume multipliers in `sound_events` (§3). Bink playback: `crate::video` (hook `audio::PlayMovieAudio`, a live `PcmFeed` voice; gain and the exe's rule in `analysis/video/BINK.md` §8).

## 6. Run-time behaviour we reproduce, and how sure we are
| Behaviour | Source | Status |
|---|---|---|
| Front-end music = slot `MUSIC_FRONTEND` (= music_states entry `music_front_end`), looped, 2D | data | CONFIRMED event; when the exe starts it: INFERRED (front-end mode enter) |
| UI click = `ui` event named after the clicked component id | data (1,291 events named after component ids) | INFERRED; the exe's lookup function is not decompiled |
| No generic hover sound (only code-triggered `mouse_over_*` events) | data has no generic hover event | INFERRED |
| Projectile fire = bank 1 by gun_type/shot_type/audio_distance | data | CONFIRMED data, PROVISIONAL match rule (§4.5) |
| audio_distance close/medium/far from `AUDIO_DISTANCE_LAND_PROJECTILES_<SMALLARMS|ARTILLERY|...>_CLOSE/MEDIUM` | settings names | INFERRED |
| Volume = event volume x group volume (prefs 0-100) x master; music group = `music`/`loading_music` categories, speech = `unit_voices`/`advisor`, everything else sfx | prefs + `0x004837D0` | PROVISIONAL gain law |
| 3D: full volume inside min_dist, inverse-distance rolloff (`SS_VOLUME_ROLLOFF` = 1) beyond, silent past max_dist; stereo pan from the listener's right vector | Miles defaults | PROVISIONAL |
| Pitch: random in [min_pitch, max_pitch] semitones | CSV header | INFERRED |
| Looping: `loop_start_block`/`loop_end_block` are byte offsets into the file (Miles loop block), set only when both are non-zero and the end is inside the file; loop count 0 when looped (§8 "Loop points") | `0x01004430` + data test | CONFIRMED units and condition |
| Campaign music = bank 19, music_state `music_campaign` + faction subculture + random 0..3 | data | CONFIRMED data, PROVISIONAL selection timing |

## 7. Open questions (for Ghidra later)
1. The bank query/selection methods (vtable slots after 2 of each bank class).
2. The anim cue enum and who resolves cues (unit sound trackers).
3. Miles gain law: how `volume`, the 2D/3D multipliers (`SS_2D/3D_VOLUME_MULTIPLIER` = 2.0) and the
   group volumes combine.
4. When the music state changes (front end, campaign turn, battle phases) and the fade times
   (`MUSIC_FRONT_END_FADE`).
5. The exe function that maps a clicked UI component to its named `ui` event.

## 8. Playback in NapoleonRust (`crates/napoleon/src/audio/`)
- `GameAudioPlugin` loads `SoundLibrary` at start (PreStartup) through the Vfs.
- Each sound is a "voice": a `VoiceClip` Bevy audio source (our own `rodio::Source`), stereo out,
  whose left/right gains are set every frame (distance law + equal-power pan, group volumes, fades).
  Short files are decoded once and cached; long ones (files > 512 KB, `streamed`/looped events) are
  decoded while playing (`ntw_formats::sound::PcmStream`), except music (every Music-group event,
  looped or not), decoded whole in the background first (see "Ours" below).
- Event parameters used: volume, category volume, 2D/3D multiplier, pitch range, probability,
  rand_volume, start/random-trigger delay, fade_in, looped + loop blocks, min/max distance (x the
  battle/campaign `*_GLOBAL_RECORDED_DISTANCE_MULTIPLIER`), max playing at once, delay before
  replay, playback (random / random_cycle), streamed. Not yet: priority, low-pass, doppler,
  ducking, launch delay by distance (speed of sound), mute on game-speed change.
- Volumes: preferences `sound_{master,music,speech,sfx}_volume` (0-100) and `*_enabled`, read from
  our copy (`%APPDATA%\NapoleonRust\scripts\preferences.script.txt`, seeded read-only from the
  original's) and re-read within a second when the file changes.

### Hooks (Bevy messages in `crate::audio`)
| Message | Who sends it | Effect |
|---|---|---|
| `PlaySound { sound: SoundRef::{Slot, Named, Event}, position: Option<Vec3> }` | anyone | plays one event |
| `UiSound { component }` | front end (wired: completed left click) ; campaign/battle HUDs should send it too | `ui` event named after the component id |
| `ProjectileFired { gun_type, shot_type, position, shots }` | wired automatically from `battle::VolleyFx`; naval/other code can send it | projectile-fire bank by distance |
| `SetMusic { state, subculture }` | wired: FrontEnd -> `music_front_end`, Campaign -> `music_campaign`, Battle -> `music_land_deployment`; campaign/battle workers should resend with the player's subculture and battle phases (`music_land_battle`, `music_land_battle_results`, ...) | music state machine (crossfade with the events' own `fade_out` / `fade_in`) |
| `CampaignAmbience { map: Option<String> }` + resource `CampaignToWorld(Mat4)` | campaign worker (map name e.g. `nap_europe`) | starts/stops the map's positional ambience emitters |
| component `AudioListener` | camera owners | the ear for 3D sounds (default: first `Camera3d`) |


### Loop points: byte offsets, not frames (CONFIRMED units; fixes "music restarts partway")
**Exe** (`SoundVoice::StartSoundVoicePlayback` `0x01004430`, `LoadSoundFileIntoMilesSample`
`0x01012A40`; Miles 7.2g):
- Every sound, music included, is read whole into memory (`AIL_file_read`) and handed to
  `AIL_set_named_sample_file`. The exe imports no `AIL_open_stream`, so the `HSTREAM` branch of the
  start routine is dead and the event's `streamed` flag does not change how Miles plays it (INFERRED).
- Loop count: 0 (forever) when the voice's looped flag is set (event param 8, or the caller's flag),
  else 1. CONFIRMED.
- Loop block: params 20 / 21 (`loop_start_block`, `loop_end_block`) are copied as floats to the
  voice (`0x01000F60`). In this order (`0x01004824`..`0x01004876`): both FLOATS are tested against
  0.0 (no truncation yet, so 0.5 counts as set); then `end < buffer length` is tested as floats,
  the length (`HSAMPLE+0x10`) converted from unsigned (so any negative end passes); only then are
  both truncated (`CVTTSS2SI`) and passed to `AIL_set_sample_loop_block`. Otherwise no loop block is
  set and the whole file loops. CONFIRMED order and tests; "+0x10 = buffer length in bytes" (for
  an MP3, the whole file) INFERRED from Miles' sample layout and the data.
- Miles' loop-block arguments are byte offsets into the sample data. The first pass plays from the
  file start to the loop end, then it jumps back to the loop start. An end of -1 means the end of
  the sample (Miles API convention; INFERRED, not checked in `mss32.dll`); we treat any negative
  end that way.

**Data** (test `music_loop_blocks_are_byte_offsets`, real install): of the looped events with a loop
block, every in-range offset is exactly the byte position of an MPEG frame header; `loop_end_block`
is 92-99.98 % of the file size but only 42-91 % of the decoded length in frames. The common start
1044 is the second frame of the 320 kbit/s files (the first frame is skipped). Two events have an end
past their file (the silent placeholder and one `mus076_results_ottoman` event, 1224620 > 1202677
bytes): the exe sets no block for them and the whole file loops. Example: `ntw\music\land\ntw_mus10.mp3`
loops 16.5 s .. 109.1 s of 113.7 s.

**Root cause of the bug:** we used the offsets as sample frames, so every `ntw\music` battle track
(about 2.2 M bytes, 5 M frames) jumped back at 42-45 % of its length (about 50 s in), to a point
near its start.

**Ours:** only music (every event in the Music group, looped or not; every music-state event is
in it, install test `music_state_tracks_pass_the_start_rules`) takes this path; every other
sound plays as before (decoded-clip cache, or a stream decoded while playing). A looped one plays
its loop block, a non-looped one its whole file once. Non-looped music is decoded first too
(round 14) so that a file that cannot be read or decodes to no audio is marked failed before any
voice of it starts; streamed, it ended at once and was picked again every frame. PROVISIONAL: a
looped non-music event with a loop block loops its whole file (no shipped one has a usable block,
see the install test).
- *Load.* The file is read, walked (`LoopIndex`) and decoded whole on Bevy's async compute pool
  (`audio::load_music`), like the exe, which reads every file whole before Miles plays it. One load
  per file: a second request for a file being decoded waits for the same load.
- *Storage.* Interleaved 16-bit PCM (`ntw_formats::sound::Pcm16`, `decode_timed_i16`): the
  decoder's samples rounded to `i16` (x 32768, clamped; tested equal to the f32 decode rounded).
  Half the memory of f32; that Miles also keeps 16-bit PCM for its mixer is INFERRED (not checked
  in `mss32.dll`). The buffer is allocated once at its final size: an MP3's length comes from the
  frame walk (an MP3 without a Xing/VBRI tag has no frame count; growing by doubling peaked at about
  3x the final size), a WAV's from its header, bounded by the file size so a bogus header cannot
  reserve gigabytes (tested; the install test checks every looped MP3).
- *Cache.* `audio::MusicCache`, keyed by file only, holds the whole decoded file and its frame
  walk; each event's loop block is a frame range into those samples (`audio::loop_region`, mapped
  when a voice starts, a binary search), so two blocks on one file, an end of -1 included, share one
  copy. Main thread only, no lock. Eviction: plain LRU over unused files (no voice holds their
  samples) once the total passes 256 MB (`MUSIC_BYTES`, a byte budget, not a count cap: any number
  of files in use stay); a miss does not count as a use. Evicted samples are freed on the pool, and
  since the cache holds every file in use the audio thread never drops the last reference.
- *Change.* One pure step function, `audio::music_step(state, input) -> (state, actions)`, holds
  every music rule; the Bevy system `audio::music` only feeds it inputs and applies its actions.
  State: the asked-for state's candidate events, the playing track (voice, event), the waiting
  track (event, time of the change) and the last track started. Inputs: state change (its
  candidates; empty for no music, an unknown state, logged once, or no sound data), the playing
  voice ended, a frame tick, and the replies to a start: started, still decoding, failed, refused
  by the start rules. Actions: fade a voice out with its event's `fade_out`, begin a track (start
  rules, then start or decode), retry the waiting track, log "nothing playable" once per state.
  Each frame the system feeds the state change first, then the end of the playing track, then the
  tick, so the old state's waiting track is replaced before anything retries it. Rules: a pick is
  random among the candidates with a file not marked failed, avoiding the last track when there is
  another (`random_number_selection` INFERRED). The playing track keeps playing until the new one
  starts (at once when its file is cached, else when its decode is done), then fades out while the
  new one fades in with its own `fade_in` (CONFIRMED `0x010089D0` / `0x010086A0`). If the decode
  takes longer than `MUSIC_SWITCH_TIMEOUT` (1.0 s, PROVISIONAL: the exe does not wait), the old
  track fades out anyway, so one state's music does not carry over into the next (front end into
  battle); the new one still starts when ready. A change back to a state whose track is playing
  keeps it and drops the waiting one. A waiting track whose file fails is replaced at once by
  another (the hold still counted from the change). When no track of the new state can play (all
  failed, or the start rules refuse the pick), the old track fades out and there is no music until
  the next change (PROVISIONAL: the exe's handling of an unreadable music file is not traced; the
  start rules never refuse a vanilla music-state track: probability 1, no reductions, no repeat
  delay, no at-once cap, install test `music_state_tracks_pass_the_start_rules`). A silent
  placeholder (e.g. deployment music for subcultures without any) is a track like any other: its
  silent file plays, so the old track fades out as for any change (round 14; before, a pick of it
  was refused and left silence, or it was skipped and the old track carried over). CONFIRMED: the
  exe has no name rule for placeholders (no string of Napoleon.exe names one; its only `silent`,
  `0x013EEAF4`, is a token of the text event reader `0x01011EA0`). The start rules run in the
  order game speed, files left, distance, repeat limit, probability (one draw), at-once and voice
  limits, then the file, pitch and delay draws, so a refused sound draws and allocates nothing past
  its refusal (round 15; the exe's own order is not traced, and the audio RNG is ours, not the
  exe's). A track that waited for its decode meets the repeat, at-once and voice limits again when
  it starts (`Player::admit`, no draw); the play is then recorded (repeat limit, `random_cycle`)
  and a capped instance it replaces (`max_number_playing_at_once`) is stopped. The rules count the
  voices playing (`Player::live`): the voice query plus the voices started this frame whose
  deferred spawn the query does not show yet (`NewVoices`, emptied each frame), less the stopped
  ones, so plays started in one system (two emitters whose decode finishes together, a volley)
  cannot pass the at-once or voice limit together (round 16). Every (state x input)
  pair is covered by the table test `music_state_machine_table`. A chain of replies longer than
  1000 steps for one input (a bug guard; each Failed reply comes from a file just marked failed,
  so a chain ends by itself) is logged once and ticks do nothing until the next other input.
- *Other callers.* A music event asked for outside the music state machine (`PlaySound`, a UI
  sound, a campaign emitter) waits for its decode in `WaitingPlays` and starts when it is done
  (`audio::poll_loads`, each frame before anything plays), if `Player::admit` lets it then. At most
  one waits per request, told apart by event, emitter or not, and position: the same request
  asked for again while it waits is dropped, so N requests during the decode start one voice, not
  N looping copies, while two emitters of one event, or a `PlaySound` and an emitter, both wait
  (an emitter's voice keeps its emitter mark, so an ambience change stops it). A waiting play is
  dropped when its file fails, when it is still decoding after 10 s (`WAITING_PLAY_TIMEOUT`,
  PROVISIONAL: the exe does not wait; a decode finished on that frame still starts; the slowest
  decode is 230 ms in release and 161 ms in a debug build, `music_decoded_size`), and on every
  game-mode change, so a front-end request never starts in a battle (a timed-out play asks for
  no new decode). PROVISIONAL: the exe plays
  at once (Miles), so the same request asked for twice within our decode time could start twice
  there.
- *Failed-file check.* Runs for every shot of a volley, so it allocates nothing: every event's
  files are normalized once at load (`SoundData::paths`, shared `Arc<str>`s), and the check is a
  plain lookup of that path in the failed set; a pick clones the `Arc`, no string is built.
  The start rules count what plays in one pass (`Player::tally`).
- *Failures.* A file that cannot be read or decoded (any sound; for music also one that decodes to
  no audio, which would otherwise end at once and be picked again every frame) is logged once and
  not tried again that session; an event skips its failed files. An unusable loop block is logged
  once per (file, block), in its own warn-once set.
- *Audio thread.* All decoded sounds (f32 effects, 16-bit music) play through one feed that reads
  frame `i` of the buffer and wraps at the loop points: it decodes, seeks, allocates, locks and
  frees nothing. The two sample types are variants of one enum (`clip::Samples`), matched per
  frame, with no dynamic dispatch.

Measured over every shipped music file (every Music-group event, looped or not; release build,
`music_decoded_size`, whole files, 2026-10-07): the biggest buffer is 25.6 MB
(`ntw\music\credits\mus02.mp3`, i16 stereo), the slowest walk + decode 230 ms (the same file; 161 ms in a
debug build, whose dependencies are optimized; the slowest looped one was 179 ms, `ntw\music\naval\ntw_mus19.mp3`). PROVISIONAL: a music change
starts up to that long after the exe would (Miles decodes MP3 while playing).

The music cache maps loop blocks with `ntw_formats::sound::loop_points::LoopIndex` (one frame walk:
frame ends plus cumulative PCM counts, each lookup a binary search). The MP3 walk finds frames the
way our decoder (symphonia 0.5.5) does: ID3v2 skipped, a strict first frame (next header must
match), then resync over junk; a sync word whose frame would run past the end of the file ends the
walk, as the decoder's frame read hits the end there; Xing/Info/VBRI tag frames yield no samples and
are found where the decoder looks (right after the side info, even with a CRC). WAV blocks use the
decoder's frames per block (PCM 1; IMA-ADPCM `(block_align - 4ch) * 2 / ch + 1`; MS-ADPCM
`(block_align - 7ch) * 2 / ch + 2`). The walk matches the decoder on every vanilla looped file and
on synthetic junk/CRC/tag/MS-ADPCM files (tested). Each offset maps to the first sample of the
frame or block holding it. The exe's tests (above) give `Ok(None)`: no block, the whole file loops.
Music is decoded on its own timeline (`ntw_formats::sound::decode_timed`: a frame
the decoder rejects in mid-stream becomes silence of its length, and a packet that decodes short
or empty is padded with silence to its length (none in vanilla data; tested), so later frames keep
their numbers), the whole file (for every block of every event); one-shots are decoded with
damaged frames skipped, so a damaged leading frame does not delay them. Rejected frames at the end
of a file are always dropped (tested). A block the exe would set that we cannot map (unknown
format, offset past the last whole frame, negative start, end not after start), or a mapped loop
start with no decoded audio after it (damaged frames at the end), makes the whole file loop, with
a WARN once per file and block. MS-ADPCM WAVs (none shipped) use our own decoder
(`ntw_formats::sound::ms_adpcm`): symphonia's algorithm and output (tested equal on normal data), but
the step size saturates where symphonia's overflows `i32` on extreme data (a panic in builds with
overflow checks, garbage in release), so every block decodes within the `i16` clamp. Mono blocks
are decoded nibble by nibble (high first), so an odd frame count per block decodes its last frame
(a byte-pair loop left it at zero, a click every block; symphonia's WAV reader derives an even
count for mono from `block_align`, so no WAV reaches it), and each block is read from its own offset (`block_align` bytes apart, so trailing bytes do not shift later blocks) in the
packet (tested). PROVISIONAL (Miles' handling unknown, no shipped event
needs it): mid-frame/mid-block rounding, negative start, reversed offsets, undecodable frames,
saturated MS-ADPCM steps.

**Music track choice:** a looped music event loops forever until the music state
changes; the next track is picked only on a state change (or when a non-looping track ends) (bank 19 query, `random_number_selection`
INFERRED). No timer or end-of-track rule picks another track while a looped one plays.

**In-game check (quick look):** `cargo run -p napoleon -- --battle-key NHB_Austerlitz --skip-deployment`,
listen past 2 minutes: the battle track runs to about 110 s, then continues from about 16 s in
without a restart from the top. Side by side with the original (runtime confirmation of the loop
block arguments via the debugger needs a battle): start Napoleon.exe, Historical Battles >
Austerlitz > Start, skip the intro, and break on `AIL_set_sample_loop_block` (`0x01004876`).
Result (user, 2026-10-07, merged `d6c843b`): no restart from the top in 3 minutes of Austerlitz. The debugger side-by-side is still to do.

## Where I am / what's next (audio worker, updated with each push)
- DONE: survey; readers for `sound_events`, `sound_bank_database`, `.anim_sound_event`; CSV/XML naming
  layer; MP3/PCM/ADPCM decoding (streaming and whole-file); install tests (all 40,308 files decode);
  Bevy playback: front-end music, UI click sounds, volumes from our prefs copy, positional voices,
  music states, volley -> projectile-fire bridge, campaign emitter hook.
- NEXT (not started): footsteps/group movement (banks 13, 21-28) driven by unit movement and ground
  type; projectile impacts/explosions (banks 0, 2); unit voices (bank 16); battle ambience (banks 6-9,
  12, weather); advisor speech; the Ghidra questions in §7; music phase logic in battle/campaign.

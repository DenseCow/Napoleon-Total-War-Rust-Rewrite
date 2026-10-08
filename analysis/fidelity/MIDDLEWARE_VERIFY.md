# Middleware and verification (BACKLOG §0, worker 0-C)

Worker 0-C notes, 2026-10-03. Tags: **CONFIRMED** = read in `Napoleon.exe` 1.3 or `mss32.dll` (own Ghidra copies, see
below); **INFERRED** = strong reading; **UNKNOWN**. No decompiled code is reproduced here: these are specs in our words.

Ghidra: `analysis/fidelity/run_ghidra.ps1 <exe|miles> <targets> <out>` with `ghidra_scripts/F0cDecomp.java`
(worker 1's DecompTargets plus `ext:` imports, `vcall:` virtual call sites, `insn:` operand search, `dis:` listing,
`exported:` by symbol). Projects: `%USERPROFILE%\Documents\NR-f0c-ghidra` (Napoleon.exe) and
`%USERPROFILE%\Documents\NR-miles-ghidra` (a copy of `mss32.dll`, analysis only; we never load or call it).
Output goes to a scratch folder, never git.

## 0. Where I am / what's next
**RESUMED 2026-10-04.** Workspace build / test / clippy pass (no new warnings). Game runs: main menu (front-end music
starts), battle `--battle --skip-deployment --ai off` (volleys at 342..533 m, "close" and "medium" audio distances, start 6/6
voices each; nothing closer than 342 m happened in the 9-minute run with `NAPOLEON_BATTLE_ZOOM=40`; no panics). Both
audio distances picked the same bank event (3468) for musket_flintlock / bullet: that is the data (one musket entry, §3). Loudness checked
against the exe (§1.3 "Loudness check": the ×10 master level and the Q11 clamp are the original's). Added since: the
game-speed sfx rule (`GameSpeed`), `UiSound` event kinds with the §2 rules (`UiEvent`, `ui_sound_choices`), the
determinism audit + twice-run harness (§4: battle and campaign IDENTICAL), bank / cue leads (§3). Headphones multiplier:
PROVISIONAL (speakers always; Miles' speaker type is not available to us). Doppler: matches the original (the exe sets
no velocities, so the factor is 1).
- 2026-10-04 later: UI hover / right-click sounds wired (§2; the grand_campaign "missing click" was a harness path
  mistake); cue enum INFERRED (§3, cues 27..106 = slot + 187); comparison harness + `docs/COMPARE_WITH_ORIGINAL.md` (§5);
  close-range battle run: with `NAPOLEON_BATTLE_CAMERA=12,-80,25,0,0.3` volleys at 6, 25, 44, 68..149 m ("close") all
  started 6/6 voices; volleys past max distance (911 m) started 0/6, as the original does.
- 2026-10-04 round 3: cue dispatch and bank query time-boxed (not found; leads in §3). Bink (BINK.md §7, §8.3): intro
  order CONFIRMED (LIFO queue: SEGA, Intel, intro), full-screen size rule CONFIRMED and implemented, intro and front-end
  movie on by default (`--no-intro`, `--no-frontend-movie`; harness runs skip them), skip rule still PROVISIONAL (leads
  written). Battle harness runs (`NAPOLEON_BATTLE_CAMERA` / `_ZOOM` / `_SCREENSHOT`, `NAPOLEON_AI_SHOT`, `--screenshot`) now
  ignore the live keyboard and mouse for the camera and for orders (`view::camera_harness_run`): the drift came from live
  input (there is no edge scrolling; wheel and keys move the camera).
- 2026-10-04 round 4: bank query and cue dispatch time-boxed again (4 headless runs; still not found, §3 new
  leads). The animation-object maker `0x010CB060` takes the cue list as its 7th argument CONFIRMED at the `0x00F85780`
  call site; the global battle-side cue list (`0x01650414`) has no consumer reads (the loader writes, `0x00F76E30`
  teardown frees). No code change (nothing new CONFIRMED).
- NEXT: ~~the bank query function~~ **CLOSED NEGATIVELY in round 11 - nothing in 1.3 dispatches one (§3 top)**;
  the cue dispatch to CONFIRM the cue mapping (the dispatch itself is closed exe-side dead, round 9); OnMove / OnShortcut
  sounds once our UI runtime has sliders and shortcut targets.
- 2026-10-04 round 7 (sandbox): the "vtable slot 7" premise is REFUTED - `0x01469494` / `0x0145D504` are global
  `UIFileIn` objects, not vtables (details and the new next leads in §3). Cue container / anim object / walk anim set
  layouts are now CONFIRMED (0x1C container, 0xA4 anim object with the list at +0xA0, 0x34-byte walk anim set of 10).
  The per-frame tick is still NOT found; no code change.
- 2026-10-04 round 8 (sandbox): the primary lead is CLOSED with a complete xref census - the global cue list
  `0x01650414` / `0x01650410` / `0x0165040C` has 8 / 8 / 3 refs, all in the loader `0x00F843A0`, the teardown
  `0x00F76E30` and the exit-time free `0x013013B0`, so **no per-frame reader exists** (§3). Two premises refuted:
  that list is campaign-side, not battle-side, and `Animations/BattleConfiguration/` (`0x00E611E0`, 0x1E610-byte
  objects) is the battle *animation action* table, not a cue list. The 0xA4 anim object is a bone-attached
  animation instance (full layout in §3). No code change.
- 2026-10-04 round 11 (sandbox): **the refuted fragment rule is fixed in our own code and locked by tests, and
  the bank query is closed negatively.** `0x00E5F760`'s 53 bytes re-read at instruction level: the fragment is
  `index % count` (one unsigned `DIV`), the five fragments per slot are **alternatives**, and the count is the
  group field `+0x14` with a **fallback to group 0** when it is 0; at both call sites (`0x006613A0`,
  `0x0065FB00`) the group is the **slot** and the dividend is the entity's `+0x1E0`. Our module docs said the
  opposite ("a later fragment that names a slot replaces that slot") - fixed, and **no test had asserted it**
  (`parses_fragment_lines` already asserted 2 alternatives for `STAND`; the wrong rule was prose only, and the
  one test whose *name* carried it was about a different, still-PROVISIONAL rule and is now renamed). New:
  `fragment_index`, `fragment_index_with_fallback`, `Fragment::fragment`, `has_clip(slot)`, `DESCRIPTOR_BYTES`,
  `descriptor_field`, and the shipped predicate table locked to **708 set / 156 clear** over 864 slots (measured,
  not transcribed: values are only 0 or 1). `+5` test fns, `cargo test -p ntw_formats --lib` **157 passed** (was
  152), no new clippy warnings in the file. Bank query: a complete `getCallingFunctions` **and** `xref:` census of
  every slot of the condition vtable `0x013A91F8` (`"audio_distance"`), the bank vtable `0x013A993C`
  (`"sound_bank_projectile_impact"`) and the entry vtable `0x013A95A8` finds **0 callers and 0 code references for
  every bank-specific slot** - the weight `0x00CB0280` has 13 refs, all DATA vtables; `0x00E28DF0` has exactly
  one, its own vtable. The two slots with callers are inherited base methods (one reached only by the **AI
  negotiator**, one a `*(this+0x24)=0` reset with 25 engine-wide callers). **In 1.3 nothing dispatches a bank
  query**, so our PROVISIONAL *weighted* matcher is probably wrong; stop re-running that hunt. Details, the
  measurement tables and the caveat are in §3's round-11 block.
- 2026-10-04 round 10 (sandbox): **the per-frame consumer of the battle ACTION table is FOUND** - the last open
  lead for battle animation is closed. Chain, all by complete `getCallingFunctions` census (never an offset
  scan): loader `0x00E611E0` <- `0x00E615F0` <- `0x00E66F00` <- `0x00E50970` <- `0x00E10170` ("battle_entity_
  animation_table_manager", roots at `Animations/Animation_Tables/`, caches the 0x110-byte manager at owner +0x6D0);
  the name-map buckets `0x0145BAD4`/`0x0145BAD8` have 5 / 3 refs, **all** inside the resolver `0x00E5FA30` or the
  atexit stub, so nothing reads them per frame. The battle entity constructor `0x0061CB90` stores its table at
  **+0x218** (`0x0061CDCC`); the per-frame chain is `0x0066CE00` (per-frame entity update, vtable-only) ->
  `0x006618D0` (bearing -> slot 53..58) / `0x006613A0` (best-direction slot pick, max dot product) ->
  **`0x00E5F760`** (43 callers; `fragment = slot % count`, the count at descriptor `+0x14`) -> `0x006666E0`
  (26 callers; play, blend 1.0f at +0x448) -> `0x007F1730` (queue lookup, 9 callers). **`0x00817BB0` picks the
  transition clip by weighted random** over `from*0x49 + to` (73 states). **Two earlier INFERRED readings are
  REFUTED/CONFIRMED: the five fragments are ALTERNATIVES chosen by modulo, not a priority list** (our
  "later fragment replaces the slot" is wrong), **and the descriptor `+0x14` is the per-slot "has a clip"
  predicate** - 708 set / 156 clear, read at runtime by five functions. `+0x08` / `+0x0C` / `+0x10` are read by
  **nobody** except the resolver (`0x013AEBE0` has exactly 2 refs): `+0x08` is a 32-code data-side enum whose
  names stay UNKNOWN, and the other two are inert link indices. Bank query still UNKNOWN; the weight accessor
  `0x00CB0280` and the entry reader `0x00E28DF0` both have **0 direct callers** (vtable-only), which explains
  every failed round - hunt it via the vtable next, not the call graph. Code: **tests only**, +6 test fns in
  `battle_animation.rs` (sentinels 10/11, `special` 0x8000, the 0x360 tag, `blend_in_time` = 1.0, `strcmp` case
  sensitivity, `SLOT_NAMES[i]` index spot-check); `cargo test -p ntw_formats --lib` **152 passed** (was 146).
  No behaviour change - the selection rule is CONFIRMED but needs the clip queue and state machine to be usable.
- 2026-10-04 round 9 (sandbox): **the cue thread is CLOSED - do not re-open it.** The `+0xA0` read scan plus a
  complete `getCallingFunctions` census of the three sound play paths proves there is **no exe-side path from any
  frame to a sound start** outside the sound manager (6 call sites, all `0x0100xxxx`), so the `.anim_sound_event`
  cue lists are parsed, cached and never read; cues are data-side only (§3 "CUE DISPATCH: CLOSED"). Round 8's
  paired-offset lead is refuted as a filter (78 hits). Fresh area chosen and finished: the **battle animation
  ACTION table** is now fully characterised - 864 slots × 5 fragments × 0x1C, the complete keyword grammar and
  the exe's fatal-error rules, the closed `equipment_usage` (10 names) and `special` (10 bits) sets, the five
  on/off flag bits, `blend_in_time` defaulting to 1.0, and `ACTION_<name>` -> slot index resolved through a
  864-entry table at `0x013AEBE0` whose order *is* the index. All 864 names transcribed into
  `ntw_formats::battle_animation` (`SLOT_NAMES`, `action_slot`) with 9 new unit tests; no behaviour change.
  (That file was also run through `rustfmt`, so the round's diff there is +663/-17 rather than purely additive:
  the 17 deletions are rustfmt reflowing pre-existing long lines and dropping a UTF-8 BOM. Semantics unchanged.)
- 2026-10-05 round 13 (sandbox): **the `+0x2C` store is found and it is a plain append, and the 63
  unnamed states are not what round 12 thought they were.** `0x00E60260` (1019 bytes, 5 callers, one per
  record type) is the population loop: `MOV [ESI+EDX*4],EAX` with `MOV [ESI+0x14],count+1` at
  `0x00E605D6`, fragments at stride `0x1C`, slots bounded by `CMP EAX,0x1e600` = 864, and `+0x2C` allocated
  and zeroed by the record ctor `0x00E506D0`. Written and tested as `SlotBlock` / `populate_slot_group` /
  `populate_runtime_table` / `Fragment::slot_image`. Round 12's premise for task 2 is **REFUTED**: only six of
  the 13 `param_1` constants are literals and four are computed, so they are not the enum - the enum is the
  73-wide pose field `entity + 0x1D8`, mapped to states by `0x00663730` (715 bytes, no callees, 73 cases),
  and that map reaches exactly **63 distinct states**. Ten of them are the already-named families (poses 0..7
  permuted, plus 37/38); the other 53 stay UNKNOWN and are not invented. The handle is a loaded clip object
  interned per path by `0x00E60660`; its `+0x48`/`+0x4C` are CONFIRMED and `+0x44`/`+0x50` partially, so
  task 4 is a partial layout and stopped. The resolver is now tested against all 864 real slots (708 with a
  handle, 156 on the documented fallback, and the fallback's precondition). Descriptor `+0x08`/`+0x0C`/`+0x10`
  are **not inert after all** - round 10's census used the wrong base address - but were not decoded.
  `cargo test -p ntw_formats --lib` **167 passed** (was 163); workspace build clean, no new clippy warnings.
  Details and the full evidence table in §3's round-13 block.
- DONE (code, 2026-10-04): NEXT 1 = `crates/napoleon/src/audio/mixer.rs` (all §1.3–1.9 rules as pure functions + 9 tests;
  the 3D pan and the Q13 low-pass were re-checked in Miles `0x2112A640` / `0x2112EB30`: rear share = ÷ (2 × 3 groups) once per
  real speaker; Q11 and Q13 use x87 round-half-even). NEXT 2 mostly done in `audio/mod.rs`: `Volumes` = 6 groups {enabled,
  int volume}, movie group = round(MOVIE_VOLUME × 100); `MixSettings` from `sound_bank_database`; `AudioMode` (campaign);
  `play()` with event distances, no start past max, probability reductions, priority stealing (× 0.95 in front NOT done),
  launch delay / random trigger delay in 0.01 s steps; group from param 10; no category volume or rand_volume; music fade =
  the old event's fade_out (`MUSIC_FADE_S` removed); `update_voices` uses the law, equal-power fades, fade end < 0.001,
  stop past max (looped: muted), distance falloff incl. the exe callback, stereo 3D gains, Q11, low-pass via
  `VoiceControl::set_cutoff` + `LowPass` in `clip.rs` (runs at the sample rate: PROVISIONAL for output ≠ sample rate).
- RESUME HERE: (a) `cargo build/test/clippy --workspace` (no new warnings); (b) run the main menu (music, clicks) and a
  battle (volleys near/far) and listen: loudness changed (2D = min(1, 10 × 0.81 × v^(5/3)), e.g. UI click ≈ 0.31,
  front-end music clipped at 1.0); check no panics; (c) the rest of the old NEXT list from item 3 (UiSound event kinds),
  then bank selection + cue enum, then the determinism audit and harness. Not done from NEXT 2: game-speed sfx rule
  (§1.6 last item), headphones multiplier, doppler (no velocities set).
Earlier state (2026-10-03): spec only, the Ghidra helper, `sound_probe settings`, these notes.
- DONE (spec, Ghidra evidence): the Miles mixing rules end to end (§1: gain law, 2D/3D multipliers, groups and master,
  Q11 clamp, falloff law and per-event callback, distances and multipliers, launch delay / speed of sound, low-pass,
  fades and music crossfade timing, 3D stereo panning, doppler, no ducking), the settings → manager map (§1.2,
  resolves BINK.md "sound group 4"), the UI click / hover / slider → event lookup (§2), the bank condition weights (§3).
- NEXT, in order:
  1. Write `crates/napoleon/src/audio/mixer.rs`: pure functions for §1.3–1.9 (`manager_gain`, `miles_curve`,
     `miles_pan`, Q11 `channel_gain`, `distance_gain` incl. the exe callback, `event_distances`, `launch_delay`,
     `low_pass_cutoff`, `equal_power_fade`, `stereo_3d_gains`, a Q13 Butterworth `LowPass`) with unit tests.
  2. Rewire `audio/mod.rs`: `Volumes` as ints + enabled flags per group 0..5 (group from event param 10, not the
     category; interface = 100; movie = MOVIE_VOLUME×100); drop category volume and rand_volume; `update_voices`
     uses the law; stop non-looped voices past max (mute looped emitters: deviation, the original's emitter manager
     restarts them, INFERRED); low-pass cutoff via `VoiceControl` and the filter in `clip.rs`; equal-power fades;
     music fade = old event fade_out / new event fade_in (remove `MUSIC_FADE_S`); launch delay; random trigger
     delay in 0.01 s steps.
  3. `UiSound` gets an event kind (LClickUp/Shortcut, RClickUp, Move, MouseOn) with the §2 rules; callers are
     `frontend/mod.rs:150`, `battle/hud.rs:388`, `campaign/hud.rs:353` (default = LClickUp, additive).
  4. Bank selection with weights (§3); keep looking for the query function (try the projectile-fire caller).
  5. Then the determinism audit (§4) and the comparison harness (§5), as in the task.
- NOT FOUND YET: the bank query function (§3), the `.anim_sound_event` cue enum (no string reference found).

## 1. Sound mixing, 1:1 (Napoleon.exe sound manager + Miles 7 `mss32.dll`)

### 1.1 What the exe calls (CONFIRMED, `ext:AIL_` import scan)
52 `AIL_*` imports. The mixing-relevant ones: `AIL_set_sample_volume_pan` (always pan 0.5), `AIL_set_sample_3D_distances`,
`AIL_set_sample_3D_position`/`velocity_vector`, `AIL_set_listener_3D_position`/`orientation` (up = (0,1,0)),
`AIL_set_3D_rolloff_factor`, `AIL_set_3D_doppler_factor`, `AIL_register_falloff_function_callback`,
`AIL_set_sample_low_pass_cut_off`, `AIL_set_sample_playback_rate`, `AIL_set_digital_master_volume_level`,
`AIL_set_sample_loop_block`, `AIL_sample_stage_property` ("Ramp Time"/"Ramp To"/"Ramp At" on the "Volume Ramp Filter").
Not imported: any reverb, cone, occlusion or 5.1 level call, so those stay at Miles defaults.

### 1.2 Settings → manager (CONFIRMED, `0x00E31F40` applies `sound_bank_database` settings via manager slot 42 `0x01005E40`)
| manager property | setting (index) | shipped | use |
|---|---|---|---|
| 0 | SS_MILES_DIGITAL_MASTER_VOLUME_LEVEL (7), clamp 0..100 | 10 | `AIL_set_digital_master_volume_level(10.0)` (also at driver open) |
| 1 | SS_VOLUME_ROLLOFF (8) | 1 | `AIL_set_3D_rolloff_factor`; also keeps ln(rolloff+1) |
| 2 | SS_VOLUME_CUTOFF (9) | 1 | max-distance estimate (§1.5) |
| 3 | SS_LOW_PASS_FILTER (10) ×0.001 | 0.00001 | low-pass slope (§1.7) |
| 4 | SS_DOPPLER (11) | 1 | `AIL_set_3D_doppler_factor` |
| 6 / 0xC | GLOBAL_RECORDED_DISTANCE_MULTIPLIER (37) / CAMPAIGN_… (62) | 14 / 20 | event distance multiplier; campaign one when game mode = 3 |
| 8 | SS_LOW_PASS_FILTER_MIN (56) | 0.05 | low-pass floor |
| 9 | SS_BATTLE_SCRIPT_VOLUME (59) | 0.15 | battle-script speech |
| 10 / 11 | SS_SPEAKERS / SS_HEADPHONES_VOLUME_MULTIPLIER (60/61) | 1 / 0.5 | picked by `AIL_speaker_configuration` (0x20 = headphones) |
| 0xD,0xE,0xF | MIN_DIST_TO_APPLY… (69), MIN_DISTANCE_FROM_LISTENER_TRIGGER_DELAY (70), SPEED_OF_SOUND (71) | 100, 0.1, 340.29 | launch delay (§1.6) |
| 0x10 / 0x11 | SS_2D / SS_3D_VOLUME_MULTIPLIER (74/75) | 2 / 2 | gain (§1.3) |
| 0x13..0x42 | the 48 advisor volumes (103..150, 8 languages × 6) | | advisor |
| group 4 volume | round(MOVIE_VOLUME (32) × 100) | 100 | movies (resolves BINK.md "sound group 4") |
Property 7 is the game mode (not a setting); `SS_GAME_DUCKING_VOLUME_MULTIPLIER` (76) is never applied: the manager's
ducking factor (`+0xC16C`) is set to 1.0 in the constructor and nowhere else, so **there is no ducking** (CONFIRMED).

### 1.3 The gain law (CONFIRMED: manager `0x01008320`, Miles `0x2112CFB0` + mixer `0x21130720`)
Volume groups: 0 music, 1 sfx, 2 speech, 3 interface, 4 movie, 5 master; each {enabled, volume int 0..100}, all
constructed enabled at 100 (`0x010009E0`). Preferences drive 5/0/2/1 (`0x004837D0`); group 3 stays 100; group 4 from
MOVIE_VOLUME. The event's group is its own `group` parameter (param 10), **not** its category.
1. Manager gain (0 if group 5 or the event's group is disabled):
   `g = clamp01(vol5 × volG × 0.0001 × event.volume × speaker_mult × (is2D ? SS_2D : SS_3D))`.
   No category volume and no `rand_volume` (neither is read on the play path `0x01000F60`/`0x01004430`/`0x01008320`).
2. The voice volume sent to Miles = `clamp01(g × fade)` with pan 0.5 (`fade` = the voice's fade level, §1.8).
3. Miles: `v' = v^1.6666666`; pan 0.5 → both sides `v' × 0.8122522` (= 0.5^0.3); other pans `(1-p)^0.3 × v'`,
   `p^0.3 × v'` (×0.8122522 again with ≥ 4 speakers).
4. Mixer: per output channel `gain × spatial(3D only) × master_level (10.0) × dry levels (1, Miles default)`,
   converted to Q11 (`round(x × 2048 + 0.5)`) and **clamped at 2048 = 1.0**.
So a 2D voice plays at `min(1, 10 × 0.8122522 × v^(5/3))` per side.

**Loudness check (2026-10-04, CONFIRMED that this is the original's behaviour, not our scaling).** The master level is
really ×10: the manager's property 0 (`0x01005E40` case 0) clamps `SS_MILES_DIGITAL_MASTER_VOLUME_LEVEL` to 0..100 and passes
the float as is (10.0) to `AIL_set_digital_master_volume_level` (also at driver open, `0x01003E56`); Miles stores it
unclamped at driver +8 (`_AIL_set_digital_master_volume_level@8` `0x211165D0`), and the mixer `0x21130720` multiplies each
voice's channel gain by driver +8 before the Q11 conversion with its clamp at 2048. Numbers at the user's real
preferences (master 100, music 16, speech 100, sfx 100, all enabled):
| sound | event volume × 2D mult | group | manager g | per side |
|---|---|---|---|---|
| UI click | 0.07 × 2 | interface 100 | 0.14 | 10 × 0.812 × 0.14^(5/3) = 0.306 |
| front-end music | 0.3 × 2 | music 16 | 0.096 | 10 × 0.812 × 0.096^(5/3) = 0.164 |
| front-end music at the default music 100 | 0.3 × 2 | music 100 | 0.6 | 3.5 → clamped 1.0 |
So the clipping at 1.0 is what Miles does at default settings (the mixer clamps per voice; the summed mix is then
saturated to 16 bits as usual). Our `voice_2d_gain` reproduces these numbers (`mixer::tests`).

### 1.4 Distance falloff (CONFIRMED: Miles `0x2112A430`, default falloff `0x2112A410`, exe callback `0x01002390`)
- `d = |source − listener|`; `min' = max(min, 0.0001)`; gain 1 when `d ≤ min' + 0.0001`, else
  `min' / ((d − min') × rolloff + min')` (Miles default, rolloff = SS_VOLUME_ROLLOFF).
- When an event's own `falloff` (param 5) differs from the global rolloff, the exe registers its own callback: same
  law with `rolloff = trunc(falloff × 100) / 100`, and gain 1 unless `d > 1` and `min > 0.01`.
- Past max: Miles clamps d to max (or mutes), and the exe **stops the voice** when `d² > max²` (per-frame update
  `0x010075F0`), and does not start a 3D voice whose start distance is past max (`0x01005140`).

### 1.5 Distances (CONFIRMED, `0x01005140`, `0x01003620`/`0x01003660`, `0x01002110`)
`min = event.min_dist × M`, `max = event.max_dist × M`, M = 20 in campaign mode else 14. If `max ≤ 0`:
`max = min × max(1, 1 − ln(cutoff × 0.01) / ln(rolloff + 1))` (the log function is an SSE libm call: INFERRED `ln`; the
ratio is base-independent).

### 1.6 Launch delay, probability, priority (CONFIRMED, `0x01005140`, `0x010015E0`, `0x01000F60`)
- Speed of sound: if `apply_launch_delay_relative_to_distance` and `d > 100`: `delay = (d − 100) / 340.29`, and 0 if
  `< 0.1` s.
- Otherwise `random_trigger_delay` r: `delay = k × 0.01`, k uniform in `[0, round(r × 100))` (only when the launch
  delay is < 0.01). Then `start_delay` (param 16) is added when the voice starts.
- Probability (param 11) minus `probability_reduction_*` × count of the same/any playing; plays if `p ≥ 1` or
  `rand01 ≤ p`; `p ≤ 0` never.
- Pitch: `min_pitch..max_pitch` uniform; playback rate = `round(2^(pitch/12) × file_rate)` (semitones CONFIRMED).
- `max_number_playing_at_once` = 1000 means unlimited; otherwise the lowest-priority instance is stolen if the new one
  has higher priority. Priority = param 7 − `priority_reduction_by_distance` × d (× 0.95 in front of the listener).
- When the game speed ≠ 1: sfx (group 1) events do not start, and playing sfx fade out over 1 s (`0x01001860`).

### 1.7 Low-pass (CONFIRMED: exe `0x01003590`, Miles `0x2112EB30`)
Only 3D voices, and never in campaign mode (game mode 3). Each frame: `x = 14 × d²` (battle multiplier, property 6),
`f = slope × (x − 2.5) + 2.5`, `cutoff = f ≤ 0 ? 1 : 2.5 / f`, clamped to `[SS_LOW_PASS_FILTER_MIN, 1]` (fraction of the
sample's Nyquist). Miles: ≥ 0.999 → filter off; `wc = cutoff × playback_rate / output_rate` clamped ≤ 0.98, a 2-pole
Butterworth (bilinear, `c = 1/tan(wc·π/2)`, `a0 = 1/(1 + √2c + c²)`) whose coefficients are quantised to Q13:
`A = round(8192·a0)`, then c is recomputed from A/8192, `B1 = round(16384·a0(1 − c²))`, `B2 = round(8192·a0(1 − √2c + c²))`.
The per-sample loop is INFERRED as direct form I: `y = a0(x0 + 2x1 + x2) − (B1/8192)·y1 − (B2/8192)·y2`.

### 1.8 Fades (CONFIRMED: `0x010075F0`, `0x01006E50`, `0x01007310`)
Linear in time (ms clock). `fade_type` 1 (equal power) maps each linear value v through the fast square root
`bits(v) → ((bits − 0x3F800000) >> 1) + 0x3F800000`. A fade-out ends (voice stopped) when the level is < 0.001. Stop
with "-1" uses the event's own `fade_out` (param 15); the music player stops the old track that way and starts the
new one with its `fade_in`, so **music crossfade timing = the events' fade_out / fade_in** (CONFIRMED path
`0x010089D0`/`0x010086A0`; no global MUSIC_FRONT_END_FADE constant is read).

### 1.9 3D panning on stereo (CONFIRMED: Miles `0x2112A640`, speaker setup `0x2112A0D0`, `0x21133BE0`)
Speakers (listener space, x right, y up, z forward): FL (−√½, 0, √½), FR (√½, 0, √½) (INFERRED for the stereo layout:
every layout in the driver-open table starts with these); since no speaker faces backwards, Miles adds a virtual rear
speaker (0, 0, −1) whose share is split equally over the real ones (÷ 2 × 3 groups). Per speaker group k:
`a_k = π − acos(clamp(dir·s_k))`, `w_k = sqrt(a_k^3 / Σa)` (falloff power 3, `AIL_set_speaker_configuration` default),
channel = `clamp01(w / √3) × distance_gain`. Source at the listener (d < 0.0001): every channel `√(1/n) × gain`.
Doppler: `pitch × 0.355 / (0.355 + v_radial × doppler × distance_factor)` clamped to [0.25, 4]; the game only sets
velocities through its velocity hook (default zero).

## 2. UI sounds (CONFIRMED, handler `0x0047F0C0`, event types = the UI `EVENT_NAMES` order)
| UI event | sound |
|---|---|
| OnMouseLClickUp (3), OnShortcut (16) | id starts with `entry_` → slot `UNIT_CARD_SELECTED` (316); else the `ui` event named after the id; none → the default UI event (empty in the shipped data) |
| OnMouseRClickUp (6) | `entry_` → `UNIT_CARD_RIGHT_CLICK_SELECTED` (317); else `right_click_<id>` |
| OnMove (13) | `slider_moved_<id>` |
| OnMouseOn (21) | id starts with `item` → `mouse_over_Slot1`; else `mouse_over_<id>`; if none and the id starts with `entry_art_`/`entry_inf_`/`entry_cav_` → slot `mouse_over_unit_card` (399) |
All play in 2D. Clicking `spain_campaign` / back / forward also changes a music flag (not done).

Wired (2026-10-04): left clicks in the front end, campaign HUD and battle HUD (since before); pointer-enter
(`MouseOn`) and right-click release (`RClickUp`) on the component under the cursor in all three. `OnMove` (sliders) and
`OnShortcut` are not sent yet: our UI runtime has no slider drag or shortcut-to-component path (PROVISIONAL). What the
shipped data has for them: `slider_moved_handle`, `right_click_item1..20`, `mouse_over_icon`, `mouse_over_unit_card`
(`mouse_over_Slot1` is not in the data). The "missing click" on `grand_campaign` (2026-10-04) was a harness mistake:
`grand_campaign` is on the single-player page, not the main page; `--ui-click single_player,grand_campaign` plays event
1602 (`single_player`) and 1694 (`grand_campaign`).

## 3. Sound bank selection
- **CLOSED NEGATIVELY in round 11: in Napoleon.exe 1.3 nothing dispatches a bank query at all.** The
  weight accessor `0x00CB0280` has **0 callers and 0 code references** (13 refs, all DATA, all vtables);
  the entry reader `0x00E28DF0` has **exactly one reference**, its own vtable slot. A complete
  `getCallingFunctions` + `xref:` census of *all* the slots of the condition vtable `0x013A91F8`
  (`"audio_distance"`), the bank vtable `0x013A993C` (`"sound_bank_projectile_impact"`) and the bank
  entry vtable `0x013A95A8` finds **0 callers for every bank-specific slot**. The only two slots with
  callers are inherited base methods shared with unrelated subsystems (a 1-byte flag at `+4` whose callers
  are the **AI negotiator**, and an 8-byte `*(this+0x24) = 0` reset with 25 engine-wide callers). See the
  round-11 block at the end of this section for the full table and the caveat. **So the weighted matcher
  below is not just PROVISIONAL, it is probably wrong: the weight is unreachable, so 1.3 cannot be
  selecting by weight.** The most consistent reading is *first entry whose conditions all contain the
  queried value*, weight inert like `+0x08`/`+0x0C`/`+0x10` in the ACTION table.
- CONFIRMED: each condition class has a weight in its data (vtable slot 3): projectile_fire gun_type 4.0,
  shot_type 1.0, audio_distance 0.25, the 4th 0.25. **But no code reads slot 3 - see above.**
- SUPERSEDED by the round-11 verdict (the layout is kept below): the query function itself. Located on 2026-10-04:
- UNKNOWN: the query function itself (not reached through any virtual call site found so far). Located on 2026-10-04 (no
  query yet): condition class vtables, e.g. `audio_distance` at `0x013A91F8` {0 dtor, 1 parse value `0x00E26D60`, 2 add value
  `0x00E26D00`, 3 weight `0x00CB0280` = 0.25, 4 `0x0054E920` (a bool at +4), 6 name, 7 / 8 / 9 binary / XML / debug export,
  10 copy}; an entry (0x84 bytes, vtable `0x013A95A8`) holds 4 condition objects at +0x14, +0x30, +0x4C, +0x68 (stride 0x1C,
  value list count +0x14 / data +0x18 of each); bank vtable e.g. `0x013A993C` {1 entry factory `0x00E28EA0`, 2 entry
  reader `0x00E28DF0`, 4 empty entry `0x00E17110`, 5 debug export `0x00E37F90`, 6 name}. Ours: among entries whose
  non-empty conditions all contain the queried value, the highest sum of weights (known weights; 1.0 otherwise),
  ties to the earlier entry (**PROVISIONAL, and now suspected wrong - see the top of this section**).

- **Cue enum (INFERRED 2026-10-04):** for cues 27..106 the cue is the built-in slot `cue + 187`: every clip family
  matches its slot names in order (archer aim/fire = BOW_DRAW/BOW_RELEASE; the cannon, rocket and mortar crew clips =
  CANNON_BALL_LOAD..MORTAR_STOKE_OUT; carbine/musket reloads = MUSKET_RAM_SHORT/LONG; `horse_cheval_*` = HORSE_CHEVAL;
  pistol deaths = PISTOL_IMPACT_GROUND; elephant attacks = ELEPHANT_MELEE). Cues 0..26 (gallops, footsteps; up to
  6,000 uses) and 107+ (campaign ships) do not fit: UNKNOWN. No `+ 0xBB` (187) operand exists in the exe, so the
  engine probably shares one enum (not CONFIRMED). Code: `ntw_formats::sound::anim_events::cue_slot`; survey:
  `cargo run -p ntw_formats --release --example anim_cue_probe`.
- Bank query, more negative results (2026-10-04): the conditions' "add value" method (slot 2) is called only by the
  bank readers, so queries are not built as entries that way. The musket entries of `sound_bank_projectile_fire.xml` have
  no `audio_distance` condition (one entry per gun type), so the same event at close and medium range is the data, not
  our matcher.
- Cue dispatch, time-boxed search (2026-10-04, not found; the cue + 187 rule stays INFERRED). The cue files are loaded
  twice: (a) the sound side, `0x00E615A0` walks a list of animation entries (+0x70 count, +0x74 data) and `0x00E61490` builds
  the path and calls `0x00E60AD0`, which caches the parsed cue list (a 0x1C-byte container, type descriptor `0x0145B920`;
  parser `0x00E25C30`: +4 cue count, +8 the 8-byte {time, cue} entries) in a path-keyed hash map at owner +0x100 and stores it
  at entry +0xD0; (b) the battle animation side, `0x00F85780` (from the anim-fragment loader `0x00DE1E10`) loads the same file
  through `0x00F843A0` (container type `0x0145D504`, also kept in the global list `0x01650414`) and passes it to the animation
  library object made by `0x010CB060`. The consumer is therefore inside the animation library (`0x010C0000..`), which calls
  back into the game with each cue; no `+ 0xBB` / `+ 0x2EC` (187 / 187 × 4) operand was found in the sound code. The animation
  object made by `0x010CB060` keeps the cue list at +0xA0 (its 7th argument). Next lead: the readers of that +0xA0 in the
  animation library (an `+ 0xA0]` scan of `0x01080000..0x01200000` gives ~300 hits; narrow it to functions that also walk
  8-byte entries and compare a float time).
- Bank query, structural scan (2026-10-04): functions using all eight entry-condition offsets (+0x28/+0x2C, +0x44/+0x48,
  +0x60/+0x64, +0x7C/+0x80) number 580 in the exe; the evenly spaced group at `0x00E3C620`..`0x00E442D0` turned out to be
  pack/VFS readers, not bank code. Still UNKNOWN; our weighted match stays PROVISIONAL.
- `.anim_sound_event` (2026-10-04 lead for the cue enum): the extension string `0x013A0FBC` is kept in the global
  `0x0164B4D4`; its users are `0x00E61490` (stores the loaded cue list at object +0xD0 via loader `0x00E60AD0`; created from
  the sound init `0x00E20760` through `0x00E615A0`) and `0x00F85780`. The cue dispatch reads object +0xD0: next target.
- Bank query, round 4 (2026-10-04, still UNKNOWN): xrefs of the entry vtable (`0x013A95A8`) and the bank vtable
  (`0x013A993C`) are all DATA refs inside the sound module (`0x00E0A7D0` entry ctor family, `0x00E0B660`/`0x00E15D20`/
  `0x00E14D30` bank factory); the entry factory `0x00E28EA0` and entry reader `0x00E28DF0` have no direct callers
  (vtable-dispatched). Bank-name strings live only in data tables (`0x00E23680..0x00E237E0`) and the bank XML paths only
  in the factory `0x00E14D30` (cases: 0 impact, 1 fire, 2 explosion, 4 idle, …). No query path surfaced.
- Cue dispatch, round 4 (2026-10-04, still not found; cue + 187 stays INFERRED). (a) The `+ 0xA0]` scan of the animation
  library is too noisy (mostly stack locals and unrelated structs; the only CONFIRMED hit is the maker's own store at
  `0x010CB23A` in `0x010CB060`). (b) `0x010CB060`'s 7th argument = the cue list CONFIRMED at the `0x00F85780` call site
  (passes the `0x00F843A0` container; its other two callers `0x011C1CC0`/`0x00E5FF20` pass other data or 0, so the maker
  is generic). (c) The global battle-side cue list `0x01650414` has no consumer: only the loader touches it plus the
  teardown `0x00F76E30` (frees each 0x1C-byte entry and zeroes the slots; called from `0x00DED3D0`). (d) Sound-module
  `+ 0xD0]` reads are other objects' vtable slots / the bulk destructor `0x00E01CE0`, not cue-list readers.
- Cue dispatch, round 5 (2026-10-04, still not found; cue + 187 stays INFERRED). (a) Correction: `0x00DE1E10`
  is not an anim-fragment loader but a campaign anim-set builder (reads `ANIM_REFERENCE_POSE_RECORD` /
  `CAMPAIGN_WALK_ANIM_SET_RECORD` tables, called only by `0x00DDC320`); it calls `0x00F85780` per entry, and its
  `matched_action` / `action` appliers (`0x00DDF060` / `0x00DDEF50`) walk 0x28-byte entries, not cue entries. There is
  no per-frame cue walk anywhere near it. (b) `ref:` scan of both cue-list container types (`0x0145D504` battle side,
  `0x0145B920` sound side): each is referenced only by its own loader (`0x00F843A0` / `0x00E60AD0`) and the `0x010CB060`
  maker call in `0x00F85780` — both containers are write-only from exe code; the reader is inside the animation
  library. (c) `callers:` scan of all three sound play paths (`0x01005140`, `0x01000F60`, `0x01004430`): every caller
  lives in the sound manager (`0x0100xxxx`); no caller in the animation library, so the anim library does not call the
  play path directly — the dispatch must go through a game-side poll or another hop. Next lead: the game-side per-frame
  animation player that ticks the `0x010CB060` object and queries elapsed time / cues (start from the maker's other two
  callers `0x011C1CC0` / `0x00E5FF20`, or the battle per-frame update that drives the anim library).
- Cue dispatch, round 6 (2026-10-04, still not found; cue + 187 stays INFERRED; no code change). All three
  `0x010CB060` callers decompiled (Ghidra reconstructed pseudocode, scratch only): the maker takes 7 args and stores
  the 7th at object +0xA0 (CONFIRMED shape again). (a) `0x011C1CC0` passes 7th arg 0 in both branches
  (`RigidModels` static path with `_alphatest` / `_alphablend` / `_twosided` reads; called only by `0x011EEE90`,
  itself called from `0x011185C0` / `0x01229EE0` / `0x01225B70`): INFERRED static-model path, no cues.
  (b) `0x00E5FF20` passes 7th arg 0 (hash-interning maker wrapper; called only by `0x00E4F4D0`, which reads
  `FIRE/FUSE/IMPACT_POSITION`, `FACE_UP/DOWN`, `FIRE/IMPACT_TIME`, `DISTANCE`, `*_FOOT_GEAR_UP_START`,
  `ON/OFF_BONE1/2/3`, via `0x00E60660` <- `0x00E60260` <- 5 callers in `0x00E5xxxx`): INFERRED
  effect/bone-attachment table build, no cues. (c) Only `0x00F85780` passes the cue list (7th arg = the `0x00F843A0`
  container), and both its callers are campaign anim-set builders (`0x00DE1E10` <- `0x00DDC320` character sets;
  `0x00F862E0` <- `0x00F77940` <- `0x00F62760` <- `0x00F72A60` walk sets, 10 `0x00F85780` calls packed into a 0x34-byte
  struct for `0x00DD5E50`); the maker's caller list is complete at 3 (CONFIRMED by the `callers:` scan). So every
  path examined is load-time; the per-frame tick was NOT found. Next lead: xref the packed-set consumer `0x00DD5E50`
  (shared by both campaign builders) and walk up from `0x00F72A60` / the `0x00E5xxxx` callers toward the battle
  per-frame update, to find what ticks the anim object (virtual update with delta time) inside `0x010Cxxxx`.
- **Cue dispatch, round 7 (2026-10-04, sandbox worker copy; per-frame tick STILL NOT found; no code change).**
  The round-7 brief's starting premise is **REFUTED (CONFIRMED by direct reads)**: `0x01469494` and `0x0145D504` are
  **not vtables**, they are two global `UIFileIn` file-handle objects, so neither has a "slot 7 / +0x1C update".
  - Evidence: a `ptrs:` dump of both addresses yields data, not code pointers - `0x01469494` = {`0x0130BC60`
    (the `UIFileIn` vtable), pointer to the string `VEGETATION_NODE`, 0, `0xBF800000` (-1.0f), 0, 0, 0, `0x3F800000`
    (1.0f), ...}; `0x0145D504` = {`0x0130BC60`, `0xFFFFFFFF`, `0xFFFFFFFF`, `0x0000FEFF`, `0x20`, `0x5F`, `0xA0`,
    `0x200B`, ...}. Reading "slot 7" gives `0x01469494+0x1C = 0x014694B0 = 0xBF800000` and
    `0x0145D504+0x1C = 0x0000200B` - a float and a small integer, not a function address.
  - Cross-checked by usage: `xref:` gives `0x01469494` <- `0x011C20DB` / `0x011C20FB` (both inside `0x011C1CC0`) and
    `0x0145D504` <- `0x00F85901` (`0x00F85780`) / `0x00F84425` (`0x00F843A0`); all four sites are the 5th argument
    of `0x010CB060`, whose body stores it as the new object's **first member** and then reads it as a file handle
    (`*local_14 = param_5;` then `UIFileInPtr::UIFileInPtr(local_f0,(UIFileIn *)*local_14)` = the anim file source,
    not a vtable). The sound-side twin global is `0x0145B920` (passed to the same maker by `0x00E60AD0`).
    Consequence: the anim object made by `0x010CB060` is **not polymorphic at +0**; its per-frame tick therefore is
    a free function (or a manager method), not a `vtable[7]` vcall. The "update = vtable slot 7" idea is dead.
  - Confirmed shape of the cue side (all read in the exe, Ghidra pseudocode): the cue-list container is **0x1C bytes**,
    built by `0x00DF0240` = {+0 tag `1`, +4 count `0`, +8 data `0`, +0xC `UIFileIn*`}, filled by the parser
    `0x00E25C30`, freed with `0x00DFDD20` / `0x0126E016(ptr,0x1C)`. Battle side: loader `0x00F843A0` (only caller
    `0x00F85780`) pushes it into the global pointer array `0x01650414` (count `0x01650410`, capacity `0x0165040C`,
    doubling via `0x00444B40`, old array freed with `0x00FF7EF0`). Sound side: `0x00E60AD0` caches it in a string-keyed
    hash map inside its owner - bucket array pointer at owner **+0x100**, bucket count at **+0x104**, 8-byte entries
    {key, container}, hash `0x00E5F310(path) % buckets`, probe/compare `0x00E57960`, insert helper `0x00E69BF0`
    (a generic CA hash map; its key-hash sibling for a second map is `0x00E69930` with `0x00E5F2A0` / `0x00E57900`).
    `0x00E61490` stores the container at **entry +0xD0** (CONFIRMED again at instruction `0x00E6157C`).
  - Confirmed shape of the campaign walk anims: `0x00F85780` allocates the **0xA4-byte** anim object
    (`FUN_010D3140(0xa4)`) and `0x010CB060` puts the cue container at its **+0xA0** (last pointer);
    `0x00F862E0` (only caller `0x00F77940`) makes **10** of them from one `ANIM_REFERENCE_POSE_RECORD`
    (anim names at record +0x18..+0x90), packs them into a **0x34-byte** struct (13 dwords) and warns
    "WARNING: some campaign walk anims are not present or failed to load" if any is missing.
  - **Round 6's "packed-set consumer `0x00DD5E50`" lead is closed as a dead end:** `0x00DD5E50` is 25 bytes and is just
    `this->field_0xC = record` (`this` in ECX), the generic applier used by the database-table appliers - `0x00F58100`
    (`UTILITYLIB::DATABASE_TABLE<EMPIREUTILITY::EFFECT_RECORD...>`, with the "is not a valid key for this table"
    error string) and `0x00F862E0` (`...<EMPIREUTILITY::ANIM_REFERENCE_POSE_RECORD...>`). It never touches a cue list;
    the walk-anim struct is simply parked in a field. The 0x34-byte "packed set" is therefore the record applier slot,
    not a cue container set.
  - Where the battle's cue objects come from (new, INFERRED): `0x00E615F0` walks 0x10-byte battle-configuration
    records (data at +0x1C, count at +0x20), builds the path `"Animations/BattleConfiguration/" + <name>` (name = u16
    string-table index at record +0xC), calls `0x00E611E0` per record and appends the result to an array at
    this+0x18/0x1C/0x20 (same doubling growth); only caller `0x00E66F00`. `0x00E611E0` calls `0x00E62760`,
    `0x010D3140`, `0x0126E016`. Its per-frame consumer is the next thing to find.
  - More negative results for the tick (all CONFIRMED absences, scans are in the round-7 scratch output):
    (a) `call [reg+0x1C]` sites: 6286 in `0x00480000..0x00600000` and 4097 in the animation library
    `0x010C0000..0x01200000`, spread over hundreds of functions - +0x1C is a generic vtable slot, not an "update"
    marker, so slot counting cannot find the tick; (b) `+ 0xA0]` reads: 529 hits in `0x00DC0000..0x00FA0000` and 538
    in `0x010C0000..0x01220000` - all unrelated structs (round 5's noise, re-confirmed); (c) `+ 0xD0]` in
    `0x00DF0000..0x00E80000`: only the store at `0x00E6157C`, the bulk destructor `0x00E01CE0`, unrelated
    `lea ecx,[x+0xD0]` ctor chains and `[esp+0xD0]` stack slots - **no reader of the sound-side cue list**
    (round 5's negative result re-confirmed); (d) `+ 0x100]` in the sound module: only the insert `0x00E60AD0`, a
    second map's users (`0x00E50970`) and the bulk destructor `0x00E56CC0` - **no lookup/reader of the cue hash map**;
    (e) **no** function in `0x00E00000..0x00E80000` calls the engine audio layer directly (`call 0x010…` in that
    range: 0 matches), so the cue fire, if it exists in the exe, goes through a pointer/vtable hop;
    (f) the main loop `0x00485B90` (6186 bytes, 109 callees, single caller `0x0048C1B0`) is the app dispatcher: its
    per-frame body is the campaign/world branch (`0x00DE6F00`, `0x00DE7B10`, `0x00DE7300`, `0x00DEAD20`,
    `0x00DE83E0`, `0x00DE82E0`, `0x00DB8C00`); the battle branch was not isolated in this time box.
  - Next leads, in order: (1) the per-frame consumer of the `Animations/BattleConfiguration/` objects built by
    `0x00E615F0` -> `0x00E611E0` (battle cues); (2) the campaign side: whoever reads the walk-anim struct's +0xC and
    then its 10 slots with a time delta; (3) whole-exe `insn:0xa4]` scan in `0x00F00000..0x01000000`: the 0xA4-byte
    anim object size is a literal only at its allocator, so sibling allocators/iterators of the same class should
    show up; (4) the cue-time compare must live in the animation library's *player*, so search
    `0x010C0000..0x01200000` for functions whose only float argument is a delta, rather than by vcall site.
- **Cue dispatch, round 8 (2026-10-04, sandbox worker copy; per-frame tick STILL NOT found; no code change).**
  The primary lead is now closed with a *complete* xref census rather than a scan, and one of its premises is
  **REFUTED**: the global at `0x01650414` is **campaign-side**, not battle-side, and nothing reads it.
  - **Xref census of the global cue list (CONFIRMED, complete).** `0x01650414` (data pointer) = **8** refs,
    `0x01650410` (count) = **8**, `0x0165040C` (capacity) = **3**; every one of the 19 is in one of three
    functions: the writer `0x00F843A0` (4 / 5 / 3), the teardown `0x00F76E30` (3 / 2 / 0) and the process-exit
    free `0x013013B0` (1 / 1 / 0). `0x013013B0` (62 bytes) is only a loop that runs the per-object destructor
    (`guard_check_icall`) over the count and then frees the array; it is reached from a DATA ref at `0x00436B60`
    (the static-init / atexit table), not from any frame. **There is no per-frame reader of the global cue list
    anywhere in `Napoleon.exe`** - primary lead item 1 answered, negatively and definitively.
  - **`0x00F843A0` in full (398 bytes, CONFIRMED):** it resolves the path, opens it, makes the 0x1C container
    (`FUN_010D3140(0x1C)` + `FUN_00DF0240(&PTR_PTR_0145D504)`), parses it with `0x00E25C30`, appends the pointer to
    `0x01650414` and returns it. It publishes the container **nowhere else**, so its only other holder is the
    return value -> `0x00F85780` -> 7th argument of `0x010CB060`. Sole caller: `0x00F85780`. So the global is a
    keep-alive registry; the real consumer would have to be the anim object's **+0xA0**.
  - **`0x00F85780` and its callers are campaign-side (CONFIRMED):** callers are `0x00DE1E10` and `0x00F862E0`
    (rounds 5/6: campaign anim-set and campaign walk-anim builders), and the matching teardown `0x00F76E30`
    (which frees that same global, plus the 0x34-byte walk-anim struct) hangs off `0x00DED3D0` <- `0x00E07B80`
    <- `0x00E17650`. The list is not battle data - the name "global battle cue list" in rounds 4-7 is wrong.
  - **REFUTED: `0x00E611E0` / `Animations/BattleConfiguration/` are not cue data (CONFIRMED).** `0x00E611E0`
    allocates a **0x1E610-byte** object (`FUN_010D3140(0x1E610)` + ctor `0x00E50930`, which zeroes `+0xC` and runs
    `0x00E50960` 0x360 = 864 times), keyed by the **u16** passed in (linear scan of its own array at `this+0xB8`
    count / `this+0xBC` data / `this+0xB4` capacity, matching `*(short *)(obj + 0xC)`). The parser `0x00E62760`
    (6764 bytes, sole caller `0x00E611E0`) reads `version`, then `ACTION_<name>` blocks, and per action stores up to
    **5 fragments of 0x1C bytes** (`if (*(uint *)(rec + 0x8C) < 5)`) at `this + type*0x90 + 0x10`, each with
    `filename`, `blend_in_time` (float), `equipment_usage` (10 named values, `0x00E60EE0`), `primary_weapon` /
    `secondary_weapon` usage and `defensive` / `ambient` / `personal` / `special` on/off flags; the 0x360 marker is
    an empty fragment and a bad keyword calls `TerminateProcess`. So this is the **battle animation action table**
    (which anim file per action, per equipment/weapon), not a sound-event list. The per-frame player of *that* is
    still UNKNOWN, but it is an animation-structure question, not a cue question.
  - **The 0xA4 anim object, full layout (CONFIRMED, 5209-byte maker `0x010CB060`; 0xA4 = 29 dwords):**
    +0x00 = the `UIFileIn*` anim-file source (arg 5); +0x04 = arg 2; +0x08 = sub-object from `0x010CC4C0`
    (bone descriptor = arg 4, arg 6); +0x34 a second `UIFileInPtr`; +0x44 arg 3 (flags, 0x84 on the campaign path);
    +0x48/+0x4C a direction from `DAT_0182D018`, normalised in place when its length at **+0x50** is > 0;
    +0x54 length / duration; +0x58 and +0x5C arc-length differences x 9.58738e-05 (radians per unit, the short-way
    -0x8000 fixed-point wrap); **+0x60** angular velocity wrapped into +/-2pi; +0x64 angular velocity / duration;
    +0x74..0x7C a second vector triple; **+0x80** a pointer array with **+0x88** count and **+0x8C** data (the bone /
    segment list, stride 4); +0x90 a second array; **+0xA0 = the cue container (arg 7)**. So the class is a
    *bone-attached animation instance* (direction + length + angular rate + a bone list) - a weapon, piece of
    equipment or effect hung on a bone - and it is its tick that would fire the cues at +0xA0. Not found.
  - **Primary lead item 3, answered: `0x00485B90` has no battle branch.** Its complete decompile (1637 lines, 109
    callees, not truncated) contains **no call at all into `0x00F7xxxx..0x00FAxxxx`**; with the "Creating ..."
    strings it is the front-end + campaign dispatcher (`Creating campaign env (includes model)...` in `0x00485B90`
    and `0x008B8930`, `... for loaded game` `0x008B8A60`, `... for replay` `0x008B8B50`, `Creating sound` /
    `Creating Multiplayer` / `Creating Debug rendering` in its caller `0x0048C1B0`). **No "Creating battle..." string
    exists in the exe at all**, so the battle dispatcher has to be found another way.
  - **New, from string anchoring (0-A technique): the `.anim_sound_event` global `0x0164B4D4` has exactly 4 refs** -
    `0x00E6152E` (`0x00E61490`, sound side), `0x00F8587C` (`0x00F85780`, animation side), `0x00432A35`
    (`FUN_004f5500(".anim_sound_event", 0x0164B4D4)`, static init) and `FUN_004f55f0()` inside the 10-byte static
    initialiser `0x012FFF80`. There is **no third loader and no consumer**. Negative anchor too: a case-sensitive
    scan of every defined string for "cue" returns **0 hits**, so there is no cue-dispatch debug/error string to
    anchor on.
  - **Round 7 lead (3) closed, and the scan pattern was wrong:** an allocator shows up as `push 0xa4`, not as a
    `0xa4]` memory operand - the `insn:0xa4]` scans (240+ hits here, ~800 in round 7) are pure `[esp + 0xa4]` stack
    noise. The correct scan over `0x00DC0000..0x01000000` yields **4 functions only**: `0x00DDF570` (7 sites),
    `0x00E5FF20` (1), `0x00F76D10` (10) and `0x00F85780` (1). **No sibling allocator of this class exists** in the
    battle/campaign range (INFERRED: battle never instantiates it).
  - **The walk-anim set is a keyed table, not one fixed struct (refines round 7).** `0x00F76D10` (286 bytes,
    CONFIRMED) is the destructor of a 0x34-byte block holding exactly **10** 0xA4-byte anim objects at dword indices
    0, 1, 2, 3, 4, 6, 8, 9, 0xB, 0xC (each destroyed by `0x010D2C30` then freed); `0x00F76E30` frees that block from
    its owner's **+0xC** field. The builder's owner is a **name-keyed hash table**:
    `UTILITYLIB::DATABASE_TABLE<EMPIREUTILITY::CAMPAIGN_WALK_ANIM_SET_RECORD::BUILDER>` with **0xC-byte** records
    (`0x00F77940`), so walk anim sets are looked up by name, not held in one array. Round 7's "the 0x34-byte packed
    set is just a record applier slot" was half right: `0x00DD5E50` is the generic `this->field_0xC = record`
    applier, but its two real callers (`0x00F88190` "governorships_onscreen_%S", `0x00F58870`
    "random_localisation_strings_string_%S") belong to a *different* table family, not the walk anims.
  - Remaining UNKNOWNs: the per-frame tick and cue dispatch; whether 1.3 consumes these containers **at all** (both
    are provably write-only from exe code, so either the dispatch is inside the animation library reached through
    anim-object +0xA0, or the feature is dormant in this build); the battle per-frame dispatcher; the bank query.
  - Next leads, in order: (1) the anim instance's tick is the only code that walks **+0x80** with count **+0x88**
    *and* reads **+0xA0** - search `0x010C0000..0x01220000` for functions touching both offsets (this is the first
    *paired-offset* criterion in this search and should be far less noisy than the +0xA0-only scans); (2) find the
    battle dispatcher from the battle HUD/interface code or from `0x00F7xxxx` entry points, not from the app main
    loop; (3) a data-side check on the shipped `.anim_sound_event` files (are the cue lists non-empty?) to decide
    whether to keep hunting or mark the feature dormant.
- **CUE DISPATCH: CLOSED, EXE-SIDE DEAD (2026-10-04 round 9, sandbox worker copy). Do not re-open this thread.**
  The answer is not "not found yet", it is *there is no such code path*, and it is now proved three ways.
  - **(a) The `+0xA0` READ scan (what round 8 asked for).** Over `0x010C0000..0x01220000` there are 216
    register-indirect `[reg + 0xa0]` sites; none of them is this class. The only store to *this* class's `+0xA0`
    is `0x010CB23A` (round 7's maker `0x010CB060`) and the only load is its own argument use. The class's
    destructor does not touch it either: `0x010D2C30` (279 bytes, 4 callers `0x00DDF570`, `0x011E33D0`,
    `0x00F76D10`, `0x00E56CC0`) walks the bone array, the second array and a third array and stops at **+0x9C**.
    So the cue container at +0xA0 is **not owned by the class** - it is a borrowed pointer into the global
    keep-alive registry `0x01650414`.
  - **(b) Round 8's paired-offset lead (1) is REFUTED as a filter.** Intersecting the functions that touch
    **+0x80**, **+0x88** and **+0xA0** over the animation library gives **78** functions, not one. It is not a
    discriminator and should not be repeated.
  - **(c) The decisive one: nothing outside the sound manager can start a sound.** `getCallingFunctions` on the
    three bank-event play paths is a *complete* static census: `0x01005140` <- {`0x01005840`, `0x01007B00`};
    `0x01000F60` <- {`0x01005140`, `0x01004F80`, `0x01004E50`}; `0x01004430` <- {`0x010075F0`}. **Six call sites,
    every one of them inside `0x0100xxxx` (the sound manager).** No function in the animation library, in
    `0x00E0xxxx` (sound), in `0x00DCxxxx..0x00FAxxxx` (battle/campaign) or anywhere else calls any of them. So a
    cue could only be fired from inside the sound manager, and rounds 4/5/7 established that the sound manager
    has **no reader** of a cue container (object +0xD0: none; the +0x100 cue hash map: no lookup).
  - **Conclusion.** In 1.3 the `.anim_sound_event` lists are parsed, cached and never read: the campaign-side
    global `0x01650414` is a keep-alive registry, the sound-side hash map is insert-only, and no code path
    exists from an animation, battle or campaign frame to a sound start. **Implication for §0: stop treating
    `.anim_sound_event` cues as a dispatch problem.** They are a data-side artefact (we may still read them for
    authoring and for cross-checking), the `cue + 187` mapping stays an INFERRED data inference, and *no
    engine-side code should be written to play them.* Battle animation **sound** fidelity must instead come from
    the action table below, which is per-fragment and equipment-driven.
  - Correction to round 8's 0xA4 layout, from `0x010D2C30`: the arrays are block **+0x80** with count **+0x88**
    and data **+0x8C** (stride 4, elements polymorphic), block **+0x90** with count **+0x98** and data **+0x9C**,
    and a third at data **+0x40** with count **+0x3C** and stride **0x14**. Round 8's "+0x90 a second array" was
    loose; +0x90 is the block pointer and its count is at +0x98.
- **Battle animation ACTION table - the contract, CONFIRMED (2026-10-04 round 9).** Round 8 identified
  `0x00E611E0` / `0x00E62760` but nobody had characterised them. Characterised now; this is the item §0 wanted.
  - **Class identity (string-anchored).** The string
    `UTILITYLIB::DATABASE_TABLE<class EMPIREBATTLE::ENTITY_ANIMATION_ACTION_TABLE, ...>::record_index` at
    `0x01335570` has exactly **one** referrer, `0x0061CB90` - the **battle entity constructor** (2074 bytes, 7
    callers). So the table belongs to a battle entity, and `0x0061CB90` is where an entity's action table is set
    up. (`EMPIREUTILITY::CAMPAIGN_ANIM_ACTION_TO_SET_RECORD::BUILDER` at `0x0139CDF0` <- `0x00DDB890` is the
    campaign counterpart.)
  - **`0x00E611E0` in full (419 bytes, CONFIRMED).** Linear scan of the owner's array for a **(path, u16 key)**
    hit: capacity **+0xB4**, count **+0xB8**, data **+0xBC**; miss allocates `0x1E610` and runs ctor
    `0x00E50930`; then parser `0x00E62760(path, key)`; success appends with doubling (`0x00444B40`, old array
    freed `0x00FF7EF0`), failure runs the per-slot destructor **0x360 = 864 times** and frees with
    `0x0126E016(obj, 0x1E610)`. Ctor `0x00E50930` stores the u16 key at **+0x0C** and runs the per-slot
    initialiser `0x00E50960` 0x360 times.
  - **Geometry (CONFIRMED):** `0x10 + 864 * 0x90 == 0x1E610`. Each action slot gets one **0x90**-byte block,
    addressed as `this + 0x10 + slot_index * 0x90`, holding **five 0x1C-byte fragments** and a **u32 count at
    block +0x8C**. A sixth fragment is the fatal "Max entries exceeded for type '...'".
  - **The 0x1C fragment, field by field (CONFIRMED from the parser's own stores):**
    **+0x00** u32 *tag* - `0x360` while the fragment is empty, overwritten with the **slot index** when the
    fragment is materialised; **+0x04** a three-word `CA::UniString` *filename* (clip path); **+0x10** f32
    *blend_in_time*; **+0x14** u32 *equipment_usage*; **+0x18** u16 *flags*. The last two bytes are never written.
    `blend_in_time` is defaulted to **1.0f** (`0x3F800000`) on materialisation, so a line that does not say it
    still blends for a second.
  - **Grammar (CONFIRMED, with the exe's own fatal messages):** `version = <int>` once ("Expecting keyword
    'version'" / "Expecting version number"), then per slot, per fragment: `filename = <path>`
    ["Expecting filename"] [`blend_in_time = <float>`] [`equipment_usage = <name>`] [`primary_weapon` |
    `secondary_weapon` | `defensive` | `ambient` | `personal` `= on | off`] [`special = <name>`] `,`.
    `=` is `0x0131328C`, `,` is `0x013343B4`, `"on"`/`"off"` are `0x013B8500`/`0x013B8504`. The five on/off
    keywords are **bits 0..4** of the flag word in that order (primary_weapon 0x1, secondary_weapon 0x2,
    defensive 0x4, ambient 0x8, personal 0x10); `= off` clears the bit, `= on` sets it, and anything else is
    "Expecting 'on' or 'off'". `special` **ORs** a bit in, so one fragment may name several.
  - **`equipment_usage` is a closed set of ten (CONFIRMED, `0x00E60EE0`):** 0 `none`, 1 `rifle`, 2 `rifle_butt`,
    3 `rifle_bayonet`, 4 `sword`, 5 `axe`, 6 `pike`, 7 `lance`, 8 `longsword`, 9 `shield`.
    **10 = unrecognised** (and the value a fresh fragment is pre-filled with) and **11 = the `cancel` marker** -
    not an eleventh usage. `cancel` is only legal when it is the slot's first fragment ("Cancel must be only
    entry in fragment for slot type '...'"), and it leaves the tag at `0x360` so the fragment is skipped. A
    materialised fragment with no filename is "Filename required"; a cancelled one with a filename is
    "Filename supplied for a cancelled entry????" - both fatal.
  - **`special` is a closed set of ten (CONFIRMED, `0x00E69EB0`):** 0x0020 `musket_ramrod`, 0x0040
    `cannon_ramrod`, 0x0080 `arrow_man`, 0x0100 `arrow_rider`, 0x0200 `axe`, 0x0400 `cannon_ball`, 0x0800
    `rocket`, 0x1000 `wooden_stake`, 0x2000 `grenade`, 0x4000 `bayonet`; **0x8000 = unrecognised**, and the
    reader ORs whatever it returns in, so a bad name marks the fragment rather than being ignored.
  - **`ACTION_<name>` -> slot index: the resolver is `0x00E5FA30` (CONFIRMED, 240 bytes, sole caller
    `0x00E62760`).** It hashes **864 descriptors of 0x18 bytes at `0x013AEBE0`** into a string map on first use
    (buckets `0x0145BAD4` / count `0x0145BAD8`, one-shot flag `0x0164BEDC`, atexit `0x01300510`; insert
    `0x00E694F0`, find `0x004CE060`, string hash `0x004C9FF0`), then returns the found entry's **+0x00**.
    **For all 864 entries `entry[i].index == i`** (checked, 0 mismatches), so the table order *is* the slot index;
    `+0x04` is the name. An unknown name is the fatal "Unrecognised animation type '...'". Compare is `strcmp`,
    so it is **case sensitive**.
  - **The `ACTION_` literal has exactly ONE referrer, `0x00E62760`** - the parser only checks the prefix and
    then resolves the suffix, so **the 864 action names are not in the exe as literals**; they live in that one
    0x18-stride table. All 864 are now transcribed (all unique, all `[A-Z][A-Z0-9_]*`): 0 `MISSING_ANIM`,
    1 `STAND`, 2 `STAND_TO_STAND_TRAINED`, 3 `STAND_TO_WALK`, 4 `STAND_TO_COMBAT_READY`, 5 `STAND_TO_PRONE`,
    10..14 `WALK_1..WALK_5`, 15 `WALK_TO_STAND`, 126 `AIM`, 130 `FIRE`, 229 `COMBAT_1`, 311 `COMBAT_83`,
    863 `PIKEMEN_CHARGED_FRONT_THRUST_2`. (20 of them sit in a `.rdata` run Ghidra had not defined as strings -
    `WALK_1..5`, `RUN_1..5`, `TROT`, `AIM`, `FIRE`, `COMBAT_83/85/88`, `SNEAK`, `JUMP1..3` - and were read raw
    from the exe: image base `0x00400000`, `.rdata` VA `0xF07000` / raw `0xF05E00`, read-only.)
  - **The other four dwords of a name-table descriptor are UNKNOWN.** `+0x08` is a small category code (32
    distinct values, 0..27; 0 = the stance/idle family, 2 = the WALK/RUN/TROT locomotion family, 15 = the
    `COMBAT_n` family, 20 = deaths, 22 = the rider family). `+0x0C` and `+0x10` look like **link indices** with
    `864` = "none" (e.g. 427 `RIDER_STAND` has `+0x0C` = 427 and `+0x10` = 1 = `STAND`; 428 `RIDER_STAND_TO_WALK`
    has 427 and 3 = `STAND_TO_WALK`) - **INFERRED**, and `+0x14` is a bool (708 set, 156 clear, the clear ones
    including 3..18 = the transitions and the gait family) - **UNKNOWN**.
  - **Code landed** (contract CONFIRMED, so it is written down): `crates/ntw_formats/src/battle_animation.rs`
    gains `ACTION_SLOT_COUNT` / `ACTION_SLOT_BYTES` / `FRAGMENT_BYTES` / `MAX_FRAGMENTS_PER_SLOT` /
    `EMPTY_FRAGMENT_TAG` / `DEFAULT_BLEND_IN_TIME`, a `fragment_field` offset module, the `EquipmentUsage` and
    `SpecialUsage` closed enums with their two sentinels, the five `FLAG_*` bits + `DISPLAY_FLAGS`, `ON_OFF`,
    the 864-name `SLOT_NAMES` table and `action_slot()`. **No behaviour change** to `Fragment::parse` - it stays
    the tolerant reader it was, and the enums are there to validate against.
  - Still UNKNOWN here: the meanings of the four descriptor dwords above, and whether the five fragments per
    slot are alternatives or a priority list (the exe stores them in file order and our reader already treats a
    later fragment as replacing the slot - that stays INFERRED). Both were closed in round 10 below, and the
    reader's prose was corrected and the rule written down in round 11.
- **The per-frame consumer of the ACTION table: FOUND (2026-10-04 round 10, sandbox worker copy).** Round 8's
  open lead 1 and round 9's "still UNKNOWN" are both answered. Method: complete `getCallingFunctions` census of
  the loader *and* of the consumers' own fields, not an offset scan (the `0x218]` scan alone returns 216 hits and
  is useless; the census is what worked).
  - **The loader chain is load-time only and is now complete at every link (CONFIRMED, `getCallingFunctions`).**
    `0x00E611E0` <- **1** caller `0x00E615F0`; <- 1 caller `0x00E66F00`; <- 1 caller `0x00E50970`; <- 1 caller
    `0x00E10170`; <- **7** callers (`0x00E10390`, `0x0064DAF0`, `0x00E10210`, `0x00E10290`, `0x00E10310`,
    `0x00E10410`, `0x00E20760`). So the table is built once, lazily, and cached; **no frame is in that chain.**
    `0x00E10170` (160 bytes) is the accessor: it caches the manager at owner **+0x6D0**, logs
    `"Loading database: %s\n", "battle_entity_animation_table_manager"`, and roots the path at
    `"Animations/Animation_Tables/"` - the `Animations/BattleConfiguration/` prefix is added per record inside
    `0x00E615F0`. The manager is a **0x110-byte** object whose arrays are the loader's dedup cache
    (cap **+0xB4** / count **+0xB8** / data **+0xBC**, matching round 9) plus three more.
  - **The name-map buckets are a closed census too, and they are dead outside the resolver (CONFIRMED).**
    `0x0145BAD4` has **5** refs, `0x0145BAD8` **3**, `0x0164BEDC` (the one-shot flag) **2** - and **every one is
    inside `0x00E5FA30`** except two at the atexit stub `0x01300510`/`0x01300511`. Nothing reads the map per
    frame; it exists only to turn `ACTION_<name>` into a slot index at parse time.
  - **The bridge from a battle entity to its table (CONFIRMED, the key new fact).** The battle entity constructor
    `0x0061CB90` (2074 bytes, 7 callers) stores the entity's action table at **`this + 0x218`** (dword index 0x86,
    instruction `0x0061CDCC`). It gets there by: `iVar8 = *(int *)(*(int *)(*(int *)(this + 0x28) + 8) + 0xF0)`
    -> a record table with count **+0x0C** / data **+0x10**; `FUN_0047AA40(this + 0x7F)` is the name->u16 index
    find (its miss is the sole `record_index` referrer, confirmed again); then
    `*(int *)(*(int *)(iVar8 + 0x10) + index * 4)` is bounds-checked against `*(iVar8 + 0x0C)` and stored at
    **+0x218**. `0x0061CDCC` is the **only** writer of `+0x218` in `0x0060xxxx..`; every other site in that
    address family is a read or a different class.
  - **The consumer chain (CONFIRMED, all by census).** Three functions, in order:
    1. **`0x006618D0` (994 bytes, 1 caller `0x0066B990`)** - *resolves a facing angle to a slot index*. Takes a
       `short` bearing and returns one of **53..58** (`0x35`..`0x3A`), reading `*(int *)(*(int *)(**(int **)
       (this + 0x218) + 0x2C) + 0x14 + slot * 0x18)` as a **predicate** at slots 53..58 and picking the first
       one the entity actually has. Exact code at `0x00661971`: `MOV EAX,[ESI+0x218]` / `MOV EAX,[EAX]` /
       `MOV EAX,[EAX+0x2C]` / `CMP dword ptr [EAX+0x56C],0x0` - so the double indirection is real and `0x56C`
       = `0x14 + 58 * 0x18`. Bearing wrap is `-0x8000` short-safe (`0x8000` on overflow), and the angle scale
       is **9.58738e-05 rad per unit**.
    2. **`0x006613A0` (742 bytes, 0 direct callers)** - *picks the slot whose clip best matches a wanted
       direction*. Iterates a **contiguous slot range** `param_2[0] .. param_2[0]+param_2[1]`, skips slots whose
       `+0x14` predicate is 0, resolves each surviving slot through **`0x00E5F760`** and reads the clip's
       `+0x9C` / `+0xA4` direction, then keeps the **maximum dot product** against the wanted direction (with the
       entity's own facing rotated in by `cos`/`sin` of the short bearing). Falls back to the first slot with the
       predicate set, and `0x360` is the "none" result. It uses an **LCG** (`*0x343FD + 0x269EC3`, the MSVC
       `rand` multiplier) stored at `*(this + 0x28) + 8 + 0x50`.
    3. **`0x00E5F760` (53 bytes, 43 callers)** - *the per-frame fragment resolver*, `resolve_clip(slot, selector)`.
       Exact code: `ESI = this + 0x2C`; `ECX = *(int *)(ESI + (3*selector)*8 + 0x14)` =
       `*(int *)(ESI + 0x18*selector + 0x14)`; if that is **0** it divides `arg1` by **`[ESI + 0x14]`** (selector 0's
       count) and returns `*(int *)(ESI + rem*4)`; otherwise it divides by the count and returns
       `*(int *)(ESI + (rem + 6*selector)*4)`. **This settles round 9's open question: the five fragments are
       ALTERNATIVES, not a priority list** - the fragment is chosen by `selector % fragment_count`, i.e. a
       *modulo over the count*, and each fragment group has its own count at `+0x14`. **Our "a later fragment
       replaces the slot" reading is REFUTED**; the file order is preserved and the exe indexes into it.
       (Round 11 correction to this entry: the parameter called `selector` here is the **slot**, and `arg1` -
       the dividend - is the entity's `+0x1E0`. Confirmed at both call sites; see the round-11 block.)
    4. The **play** entry point is **`0x006666E0` (79 bytes, 26 callers)**: `(this, slot, mode, restart, blend)`.
       If `mode == 1` it calls `0x006595A0(1)`; if `*(this+0x18) == 0 || restart` it stores `blend` at **+0x448**,
       the resolved handle from **`0x007F1730`** at **+0x444**, and zeroes **+0x1C0**. **`0x007F1730` (24 bytes,
       9 callers)** is the actual clip lookup: `queue[slot]`, falling back to `queue[0]` when the slot is empty -
       and the queue is filled by **`0x007EE760`** (240 bytes, 1 caller `0x006631A0`) as `{handle, time}` 8-byte
       pairs with `time = *(this+0x14) + delta`. **`0x006631A0` (663 bytes, 13 callers including the entity
       constructor)** is the **stance/state driver**: it switches on `param_1` (0..7, 0x3C, 0x3D) to a
       `DAT_014520B0`-rooted slot-range pair, queues the clips whose `+0x14` predicate is set, resolves the
       transition with **`0x00817BB0`**, and calls `0x006666E0` - with a `*(int *)(iVar1 + 0x8C) != 0x360` guard,
       which is the **`EMPTY_FRAGMENT_TAG`** test round 9 found, now shown to be the *play* guard.
    5. **`0x00817BB0` (90 bytes, 1 caller `0x006631A0`)** - the **state-transition chooser**: index
       `from*0x49 + to` (**0x49 = 73 states**), count at `this + 0x18 + idx*0x1C`, **8-byte weighted entries**,
       picked by `((rand >> 16) * count) / 0xFFFF` with a `CMOVNC` clamp, returning `&DAT_01454398` (a zero
       entry) when the count is 0. **So transition clips are weighted-random, not first-match.** Sibling
       `0x00817C10` is the matching writer, and the zero-entry return value is what `0x006631A0`'s `0x360` test
       catches.
  - **`+0x2C` is a runtime array of 0x18-stride descriptors, and it is a per-entity-table copy of the name
    table (CONFIRMED, `0x00E5F760` + `0x006618D0` + `0x0065FB00` all index it identically).** It is reached as
    `**(entity + 0x218) + 0x2C`, i.e. the entity's table pointer is itself indirected. So the entity holds a
    pointer to a table whose `+0x2C` is an array of the 0x18-byte descriptors, and the readers test
    `descriptor[slot].+0x14`.
  - **The four uncharacterised descriptor dwords (task 3), resolved (CONFIRMED by census).**
    `xref:0x013AEBE0` returns **exactly 2** refs, **both inside `0x00E5FA30`** - the resolver. So in this build the
    static name table is read by **one function only**, and `+0x08` (the 32 category codes) is **never read by any
    exe code**. Round 9's "find what reads `+0x08`" therefore has a well-evidenced negative answer: **nothing.**
    `+0x08` is a **build-time/data-side category enum** (28 used values, 0..27; 0 = stance/idle, 2 = locomotion
    WALK/RUN/TROT, 3 = transitions, 10 = ?, 17/18 = the `STAND_TO_*` family, 19 = the `RUN_*`/gait family,
    20 = deaths, 22 = rider) - **CONFIRMED as an enum of 32 codes, UNKNOWN as to its names**, and it is
    documentation-grade in 1.3, not behaviour. `+0x0C` and `+0x10` are **INFERRED link indices** and, like
    `+0x08`, are read by nobody but the resolver, which uses only `+0x00` and `+0x04` - so they are also inert
    here. **`+0x14` is the one that matters**: it is read at runtime by `0x006613A0`, `0x006618D0`,
    `0x0065FB00`, `0x0064D6F0` and `0x00E5F760` as a **per-slot availability predicate / fragment count**, which
    is why 708 entries are non-zero (the slots the entity actually has clips for) and 156 are clear (3..18, the
    transitions and gait slots that only exist as sources). **Round 9's "bool" reading of `+0x14` is correct and is
    now its purpose: "does this entity have a clip in this slot".**
  - **Battle entity animation plumbing, for the record (CONFIRMED offsets on the entity, `this` = `0x0061CB90`'s
    object):** +0x28 a pointer to the entity's type/record block (the RNG seed is at `+0x50` inside it),
    **+0x1E0** the *selector* passed to `0x00E5F760` (the equipment/weapon code that picks the fragment),
    **+0x218** the action table, +0x1D8 a small enum the facing resolver gates on (`< 4`, `== 4`), +0x100 the
    short facing, +0x1C0 the elapsed time, +0x444 the resolved clip handle, +0x448 the blend time, +0x46 the
    "combat" byte and +0x117 a byte the driver tests. `0x0066CE00` (2236 bytes, **0 direct callers** - reached
    through a vtable) is the **per-frame battle entity update** and it calls both `0x006666E0` and
    `0x00E5F760`; this is the per-frame consumer at the top of the chain that §0 wanted.
  - **Bank query (task 5): still UNKNOWN, and string anchoring did not find it this round.** `str:bank` returns
    only two hits, both unrelated to the query: `0x01310798` (`"Raw data path, in which the sound events, sound
    bank files are found"`, in a settings reader `0x00405190`) and `0x0131366C`
    (`"sounds_packed\sound_bank_database"`, in the app init `0x0048C1B0`). `str:condition` returns only campaign
    `victory_condition*` strings. The weight accessor `0x00CB0280` (7 bytes, a field return) has **0 callers** -
    it is reached only through the condition vtable, so **the scorer is vtable-dispatched and has no direct call
    site**, which is why every round's `callers:` attempt has failed. The entry reader `0x00E28DF0` (167 bytes)
    also has **0 direct callers** (vtable-dispatched), confirming round 4. **New lead:** the scorer must be the
    function that *iterates a bank's entries and calls the conditions' slot-3 virtuals*; since it is vtable-only,
    find it by the **vtable** (`0x013A95A8` entry / `0x013A993C` bank) rather than by call graph - i.e. look for
    the caller of the entry **count** accessor (slot 0/1) which a scorer must read before looping, or scan for
    `call [reg+0xC]` inside `0x00E0A7D0..0x00E17000` only (not the whole module).
  - **Code landed this round (tests only, no behaviour change):** `battle_animation.rs` gains 6 test functions
    covering the two `equipment_usage` sentinels (10 unrecognised / 11 cancel), the `special` sentinel `0x8000`
    and its OR-in behaviour, the `0x360` empty tag vs a real slot index, `blend_in_time` defaulting to 1.0
    (including the f32 bit pattern), `action_slot()`'s `strcmp` case sensitivity across the range, and a
    spot-check that `SLOT_NAMES[i]`'s index is `i` at 0, 1, 432 and 863 plus an all-864 name-shape check.
    `cargo test -p ntw_formats --lib`: **152 passed, 0 failed** (was 146). No Rust engine behaviour written:
    the fragment-selection rule is now CONFIRMED but needs the clip queue and the state machine to be usable.
  - **Still UNKNOWN after round 10:** the names of the 32 `+0x08` category codes and the meaning of `+0x0C` /
    `+0x10` (all three are inert in 1.3 - CONFIRMED unread, so this may be unanswerable from the exe); the
    73-state enumeration behind `0x00817BB0`'s `from*0x49 + to`; who builds the `+0x2C` runtime array (its
    writer is in `0x00DFxxxx`/`0x00E5xxxx` but not yet isolated - the `+0x2C` register-indirect scan returns
    216 false positives again, the same trap as `+0xA0`); and the bank query (**answered negatively in round 11
    below - it is not unfound, it does not exist**).
- **Round 11 (2026-10-04, sandbox worker copy): the fragment rule re-read at instruction level and FIXED in our
  code; descriptor `+0x14` exposed as `has_clip` and locked to the 708/156 census; the bank query is closed
  negatively.** Evidence `ghidra_evidence/0c/ghidra_out_r11{a,b,c,d,e,f}.txt`, targets `targets_r11*.txt`.
  - **The fragment rule, CONFIRMED from the exact 53 bytes of `0x00E5F760` (this is what was wrong in our
    code).** Full instruction listing read this round:
    `MOV EAX,[ESP+0x8]` / `XOR EDX,EDX` / `MOV ESI,[ECX+0x2C]` / `MOV EDI,[ESP+0xC]` /
    `LEA ECX,[EDI+EDI*2]` / `MOV ECX,[ESI+ECX*8+0x14]` / `TEST ECX,ECX` / `JNZ +0xB` /
    `DIV dword ptr [ESI+0x14]` / `MOV EAX,[ESI+EDX*4]` / `RET 0x8`, else `DIV ECX` /
    `LEA EAX,[EDX+EAX*2]` / `MOV EAX,[ESI+EAX*0x4]` / `RET 0x8`. Read as C:
    `uVar2 = *(uint *)(A + 0x14 + p1 * 0x18); if (uVar2 == 0) return *(uint *)(A + (p2 % *(uint *)(A + 0x14)) * 4);
     return *(uint *)(A + (p2 % uVar2 + p1 * 6) * 4);` with `A = *(this + 0x2C)`, one unsigned `DIV`.
    So **`alternative = index % count`**, and the five fragments of a slot are ALTERNATIVES. Two
    consequences the prose had wrong: a later fragment does **not** replace an earlier one, and the count
    being read is the group's own `+0x14` with a **fallback to group 0** when it is 0 (`TEST`/`JNZ`). `0x18`
    = 24 = 6 dwords = **five clip handles plus the count**, so the runtime group is `{5 handles, count}`.
    `p1 * 6` in the second return is the same 24-byte stride.
  - **Which argument is which, CONFIRMED at both call sites (this corrects round 10's "selector" gloss).**
    `0x006613A0` and `0x0065FB00` both call `FUN_00e5f760(<candidate slot>, *(in_ECX + 0x1e0))`, so **the
    group being indexed is the action slot and the dividend is the entity's `+0x1E0` field** - one value
    per entity, not per slot. `0x006613A0`'s body also re-reads the gate as
    `*(int *)(*(int *)(**(int **)(in_ECX + 0x218) + 0x2c) + 0x14 + slot * 0x18) != 0` before each call,
    and its fallback loop strides `piVar4 += 6` ints = 24 bytes, independently confirming the `0x18` group.
    So the cycle is: pick the slot, read that slot's fragment count at `+0x14`, index it with
    `entity+0x1E0 % count`. **Round 10's "`0x1E0` = the equipment/weapon code that picks the fragment" is
    right; round 10's placement of the modulo on the wrong side is not.**
  - **`0x006618D0`'s facing scan, CONFIRMED in full (994 bytes).** It reads the gate at
    `A + 0x50C`, `A + 0x53C`, `A + 0x554`, `A + 0x56C`, `A + 0x584`, `A + 0x524`, `A + 0x584` and returns
    `0x35..0x3A` = **53..58**, i.e. `A + 0x14 + slot * 0x18` for slot 53..58 - the same count field the
    modulo divides by, read as a predicate. Gated on `*(this + 0x1D8) < 4` / `== 4`.
  - **The shipped `+0x14` census, measured this round (not transcribed): all 864 dwords read out of
    `0x013AEBE0` at stride `0x18` offset `0x14`. Values are exactly `0` or `1` - no third value exists.
    **708 ones, 156 zeros**, in 32 disjoint runs. The 156 are the transitions into and including the
    locomotion slots (`10..46` = `WALK_1` .. `TURN_RIGHT_180`), the six `TURN_*_TO_WALK_*` turns
    `53..58` that `0x006618D0` scans, the trained / stand-for-stoke / stand-no-weapon gait and transition
    families, the rider `*_TO_WALK_*` turns, `ENGINE_LIMBERED_*`, `SNEAK`, `STOKE`, `NAVAL_STOKE`,
    `MORTAR_STOKE`, `CHARGE`, `RETURN_TO_FIRING_POSITION`, `RELOAD_1`, `KNEEL_RELOAD`,
    `STAND_IDLE_11_TELESCOPE`, the crouch-to-walk turns, `FALL_FLAILING1`, `GRAPPLE_SWING`,
    `GRAPPLE_PULL`, `KNOCKED_FLYING_1` and `CLIMB_*` (`787..804`). **Correction to round 9's "the clear
    ones including 3..18": the truth is 3, then a gap of 4..9 (the `STAND_TO_*` family is SET), then
    `10..46`.** The other dwords re-measured for the record: `+0x00` has exactly one zero (slot 0, so
    `entry[i] == i` holds for all 864), `+0x0C` and `+0x10` are never zero, `+0x08` has 810 non-zero over
    28 distinct values - confirming round 10's "32 codes, 28 used" only loosely; the exact distinct-value
    census is not re-derived here.
  - **Bank query (task 4): the vtable hunt is DONE and the answer is NEGATIVE.** The two vtables were
    dumped in full (`ptrs:` over 20 dwords each; slot 15+ of both is a string, so they end at slot 14/7).
    Complete `getCallingFunctions` census of **every** slot, then a complete `xref:` census:
    - condition vtable `0x013A91F8` - slot 6 `0x00E2EA80` returns the literal `"audio_distance"`, so this
      IS that class (CONFIRMED). Slots: 0 `0x00E0AB30`, 1 `0x00E26D60` parse value, 2 `0x00E26D00` add
      value, 3 `0x00CB0280` weight, 4 `0x0054E920`, 5 `0x00445100`, 6 `0x00E2EA80`, 7 `0x00E35570`,
      8 `0x00E33CA0`, 9 `0x00E386E0`, 10 `0x00E04E00`. **Callers: slot 2 = 11 (all load-time bank
      readers `0x00DFCxxx` / `0x00E263C0`), slot 4 = 4, every other slot = 0.** Slot 4 is a 4-byte
      `return *(u8 *)(this + 4)` whose callers are `0x0096E8C0`, `0x00968300`, `0x0096F280`,
      `0x00A14850` - and those functions are the **AI negotiator** (`"InitialiseNegotiation"`,
      `"auto_restore_camera"`), not sound at all. It is a shared base accessor, not bank logic.
    - bank vtable `0x013A993C` - slot 6 `0x00E237C0` returns `"sound_bank_projectile_impact"` (CONFIRMED
      the class). Slots: 0 `0x00E0B660`, 1 `0x00E28EA0` entry factory, 2 `0x00E28DF0` entry reader,
      3 `0x006F3C00`, 4 `0x00E17110` empty entry, 5 `0x00E37F90` debug export, 6 `0x00E237C0` name.
      **Callers: slot 3 = 25, every other slot = 0.** Slot 3 is `*(int *)(this + 0x24) = 0`, an 8-byte
      **reset**, and its 25 callers span `0x0055xxxx`..`0x0057xxxx`, `0x006Dxxxx`, `0x006Fxxxx`,
      `0x00819430`, `0x00A4xxxx`..`0x00A7xxxx` - i.e. another shared base method, not bank logic.
    - bank entry vtable `0x013A95A8` - it holds **two** 10-slot vtables back to back (`0x00E0A7D0`,
      `0x00E33AE0`, `0x00E32DF0`, `0x00E352F0`, `0x00E381D0`, `0x00E384E0`, `0x00445100`, `0x006CB710`,
      `0x00E04D30`, `0x00E0A710`; then `0x00E33AE0`, `0x00E32DB0`, `0x00E35280`, `0x00E381A0`,
      `0x00E384A0`, `0x00445100`, `0x006CB710`, `0x00E04CF0`, `0x00E0A6B0`, `0x00E33AE0`).
      **All 20 functions: 0 callers.**
    - **The `xref:` census is the decisive part.** `0x00CB0280` (the weight) has **13 references, all
      DATA**, and they are all vtables: a cluster at `0x0138A874..0x0138AAC4` (the *other* condition
      classes) plus `0x013A91BC` / `0x013A9204`. `0x00E28DF0` has **exactly one reference**, its own vtable
      slot at `0x013A9944`. `0x00E28EA0` -> one (`0x013A9940`). `0x00E26D60` -> one (`0x013A91FC`).
      `0x00E237C0`, `0x00E17110`, `0x00E37F90` -> one each. Every bank-entry virtual -> vtable slots only.
      Ghidra *does* resolve `COMPUTED_CALL` references where the target is provable - and it produced 11 of
      them for `0x00E26D00` (the add-value slot), all inside the readers - while producing **none at all**
      for the weight, the value comparator, the entry reader, the entry factory or any bank-entry virtual.
    - **Conclusion, and it is the same shape as the round-9 cue verdict:** in Napoleon.exe 1.3 **nothing
      dispatches a bank query.** The banks and their conditions are loaded, the weights are parsed into the
      objects, and no code path reaches any of it. Caveat stated honestly: a `call [reg]` through a vtable
      pointer held in memory produces no reference in Ghidra, so "no reference" is not by itself "no code";
      but combined with "0 direct callers" on all 15 bank-specific slots and with round 3's `+0xC`
      observation, the census is complete and negative. **Consequence for §3 and for our code: the
      weighted matcher is not merely PROVISIONAL, it is probably WRONG** - the weight is unreachable, so
      1.3 cannot be selecting by weight. The most consistent reading is *first entry whose conditions all
      contain the queried value*, with the weight field inert like `+0x08`/`+0x0C`/`+0x10` in the ACTION
      table. **PROVISIONAL** (a direct `mov eax,[ecx+4]` field read bypassing the accessor cannot be
      excluded, though the existence of the accessor argues against it). **Stop re-running the bank-query
      hunt; it is not a search problem.**
  - **Code landed (behaviour of the *reader* unchanged, but the documented rule corrected):**
    `battle_animation.rs` - the module docs no longer claim "a later fragment that names a slot replaces
    that slot"; `fragment_index(index, count) -> Option<usize>` is the rule, `fragment_index_with_fallback`
    folds in the group-0 fallback, `Fragment::fragment(slot, index)` applies it, `resolve`/`ResolvedClip`
    docs now mark the *table-level* "last fragment file wins" merge as PROVISIONAL and explicitly not the
    exe's rule; `DESCRIPTOR_BYTES`, `descriptor_field::{INDEX,NAME,CATEGORY,LINK_A,LINK_B,HAS_CLIP}`,
    `SLOTS_WITH_CLIP` (708), `SLOTS_WITHOUT_CLIP_COUNT` (156) and `SLOTS_WITHOUT_CLIP` (32 ranges) plus
    `has_clip(slot) -> bool`. **+5 test functions** (`fragments_of_one_slot_are_alternatives_chosen_by_modulo`,
    `a_repeated_slot_line_adds_an_alternative_and_never_replaces`,
    `the_slot_resolver_falls_back_to_the_base_group`,
    `has_clip_predicate_matches_the_shipped_table`,
    `a_slot_block_holds_five_alternatives_and_their_count`) and
    `SpecialUsage::from_keyword` case sensitivity added to the round-10 sentinel test.
    `cargo test -p ntw_formats --lib`: **157 passed, 0 failed** (was 152). No new clippy warnings in the
    file (54 crate warnings, down from 56; the one remaining `battle_animation.rs` warning at line 1227 is
    round 10's). **No engine behaviour written** - the rule is CONFIRMED but the clip queue, the state
    machine and the runtime `+0x2C` array are still missing.
  - **Did a test lock in the bug? NO - and that is worth stating.** No test ever asserted
    within-file replace semantics: `parses_fragment_lines` already asserted `STAND` -> **2** clips, i.e.
    alternatives, and `Fragment::parse` already appended. The refuted rule lived **only in prose** - the
    module header and the `resolve` doc - and in the fact that there was **no way at all to express the
    modulo**, so nothing could be written against it. The one test whose *name* carried the wrong idea,
    `later_fragment_overrides_and_cancels`, is about the *table-level* composition (a different rule,
    still PROVISIONAL) and its assertions were all correct; it is renamed
    `table_files_compose_by_last_file_and_cancel_drops_a_slot` with a note saying so. The module header had
    already contradicted itself in the same breath (`(repeat = alternative)` in the example above the
    INFERRED paragraph), which is a sign the refuted reading was inherited rather than tested.
  - **Still UNKNOWN after round 11:** the 73-state enumeration behind `0x00817BB0`'s `from*0x49 + to`; who
    builds the `+0x2C` runtime array (its writer is in `0x00DFxxxx`/`0x00E5xxxx`, not isolated); the names
    of the `+0x08` category codes and the meaning of `+0x0C`/`+0x10` (CONFIRMED unread, so probably
    unanswerable); and **the bank query, now answered negatively rather than left open** - if someone
    resumes it, the only unexplored angle is a vcall-site census from the bank *container* object rather
    than from the vtable, and this round's evidence says do not bother.
- **Round 12 (2026-10-04, sandbox worker copy): the fragment resolver is now WRITTEN and TESTED, the 73
  states are half-named from static data, the `+0x2C` writer chain is closed, and the queue entry is
  corrected.** Evidence `ghidra_evidence/0c/ghidra_out_r12{a,b,c,e,f,g}.txt`, targets `targets_r12*.txt`.
  Three negatives were re-confirmed rather than re-hunted (cue dispatch, the bank query, `+0x08`/`+0x0C`/
  `+0x10`), and **no engine code was written to play anything.**
  - **The resolver, as data and as a function (task 1, done).** `fragment_index` is now 32-bit faithful:
    `fragment_index_u32(index, count) = index % count` is the raw rule, and the `usize` wrapper truncates
    to `u32` first. That truncation is a **correction**, not a convenience: the 53 bytes of `0x00E5F760`
    are `MOV EAX,[ESP+0x8]` (a 32-bit load, zeroing the upper half) + `DIV dword ptr`, so the 64-bit
    answer for `usize::MAX % 5` (3) is wrong and the exe's is `u32::MAX % 5` (0). Round 11's one
    `usize::MAX` assertion asserted the 64-bit answer and has been corrected.
    New: **`SlotGroup`** = the `0x18`-byte runtime element (five 32-bit clip handles + the u32 count) and
    **`RuntimeClipTable`** = the `**(this+0x218) + 0x2C` array, one group per slot, with `group`, `set_group`,
    `has_clip`, and `resolve(slot, selector) = 0x00E5F760(slot, entity+0x1E0)`. `Fragment::runtime_group` /
    `runtime_table` / `handle_clip` build one straight from a parsed fragment file, so the whole rule is now
    exercisable end to end: *file -> group -> `selector % count` -> clip*. Handles are numbered from 1
    because 0 is the exe's "empty" marker. `RuntimeClipTable::has_clip` is the **per-entity** predicate,
    which is what rounds 10/11 wanted and could not supply; `has_clip()` stays the shipped 708/156 default.
  - **The runtime array's element type is CONFIRMED: the handles are 32-bit clip *pointers*.** Two
    independent per-frame callers dereference the resolver's result: `0x0065FB00` reads
    `*(float *)(*(handle + 0x44) + 0x60)` (a clip duration, compared against the entity's `+0x34`) and
    `0x0067A050` reads `*(short *)(handle + 0x50)` (a heading short, summed with two other headings). So
    `0x00E5F760` returns a pointer, and the runtime group is `{5 clip pointers, count}`.
  - **`0x00E5F760`'s two arguments, re-confirmed at a THIRD call site.** `0x0067A050` calls
    `FUN_00e5f760(local_8[0x6f], local_8[0x78])` = `0x00E5F760(this+0x1BC, this+0x1E0)`. With
    `[ESP+0x4]`/`[ESP+0x8]` as the two stack arguments that is arg1 = **the slot** (`+0x1BC`) and arg2 =
    **the dividend** (`+0x1E0`) - exactly round 11's conclusion, now on three call sites, not two. The
    same function shows `+0x1B8` is the *state* (a separate field from the current slot at `+0x1BC`).
  - **The clip queue's entry shape is CORRECTED. `0x007EE760` queues `{action_slot, time}`, not
    `{handle, time}`.** Its 240 bytes, read in full: `time = *(this + 0x14) + param_1[1]` (a running clock
    plus the caller's blend), then an 8-byte write `{param_1[0], time}` into the array at **`this + 0x10`**
    (capacity `+0x08`, count `+0x0C`, doubling via `0x00449E10`, old array freed `0x00FF7EF0`). The caller
    `0x006631A0` passes the *slot* as `param_1[0]`. Round 10's "{handle, time}" was wrong.
    **`0x007F1730` (24 bytes, CONFIRMED verbatim)** is `data[param_1]`, falling back to `data[0]` when that
    is 0 - i.e. "the queue is empty here, use the first entry" - which is why 0 is the "no clip" marker.
    Its 9 callers include `0x006666E0`, `0x0067A050` and `0x0066B7E0`.
  - **The transition table, `0x00817BB0`, in exact bytes - and it returns an 8-byte entry, not a clip.**
    `IMUL EDX,[ESP+0x4],0x49` / `ADD EDX,[ESP+0xc]` / `LEA EAX,[EDX*0x8]` / `SUB EAX,EDX` (i.e.
    `EAX = idx * 0x1C`) / `MOV EDX,[ECX+EAX*0x4+0x18]` / `LEA EDI,[ECX+EAX*0x4]` / ... /
    `LEA EAX,[EDI+ESI*0x8]` / `MOV EAX,0x1454398`. So: **block = `this + 0x18 + (from*73 + to) * 0x1C`,
    block `+0x00` is the u32 count, the entries are 8 bytes at `block + 4 + pick*8`, and the return value is
    a pointer into that array.** The block is `0x1C` = count + **three** entries, so a transition has at
    most **3** weighted clips. The pick is `seed = seed*0x343FD + 0x269EC3` (MSVC `rand`), then
    `((seed>>16) * count) / 0xFFFF` with a `CMOVNC` clamp to `count-1` (`SHR ECX,0x10; IMUL ECX,EDX;
    MUL ECX; SHR EDX,0xf; CMP ESI,EDX; CMOVNC ESI,EDX`). `seed` is `*(this + 0x28) + 8 + 0x50`.
    **The count-0 answer is the static 8-byte pair at `0x01454398` = `{0x360, 0}`** (read out of the exe), and
    since the return points at `block+4`, `0x006631A0`'s `*piVar2 != 0x360` is testing the **action slot**
    and its `(char)piVar2[1] == 0` is testing the **byte above it** - the weight. So the play gate is
    *"a transition slot exists, and its weight is non-zero or the slot has a clip"*; `0x360` here is
    `EMPTY_FRAGMENT_TAG` / `ACTION_SLOT_COUNT`, the same sentinel as in a fragment tag. Round 10's "the
    `0x360` test is the play guard" is CONFIRMED and now explained.
  - **The 73 states: the *cycle families* are named from static data; the state *ids* are not (task 3,
    partly answered).** `0x006631A0` does not pick a slot by name; it picks a **state**, and each state owns
    a contiguous range of action slots whose clips it cycles through. The ranges are **static data read out
    of the exe**: **46 eight-byte `{first_slot, count}` pairs at `0x014520B0`**, and every one of them names
    a coherent animation family. The stance driver's switch reads only **10** of them, and those ten are now
    named (this is the round's main data-side result):
    | state | first_slot | count | slots |
    |---|---|---|---|
    | 0 | 139 | 11 | `STAND_IDLE_1` .. `STAND_IDLE_11_TELESCOPE` |
    | 1 | 210 | 5 | `STAND_ALT_1_IDLE_1` .. `_5` |
    | 2 | 217 | 5 | `STAND_ALT_2_IDLE_1` .. `_5` |
    | 3 | 224 | 5 | `STAND_ALT_3_IDLE_1` .. `_5` |
    | 4 | 150 | 6 | `STAND_TRAINED_IDLE_1` .. `_6` |
    | 5 | 626 | 5 | `CROUCH_IDLE_1` .. `_5` |
    | 6 | 176 | 5 | `STAND_FOR_STOKE_IDLE_1` .. `_5` |
    | 7 | 203 | 4 | `STAND_NO_WEAPON_IDLE_1` .. `_4` |
    | 60 | 817 | 5 | `WOUNDED_FRONT_IDLE` .. `WOUNDED_FRONT_CRAWL` |
    | 61 | 822 | 4 | `WOUNDED_BACK_IDLE` .. `WOUNDED_BACK_CRAWL` |
    The other 36 pairs are `ATTACK_1..10`, `DEFEND_1..10`, `MOUNT_ATTACK_1..5`, `RIDER_ATTACK_1..10`,
    `RIDER_ATTACK_BLOCKED_1..5`, `RIDER_CHARGE_ATTACK_1..5`, sixteen `DEATH_*` families
    (`DEATH_STAND_1..13` .. `DEATH_MOVING_1..12`), `RIDER_DEATH_STAND_1..5`, the combat cycles
    (`COMBAT_READY`.. and `COMBAT_1..COMBAT_IDLE_1`, `COMBAT_IDLE_1..10`), `KNOCKBACK_1..5`,
    `KNOCKDOWN_1..5`, the three `PIKEMEN_CHARGED_*_THRUST_*` pairs and the four `REFUSE*` families. The array
    ends at 46 entries: dword 92 onward is floats (the per-slot blend tables `0x01332050` / `0x01332064`,
    nine floats each, read for states 60 and 61).
    **So the 46 entries are the cycle families, and the state ids are a different 73-wide enum: only ten of
    the 73 have a family, and the mapping is not the identity** (state 60 -> entry 36 = `WOUNDED_FRONT_*`,
    while entry 60 = `DEATH_COMBAT_READY_1..10`; state 0..7 -> entries 0..7 *is* the identity by luck of
    ordering). **The other 63 state ids are UNKNOWN and are not nameable from the exe** - they are plain
    integers passed by the 13 callers of `0x006631A0`, and there is no string or enum table for them
    (`str:` on the surrounding data finds nothing; the whole `0x014520B0` block is `u32`s). What *would*
    settle it, precisely: **read the `param_1` constant at each of the 13 call sites of `0x006631A0`** and
    collect the distinct values - that set is the enum, and matching each against one of the 46 families is
    the whole mapping. Two of the 13 are already known to pass 0 and 4 (`0x006631A0` itself rewrites 10 -> 8
    and 0 -> 4 under two conditions).
    A caution on method: **`xref:` is not a complete census here.** `xref:0x014520B0` reports exactly one
    referrer (`0x00663245`), yet the very next entry, `0x014520F0` (= `0x014520B0 + 0x40` = entry 8), is
    referenced by name from `0x0064D8C0`, which Ghidra's reference table does not record. Two `insn:` scans
    over the whole exe for the table found nothing, and the reason is now known and worth recording: Ghidra
    renders a labelled data address as `[0x014520b0]`, and the needle `0x14520b0` is **not** a substring of
    `0x014520b0` (leading zero). Use `014520b` or `dat_014520`. **Do not trust a null `insn:` result for an
    absolute address.**
  - **Who builds the `+0x2C` runtime array: the chain is closed, the single store is not yet read (task 4,
    substantially answered).** The array is not on the `0x1E610` fragment-block object at all. The owner is a
    **per-animation-table record reached through the `battle_entity_man_animation_tables` database**, and the
    whole chain is now known by census:
    `0x006631A0` / `0x0065FB00` / `0x0064D6F0` read `*(int *)(*(int *)(table) + 0x2C)` where `table` comes
    from `0x00E10410()` -> **`battle_entity_man_animation_tables`** (166 bytes, logs
    `"Loading database: %s\n"`, caches at owner `+0x6E4`, **8 callers**), which builds a **100-byte** object
    by `0x00E52C30(horse, elephant, camel, man)` (613 bytes, **sole caller `0x00E10410`**). `0x00E52C30`'s
    own record array is at **`+0x08` capacity / `+0x0C` count / `+0x10` data** (the triple `0x0064D6F0`
    walks), and for each entry of the horse table's `+0xC8`/`+0xCC` array with `*(rec + 0x10) == 0` it
    allocates **`0xA8` bytes** and calls **`0x00E52480`** (1954 bytes, **sole caller `0x00E52C30`**) - the
    four-way animal-table join. So: **the writer of the `+0x2C` array is inside `0x00E52480` or the three
    clip helpers it calls (`0x00E60870`, `0x00E61020`, `0x00E60260`); `0x00E52480` is the only allocator of
    the 0xA8 records and is 0x00E52C30's only callee that can be it.** What is still missing is the actual
    store into `+0x2C` and the loop that walks the 864 slots - **`0x00E52480` in full is the next step, and
    it is one decompile away.** Its 13 x `0x48`-byte allocations match the parent's `in_ECX[6] = 0xD`, and
    `0x00E52C30` also loads a 13-element set from the path string at `0x0131005F` - suggestive but not yet
    read. INFERRED (not yet checked): the runtime array is 864 groups built once per animation table, i.e.
    the static 0x90-byte fragment blocks are realised into 0x18-byte handle groups, which is the same shape
    (`5 x 0x1C + u32` vs `5 x 4 + u32`) with the filename replaced by a loaded clip.
  - **`+0x08` / `+0x0C` / `+0x10` of the name-table descriptor (task 5): answered once, closing.** Still
    CONFIRMED unread by any exe code in 1.3 (round 10's census: the two `xref:` refs are both inside
    `0x00E5FA30`, which reads only `+0x00` and `+0x04`). This round did **not** spend a search on it, and
    should not: an inert field cannot be recovered from the exe, only from a data file that names it, and no
    shipped file does. Leave as documentation.
  - **Also re-confirmed, cheaply, as the two closest things to the per-frame path:** `0x0065FB00` (187 bytes,
    **0 direct callers**, so vtable-dispatched) cycles a *global* `DAT_014DEF98` over slots **406..409** and
    plays the one whose clip duration matches the entity's `+0x34` within 20%; and `0x006616C0` is the
    bearing -> action-slot resolver paired with the play path (it returns `0x360` for "none", the same
    sentinel).
  - **Code landed this round (behaviour of the reader unchanged; nothing was wired into the engine).**
    `battle_animation.rs` gains `fragment_index_u32`, `SlotGroup` (+ `EMPTY`, `new`, `has_clip`, `resolve`),
    `RuntimeClipTable` (+ `new`, `with_slots`, `len`, `group`, `set_group`, `has_clip`, `resolve`),
    `Fragment::{runtime_group, runtime_table, handle_clip}`, `STATE_COUNT` (73), `TRANSITION_BLOCK_BYTES`,
    `TRANSITION_ENTRIES_PER_BLOCK` (3), `NO_TRANSITION_SLOT`, `transition_index`, `pick_weighted`,
    `STANCE_TABLE_ENTRIES` (46), `STANCE_CLIP_RANGES` (the ten named ranges) and `stance_clip_range`. The
    32-bit truncation of `fragment_index` is the only change to existing behaviour, and it makes the function
    match the exe's `DIV dword`.
    **+6 test functions** (`the_runtime_resolver_is_the_modulo_over_the_groups_count`,
    `the_runtime_resolver_falls_back_to_group_zero`,
    `the_runtime_resolver_refuses_what_the_exe_would_only_fault_or_overrun_on`,
    `a_fragment_file_becomes_a_runtime_clip_table`,
    `the_transition_table_is_seventy_three_states_wide_and_picks_weighted`,
    `the_ten_named_states_are_idle_and_wounded_ranges`) and the corrected 32-bit assertion inside round 11's
    `fragments_of_one_slot_are_alternatives_chosen_by_modulo`. `cargo test -p ntw_formats --lib`:
    **163 passed, 0 failed** (was 157). Clippy: **1 warning in this file, unchanged** - round 10's
    `assertions_on_constants` at the `fragment_field` geometry test; no new ones.
  - **Still UNKNOWN after round 12:** the identity of the 63 unnamed states (needs the `param_1` constants at
    the 13 call sites of `0x006631A0`); the actual store into `+0x2C` and the 864-slot loop (one decompile of
    `0x00E52480` away); what a clip *handle* points at (we know `+0x44` is a pointer to something with a
    duration at `+0x60`, and that the clip has a heading short at `+0x50`); and the meanings of the
    descriptor's `+0x08`/`+0x0C`/`+0x10` (CONFIRMED inert, probably unanswerable). **Nothing else about
    battle animation sound or cue dispatch is open** - those two threads are closed negatively and were not
    re-run.
- **Round 13 (2026-10-05, sandbox worker copy): the `+0x2C` store is FOUND and is a plain append; the 63
  unnamed states are not the `param_1` constants but a 73-entry pose table; the resolver is now tested
  against the real 864-slot table.** Evidence `ghidra_evidence/0c/ghidra_out_r13{a,b,c,d}.txt`, targets
  `targets_r13*.txt`. The two closed threads (cue dispatch, bank query) were not re-opened, and no engine
  code was written to play anything.
  - **TASK 1 ANSWERED: the writer is `0x00E60260`, and the store is an unconditional append.** Round 12's
    inference that the writer sat "inside `0x00E52480` or the three clip helpers it calls" was right about
    the callee and wrong about the function: `0x00E52480` only makes the 0xA8 record and its 17 bone
    descriptors and then calls `0x00E60260` last; `0x00E60260` (1019 bytes, **5 callers** - `0x00E510D0`,
    `0x00E515E0`, `0x00E51FD0`, `0x00E50C20`, `0x00E52480`, i.e. the man/rider/camel/elephant/engine record
    builders all of them) is the one population loop. Its store, exact bytes:
    ```
    00e605c4  MOV ESI,dword ptr [EBX + 0x2c]      ; the 864-group array
    00e605c7  ADD ESI,EDI                         ; + slot * 0x18
    00e605c9  MOV EDX,dword ptr [ESI + 0x14]      ; the group's count
    00e605cc  LEA ECX,[EDX + 0x1]
    00e605cf  MOV dword ptr [ESI + 0x14],ECX      ; count += 1
    00e605d6  MOV dword ptr [ESI + EDX*0x4],EAX   ; handles[old count] = EAX
    ```
    `EAX` is the return of `0x00E60660`, the clip loader. **The inner loop is the fragment stride
    `ADD ESI,0x1c` over a count read from `aiStack + slot*0x90`, and the outer loop is `ADD EAX,0x90` /
    `ADD EDI,0x18` bounded by `CMP EAX,0x1e600`** - so **864 slots, `0x18` per runtime group, `0x1C` per
    fragment, and every slot walked whether or not it has fragments**. There is no gap, no dedup, no sort,
    no per-slot branch: the rule is *append in file order*. The count field it increments was already zero,
    because the record's constructor `0x00E506D0` allocates the array and zeroes it:
    `in_ECX[0xb] = FUN_00453500(0x360,0)` followed by `*(dword *)(base + 0x14) = 0; base += 0x18` **864
    times**. **`+0x2C` is that array, and its allocation is now read too** (round 12 could only say "the
    owner is a per-animation-table record"; the field is `in_ECX[0xb]`).
    Two independent confirmations of the 864 geometry: `0x00E61160` walks the same array with
    `ADD EBX,0x18` / `CMP EBX,0x5100` (its exact bytes, `0x5100 / 0x18 = 864`), and `0x00E50930` runs
    `0x00E50960` (`*(in_ECX + 0x8c) = 0`) exactly `0x360` times over the load-time image, whose per-slot
    fragment count therefore lives at **`+0x8C` of the `0x90` block** - CONFIRMED, and it is the count the
    population loop iterates, not `handles.len()`.
    **The per-frame consumer is `0x006631A0` itself**, which reads the very field the loop writes:
    `if (*(int *)(*(int *)(*(int *)in_ECX[0x86] + 0x2c) + 0x14 + iStack_14) != 0)` with
    `iStack_14 = first_slot * 0x18`, i.e. it walks a cycle family's slots and queues each one whose group
    count is non-zero. So load-time image -> `+0x2C` array -> stance driver's cycle -> `0x007EE760` is one
    closed chain.
    **Code:** `SlotBlock` (the load-time block: five handles + the `+0x8C` count), `populate_slot_group`,
    `populate_runtime_table`, `SLOT_BLOCK_FRAGMENT_COUNT`, `LOAD_IMAGE_BYTES`, and `Fragment::slot_image`;
    `Fragment::runtime_table` is now *literally* `populate_runtime_table(&self.slot_image())` rather than a
    parallel implementation of it.
  - **The handle is a loaded clip object, interned per path (CONFIRMED).** `0x00E60660` is a small
    path-keyed cache: bucket count at `+0x7C`, buckets at `+0x78` in 8-byte `{key, clip}` entries,
    `hash = 0x00E5F2A0(path) % buckets` with linear probing (`0x00E57900` compares) and a wrap-around second
    probe from bucket 0; on a miss it calls **`0x00E4F4D0`** (4467 bytes, the clip-object maker) and appends
    the result to a growing vector at `+0x74` (`+0x70` count / `+0x6C` capacity), then inserts. So two
    fragments naming the same clip share one handle, and the count in a group can exceed the number of
    distinct clips. **Partial `Clip` layout, each field tagged** (this is task 4; it is a partial
    characterisation and I stopped rather than spend the round):
    | offset | what | tag |
    |---|---|---|
    | `+0x44` | a pointer; `0x0065FB00` reads `*(float *)(*(handle + 0x44) + 0x60)` - a clip *duration* one deref further on. Not typed beyond that | CONFIRMED (pointer + the read), UNKNOWN (what it points at) |
    | `+0x48` | u32 clip id: `0` unassigned, assigned sequentially by `0x00E61020`, reset to 0 for the slots linked through descriptor `+0x0C`, and tested against `-1` by `0x00E61160` | CONFIRMED |
    | `+0x4C` | u32: the clip's index within its record's clip vector, written by `0x00E61160` | CONFIRMED |
    | `+0x50` | short, a heading, read by `0x0067A050` and summed with two others. **Note this does not collide with round 12's `+0x50`**: the `+0x50` that `0x00E61160` writes is on the *record*, not the clip | CONFIRMED (read) |
    Anything below `+0x50`, and the whole of the record around it, is UNKNOWN. Not worth another round on
    its own: our handles are already stand-ins and the resolver never dereferences one.
  - **TASK 2 ANSWERED, AND ROUND 12'S PREMISE IS REFUTED: the 13 `param_1` constants are NOT the enum.**
    All 13 call sites of `0x006631A0` were read (`callers:` census + all 13 decompiles). Only **six** pass a
    literal: `0x0065D940` -> **10**, `0x005BE5D0` -> **0**, `0x005F0170` -> **23**, `0x005EFFE0` -> **29**,
    `0x00679300` -> **`(bVar10 ^ 1) + 60`** (so 60 or 61), and `0x0061CB90` / `0x00660AB0` / `0x005BE710` ->
    **0**. The other four pass a computed value: `0x00649480` -> `*(this + 0x4FC)` (a field a frame earlier
    copied from `+0x1C4`), `0x00652470` -> its caller's `param_1`, and `0x006543D0` / `0x0062A1F0` -> the
    return of **`0x00643900`**, a 28-byte vcall that answers `0x49` = 73 = "no state" (both callers then
    skip the call). `0x00662E70` is an 8-byte thunk, `MOV ECX,[ECX+4]` / `JMP 0x006631A0`, so it forwards a
    field and has **33 callers**. Six constants and four computed sites cannot be "the enum"; that thread is
    closed.
    **What is in the exe instead is the state *writer*, and it is a pure function of one 73-wide field.**
    `0x00663730` (715 bytes, **8 callers, no callees at all**) is exactly
    `switch (entity + 0x1D8) { case k: entity + 0x1B8 = <immediate>; }` over `0x00`..`0x48` - 73 cases, each
    one store, transcribed in full into `POSE_STATE`. So **`+0x1D8` is the per-frame pose / activity code and
    `+0x1B8` is the animation state**, and the map **reaches 63 distinct states** - round 12's "63 unnamed
    states" counted the other way, which is a satisfying convergence. Six pose groups share a state with
    their three neighbours (`0x28..0x2A` -> 63, `0x2B..0x2D` -> 64, `0x2E..0x30` -> 65, `0x31..0x33` -> 66,
    `0x3C..0x3D` -> 75, `0x44..0x45` -> 44), which is where 73 poses give 63 states.
    **The states remain UNNAMED and I am not going to name them.** The map says which pose picks which state,
    nothing more; the exe has no string or enum table for them. What it *does* give is the pose behind each
    of the ten states §3 can already name: the eight standing idles come from poses `0x00..0x07` **in a
    permuted order** (states `0,1,2,3,4,5,6,7` from poses `0,4,5,6,7,1,2,3`), and wounded 60/61 from poses 37
    and 38. **The remaining 53 of the 63 states match none of the 46 named cycle families and stay
    UNKNOWN** - explicitly, not silently.
    **A measured inconsistency worth recording:** the map's states run to **82**, while the transition table
    is 73 wide, and `0x006631A0` passes both states to `0x00817BB0` with **no range check at all**. So 11 of
    the 73 poses select a state the transition table cannot index. Either those states are never reached in
    play, or the table is larger than round 12 measured - UNKNOWN, and the test asserts the split (62 inside,
    11 outside) rather than hiding it.
    Code: `POSE_COUNT`, `POSE_STATE`, `POSE_LEAVES_STATE`, `pose_state`, `STANCE_STATE_POSES`,
    `WOUNDED_STATE_POSES`.
  - **TASK 3 DONE: the resolver is now tested against the real 864-slot table, not synthetic input.**
    `every_shipped_slot_resolves_through_the_populated_array` walks **all 864** `SLOT_NAMES` entries, builds
    the load-time image a real record would hold (one alternative for each of the 708 slots the shipped
    descriptor marks `HAS_CLIP`, handle = slot + 1), populates it with `populate_runtime_table`, and then for
    every slot asserts the runtime predicate equals the shipped `has_clip` (the two are now tied to each
    other), that the 708 resolve to their own handle for selectors `0, 1, 2, 7, 0xFFFF_FFFF`, and that the
    156 take the documented group-0 fallback and land on group 0's handle for the same selectors. It also
    asserts the fallback's *precondition* rather than assuming it: with slot 0 forced empty, the 156 read
    `None` instead, which is the divide the exe does not survive. `the_population_loop_is_what_a_fragment_file_becomes`
    checks `slot_image` -> `populate_runtime_table` == `runtime_table` and == `runtime_group` per slot, and
    `the_runtime_array_is_populated_by_appending_in_file_order` locks the geometry (`0x1E600`, `0x5100`,
    `0x8C`, five fragments then the count) and the append order.
  - **`+0x08` / `+0x0C` / `+0x10` (task 5): not spent, as instructed - but the field census needs one
    correction.** Round 10 read the name-table base `0x013AEBE0` as having two code references, both inside
    the resolver. That is an artefact of *which address* was census'd: the loader reads the descriptors
    through the labelled bases `DAT_013AEBE8` (`0x013AEBE0 + 8`) and `DAT_013AEBF0` (`+ 0x10`), and
    `FUN_00E506D0` copies all 864 `+0x0C` values out of `DAT_013AEBEC` (`+ 0x0C`) in a `0x360`-iteration
    loop. So **`+0x0C` is read wholesale by every record's constructor** and `+0x08`/`+0x10` are read by the
    population loop (`+0x08 == 0x0F` takes a special path; `+0x10 != 0x360` records which slot actually
    supplied the clip). They are *not* inert; they are simply not decoded, and this round did not decode them.
    The correct statement is "read by the loader, meaning UNKNOWN", not "read by nobody".
  - **Also recorded, no code:** `0x00E61020` (314 bytes, the pass after the population loop) walks all 864
    groups twice: the first loop resets `handle[0]->+0x48` to 0 for every slot whose descriptor `+0x0C` link
    index is in range, the second assigns each unassigned clip the next id and appends it to the record's
    clip vector; then it makes one indirect call. `0x00E61160` (125 bytes, exact bytes read) then numbers the
    clips: for every slot whose count is non-zero *and* whose first handle has `+0x48 != -1`, it stores the
    running counter in `handle->+0x4C`, and finally writes the total into every record's `+0x50`.
    **`0x00E50C20` / `0x00E515E0` / `0x00E51FD0` / `0x00E51580`** are the camel / naval-engine / elephant
    siblings of `0x00E52480`, all with the same shape (`FUN_00E506D0` then bones then `FUN_00E60260` then
    `FUN_00E61020`) - the population loop is shared by every record type, which is why its shape can be
    trusted as *the* rule.
  - **Code landed this round** (no behaviour change; nothing wired into the engine):
    `SlotBlock` (+ `EMPTY`, `new`, `Default`), `populate_slot_group`, `populate_runtime_table`,
    `Fragment::slot_image`, `SLOT_BLOCK_FRAGMENT_COUNT`, `LOAD_IMAGE_BYTES`, `POSE_COUNT`, `POSE_STATE`,
    `POSE_LEAVES_STATE`, `pose_state`, `STANCE_STATE_POSES`, `WOUNDED_STATE_POSES`; `Fragment::runtime_table`
    is now defined as the population loop over `slot_image`.
    **+4 test functions** (`the_runtime_array_is_populated_by_appending_in_file_order`,
    `the_population_loop_is_what_a_fragment_file_becomes`,
    `every_shipped_slot_resolves_through_the_populated_array`,
    `the_pose_code_reaches_sixty_three_states_and_names_only_ten`).
    `cargo test -p ntw_formats --lib`: **167 passed, 0 failed** (was 163).
    `cargo test --workspace --lib`: 39 + 31 + 18 + **167** + 20 + 305, all ok.
    `cargo build --workspace`: clean. `cargo clippy -p ntw_formats --all-targets`: **1 warning in this file,
    the pre-existing round-10 `assertions_on_constants`** - none new. `rustfmt --edition 2024 --check` on the
    file: clean (the diff in this file is +66/-26 net, and the 26 are rustfmt re-wrapping lines this round
    touched).
  - **Still UNKNOWN after round 13:** the names of the 53 states that match no cycle family (and the 63
    minus the ten named, as §3 has always said - the pose table locates them, it does not name them); what a
    clip handle points at beyond `+0x44`/`+0x48`/`+0x4C`/`+0x50`; the meanings of the descriptor's `+0x08`,
    `+0x0C` and `+0x10`, now known to be read but not decoded; and whether the states above 72 are ever
    reached. **Closed this round:** the `+0x2C` store and the 864-slot loop (both), the `param_1`-is-the-enum
    premise (refuted, replaced by the pose table). Nothing else about battle animation sound or cue dispatch
    is open - those two threads remain closed negatively and were not re-run.


## 4. Determinism audit (2026-10-04)
Scope: the model crates that must replay exactly (`ntw_sim` battle + campaign, `ntw_ai`, `ntw_campaign` turn loop) and how
the app drives them.
- **Hash-map order:** no `HashMap`/`HashSet` is iterated in those crates. The only one (`ntw_sim::campaign::polysmooth`,
  `by_edge`) is filled and then only looked up by key, so its random order never reaches the result. Every model
  collection that is iterated is a `BTreeMap` or a `Vec` (CONFIRMED by a source scan).
- **Time, threads, OS randomness:** none in the model crates (no `SystemTime`, `Instant`, `thread`, `rayon`,
  `RandomState`, `process::id`, `rand`). The model RNGs are seeded explicitly (battle `seed`, campaign `rng.state`, both
  in the state hash). The display-only `AudioRng` uses the process id: sound variation only, never read by the model.
- **Tick scheduling:** the battle model ticks in Bevy's `FixedUpdate` at 10 Hz, the battle AI in `FixedPreUpdate` right
  before each tick; speed changes scale the virtual clock, so a tick's result does not depend on speed or frame rate
  (`battle/mod.rs` header). Campaign turns run whole in `driver::end_turn`.
- **Floating point:** the model uses f32 with plain `+ − × ÷ sqrt` (IEEE exact) and 16 calls to `sin`/`cos`/`atan2`/`powf`/
  `exp`/`ln` (`ntw_sim` battle model and campaign `polypath`; `ntw_ai` battle classes, outflank, rating, campaign
  region value). Those come from the platform libm: identical for the same build on Windows x64 (the only target), but
  **not guaranteed across platforms or toolchains** (RISK for cross-platform multiplayer: replace with our own
  deterministic versions if a non-Windows build is ever needed). Rust never fuses `a × b + c` into FMA by itself.
- **Harness:** `cargo run -p ntw_ai --release --example determinism -- twice battle [ticks] [seed]` / `twice campaign
  [turns]` runs the trace in two separate processes (so per-process differences such as hash seeds and addresses would
  show) and reports the first diverging line. Results 2026-10-04: battle France v Austria AI v AI, seed 7, 3000 ticks:
  IDENTICAL (61 hashes); seed 3 to the end (tick 3622, Won side 0): IDENTICAL; campaign eur_napoleon 3 End Turns with the
  AI (38 orders on turn 3): IDENTICAL. In-process checks already exist (`ntw_sim` `determinism_1000_ticks`,
  `ntw_ai` `campaign_ai_is_deterministic`, `real_ai_v_ai_same_seed_same_result`).
- Open: the app-level replay (record player orders with tick numbers and re-run) belongs to the multiplayer work;
  `state_hash` covers the model, not the AI's internal state (its effect shows through the orders it gives).

## 5. Comparison harness (2026-10-04)
For the user: `docs/COMPARE_WITH_ORIGINAL.md` (checklist: main menu picture, a historical battle's opening view, a
battle trace, listening). Our side:
- pictures: `--screenshot <png>` (front end), `NAPOLEON_BATTLE_SCREENSHOT=<png>` with `--battle-key <KEY>` (the battle
  file's start camera, the same the original uses; INFERRED for the camera height);
- `cargo run -p napoleon --release --example image_diff -- <ours> <original> [heat.png]`: resamples ours to the
  original's size; prints mean abs difference per channel, PSNR, share of pixels with a brightness difference > 32, and
  the correlation of 16 × 12 block brightness grids (layout); writes a heat map;
- `NAPOLEON_BATTLE_TRACE=<csv>`: one row per unit every 10 s of battle time (men, morale value and behaviour, fatigue,
  position), to compare in shape with numbers read off the original's unit cards;
- determinism: the twice-run harness of §4.
Checked: `--battle-key NHB_Austerlitz --skip-deployment` writes the picture and a 40-row trace; image_diff of a picture
with itself gives 0 difference and correlation 1.0.

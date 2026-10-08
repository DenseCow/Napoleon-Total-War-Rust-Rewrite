# Bink video (`.bik`) — format notes and our decoder

Worker: bink (branch `work/bink`). BACKLOG §1 and §8. The original plays movies through RAD's `binkw32.dll`
(shipped in the install, 173,056 bytes, Bink 1.9-era exports such as `_BinkOpen@8`, `_BinkDoFrame@4`,
`_BinkOpenTrack@8`). **NapoleonRust never loads or calls it.** Our decoder is `ntw_formats::bink`, our own Rust,
written from public format descriptions of Bink 1 (used as hints) and from Ghidra analysis of the DLL (project
`%USERPROFILE%\Documents\NR-bink-ghidra\`, not in git). No decompiled code is stored; this file is the spec.

Tags: CONFIRMED (checked on the shipped files and/or read in the DLL), INFERRED, UNKNOWN; stand-ins PLACEHOLDER /
PROVISIONAL.

## 0. Where I am / what's next
(kept current; newest first)
- 2026-10-04, **audio + player (worker bink-audio, branch `work/bink-audio`)**: took over `bink/audio.rs` + `bink/dct.rs`
  (the draft from 93b3394, verified, one additive change: the three tables are `pub` for the DLL check) and wrote the
  player `crates/napoleon/src/video/`. Done: install test `tests/bink_audio_install.rs` (all 229 sound tracks of the 70
  movies with sound decode, every packet used exactly, sample counts exact; tables byte-identical to `binkw32.dll`);
  Bevy player with GPU colour conversion, sound through our audio system with the movie volumes, skip, `--intro`,
  `--frontend-movie`, `PlayMovie` hook; track-per-language, intro list and front-end movie read from Napoleon.exe (§8).
  Open: §7 (skip rule, volume group 4, intro order, sample-exact audio, ±1 colour on the GPU path).
- 2026-10-03 (latest): video side complete. Install test: 73/73 movies, 71,518 frames, every plane boundary exact; golden
  frame hashes; colour conversion now follows the game's own movie shader (§6, `Frame::to_rgba` / `YuvToRgb`; a
  player should do the same maths in a shader from three R8 textures: `Frame::{y,u,v}`, sizes `y_size`/`c_size`,
  stride = width). Decode speed 1.4 ms per 720p frame in a debug build. Open: §7. Unrelated: `ntw_campaign`
  `real_install::loads_user_saves` fails on a `test.save` written today (19:50) into the user's save folder by
  someone else (force without commander); not touched here.
- **2026-10-03, NOTE FOR THE AUDIO HELPER (work/bink-audio):** before the split arrived, commit **93b3394** on `work/bink`
  already added a complete Bink audio decoder: `crates/ntw_formats/src/bink/audio.rs` (`AudioDecoder::new(&AudioTrackInfo)`,
  `decode_packet(packet, &mut Vec<i16>) -> Result<usize, BinkError>`, returns the block bytes used) and
  `bink/dct.rs` (our own FFT-based DCT-III and inverse real DFT). Spec in §5 below. Verified: on `sega_logo_sting_hd`,
  `ntw_intro` (track 0) and `nhb_01_arcole` (tracks 0, 3, 6) every audio packet is used to its last 32-bit word and the
  sample count equals the packets' byte counts and the video duration exactly; peaks look sane (intro 23103, SEGA 32767
  with 3 clipped samples). Not done: sample-exact comparison with the DLL (float rounding, §5.3). From now on I do not
  touch `bink/audio.rs`, `bink/dct.rs` or `napoleon/src/video/`; please take them over from 93b3394 (merge `work/bink`).
  Container API (`BinkHeader`, `split_packet`, `FramePacket`, `AudioTrackInfo`, `Vfs::read_range`) is stable.
  Video API: `VideoDecoder::new(&BinkHeader)`, `decode(video_bytes) -> Result<PlaneEnds, _>`, `frame() -> &Frame`
  (Y/U/V planes padded to 8, `Frame::to_rgba` PROVISIONAL BT.601).
- 2026-10-03: video decoder done; all 3,869 frames of `frontend2.bik`, all of `sega_logo_sting_hd.bik` and
  `nhb_01_arcole.bik` decode with every plane ending exactly at the stream's next-plane offset. Next: install test over
  all 73 movies, golden hashes, colour conversion check, spec write-up.
- 2026-10-03: survey done (§1), container reader done (§2). Next: video tables from the DLL, video decoder.

## 1. Survey of the shipped movies (CONFIRMED, `cargo run -p ntw_formats --example bink_probe -- survey`)
- **73 movies**, all in `media.pack` under `movies\` (one, `ntw_intro_lowres.bik`, under `movies\low_res\`). There is no
  `movies.pack` in the Steam install; `movies\rules.bob` (68 bytes, `[Pack] PackFile = movies.pack, PackType = bink`) is a
  leftover build rule. Subtitles are separate: `movies\<name>.bik_<lang>.csv` in `data.pack` (8 languages: cz de en es fr
  it po ru).
- **Every file is revision `BIKi`**, frame rate 30/1, video flags 0 (no alpha, not grey-scale), **one keyframe** (frame 0
  only; every later frame is a delta frame).
- Sizes: 19 at 1280x720 (intro, front end, campaign cutscenes `ncs_*`, Ottoman victory, SEGA and Core i7 logos), 32 at
  1024x576 (duel/agent clips `sword_*`, `pistol_*`, `social_*`, `surgery_00`, `funeral_00`, `hospital_00`, `eur_*_rev_*`,
  `ottoman_lose_battle`), 21 at 640x360 (`nhb_*` historical-battle intros, `eur_*_vic`, `eur_lose_*`, `wel_waterloo`),
  1 at 800x450 (`low_res\ntw_intro_lowres.bik`).
- Total 71,518 frames, 2,383.9 s (39.7 min). Longest `ncs_09_waterloo.bik` 4,164 frames (138.8 s, 168.7 MB); the front-end
  loops `frontend.bik`/`frontend2.bik` 3,869 frames (129.0 s) each.
- **Audio:** 3 movies have no audio (`frontend.bik`, `frontend2.bik`, `corei7_intro.bik`). Every audio track has flags
  `0x7000` = DCT + stereo + 16-bit. 48,000 Hz in 12 files (intro, SEGA logo, `eur_*_vic`, `eur_lose_*`), 44,100 Hz in the rest.
  - 1 track in most files; **7 tracks** (ids 0..6) in the `ncs_*`, `nhb_*` and `ntw_intro.bik` files, 8 (ids 0..7) in
    `ott_*_vic.bik`. The ids are not in order in some files (`ntw_intro.bik`: 0,1,3,4,5,6,2). INFERRED: one track per voice
    language; which id is which language is UNKNOWN (the game picks it with `BinkSetSoundTrack`; see §6).
  - Header "max decoded size" 145,920 bytes (48 kHz) / 138,240 (44.1 kHz) = 19 / 18 audio blocks of 7,680 bytes; frame 0
    carries that much preroll, every later frame one block (7,680 bytes = 1,920 stereo 16-bit samples).

## 2. Container (CONFIRMED on all 73 files)
```
0   char[3] "BIK", u8 revision ('i')
4   u32 file size - 8
8   u32 frame count
12  u32 largest frame packet
16  u32 frame count (again)
20  u32 width, 24 u32 height
28  u32 fps numerator, 32 u32 fps denominator
36  u32 video flags (0x100000 alpha, 0x20000 grey)
40  u32 audio track count T
44  T x u32 max decoded bytes per packet
    T x { u16 sample rate, u16 flags }   flags 0x1000 DCT, 0x2000 stereo, 0x4000 16-bit
    T x u32 track id
    (frames + 1) x u32 file offset of each frame; bit 0 = keyframe; the last entry is the file size
```
Frame packet = for each track `u32 n; n bytes` (n = 0: no audio this frame; otherwise the first u32 is the decoded byte
count), then the video bitstream to the end of the packet.

Reader: `ntw_formats::bink::{BinkHeader, split_packet}`; streaming reads use `Vfs::read_range` (new) so a 169 MB movie is
never loaded whole.

## 3. Video bitstream, revision `'i'` (CONFIRMED in `binkw32.dll` and on all 73 files)
Addresses are `binkw32.dll` (image base `0x18000000`). Plane driver `0x180182D0`, block loop `0x18015CC0`.

### 3.0 Frame layout
- Bits are read LSB-first from little-endian 32-bit words (`bits.rs`).
- Video packet = `u32 chroma_offset` (bytes from the packet start), the Y plane, then at `chroma_offset` the **Cr**
  plane, then (32-bit aligned) the **Cb** plane. Y ends (aligned) exactly at `chroma_offset` and Cb ends exactly at the
  packet end on every one of the 71,518 shipped frames (install test). Alpha (flag `0x100000`): `u32 size` + a plane
  before Y; grey (flag `0x20000`): no chroma. Revisions before `'h'` have no offset word and another colour coding; not
  supported (no shipped file needs them).
- Plane buffers (`BinkGetFrameBuffersInfo` `0x1800D5B0`): Y `(w+7)&~7` x `(h+7)&~7`, chroma `(((w+1)>>1)+7)&~7` x
  `(((h+1)>>1)+7)&~7`. The plane loops run over these padded sizes (e.g. 640x360 has 184 chroma rows).
- Two frame buffers alternate; each frame decodes into the other buffer and reads the previous picture from the current
  one. INFERRED: both start zero-filled (frame 0 of every shipped movie is a keyframe anyway).
- Napoleon registers its own buffers (`0x012161C0`: pitch 0x500 for Y, 0x280 for chroma, i.e. 1280-wide textures).
  The pitch could only matter for motion vectors pointing outside the picture; **none of the 73 movies has one**
  (checked), so our stride = padded width gives the same pixels.

### 3.1 Bundles
At the start of each plane, in order: Huffman descriptions for block types, sub-block types, the 16 "colour high"
trees and the colour low tree, patterns, x offsets, y offsets, runs (the intra/inter DC bundles have none). At the start
of each 8-pixel block row, every bundle whose decoded values are used up reads `count = read(bits)` (`count == 0` ends
that bundle for the plane) and refills, in order: block types, sub-block types, colours, patterns, x off, y off, intra
DC, inter DC, runs. Count widths: `bits = floor(log2(v + 511)) + 1` with `v` = `w/8` (block types, x/y off, both DCs),
`w/16` (sub-types), `w/8*64` (colours), `w/8*8` (patterns), `w/8*48` (runs), `w` = padded plane width.
- Block types: 1 bit fill -> `read(4)` repeated; else Huffman symbols, `<12` = a type, `12..15` = repeat the last type
  `{4, 8, 12, 32}` times (`last` starts at 0 each refill; a run longer than the space left stops the refill).
- Colours: 1 bit fill -> one value repeated; each value = `high << 4 | low`, `high` decoded with tree
  `colour_high[previous high]` (the previous high persists across refills, reset to 0 per plane), `low` with the colour
  tree. No `^0x80` in revision `'i'`.
- Patterns: two Huffman nibbles per byte, low first.
- x/y offsets: fill -> `read(4)` plus a sign bit if non-zero; else Huffman value plus a sign bit if non-zero.
- DCs: 11 bits (inter: 10 bits + sign if non-zero), then groups of up to 8: `size = read(4)`, deltas of `size` bits
  each with a sign bit if non-zero, accumulated.
- Runs: fill -> `read(4)` repeated; else Huffman symbols.

### 3.2 Huffman trees
`read(4)` = code set (16 fixed prefix codes of up to 7 bits; tables at `0x1802DA84`, `0x1802DAA0`, `0x1802DE80`, in
`tables.rs`); set 0 = plain 4 bits. Then a symbol permutation: bit 1 -> `read(3)+1` explicit 4-bit symbols, the rest
in ascending order; bit 0 -> `depth = read(2)` and `depth+1` bit-driven merge passes over `0..16` (pairs, then 4s, ...;
each merge decision reads one bit: 0 = left list, 1 = right list).

### 3.3 Block types (per 8x8 block, left to right)
| type | name | contents |
|---|---|---|
| 0 | skip | copy from the previous frame, same place |
| 1 | scaled | 16x16 from an 8x8 sub-block. Sub-type from its bundle on even block rows only; odd rows skip 16 px. Needs 16 px left in the row (else an 8 px no-op, sub-type not read) and 16 rows below (else an 8 px no-op, sub-type read). Sub-types 3 run, 5 intra, 6 fill, 8 pattern, 9 raw; others decode nothing |
| 2 | motion | copy from previous + (x, y) offsets |
| 3 | run | `read(4)` = one of 16 pixel orders (`0x18029600`); runs of `run+1` pixels, 1 bit: 1 = one colour, 0 = a colour each; repeated while more than 1 pixel is left; a final single pixel takes one more colour |
| 4 | residue | motion copy + `read(7)` masks of bit-plane residue (§3.5), added with 8-bit wrap |
| 5 | intra | DC from the intra DC bundle, coefficients (§3.4), `read(4)` quantiser, IDCT |
| 6 | fill | one colour |
| 7 | inter | motion + DC from the inter DC bundle, coefficients, `read(4)` quantiser, IDCT added to the motion source with 8-bit wrap |
| 8 | pattern | two colours, 8 pattern bytes (bit set = second colour) |
| 9 | raw | 64 colours |

Motion sources are accepted only if the 8x8 source lies inside the plane by a linear address check; otherwise the
block (and, for types 4 and 7, its extra bits) is skipped. This never happens in the shipped files.

### 3.4 DCT blocks
Coefficients: a work list of `(index, mode)` entries starting `{4:0, 24:0, 44:0, 1:3, 2:3, 3:3}`; `read(4)` passes
with `bits` = n-1 .. 0. Mode 0 -> becomes `(i+4):1`, then reads 4 coefficients; mode 1 -> becomes `i:2` and appends
`(i+4):2, (i+8):2, (i+12):2`; mode 2 -> cleared, reads 4 coefficients; mode 3 -> one coefficient. For each of the 4:
1 bit = push `i:3` at the list front, else a value (`bits` magnitude bits + implied top bit + sign; ±1 when `bits` = 0).
Scan order `SCAN` in `tables.rs` (from the reader's final permutation). Dequantisation happens in the IDCT column pass:
`coef * q[natural index] >> 11` (32-bit wrap), with `q` from 16 intra (`0x18029A00`) or inter (`0x1802AA00`) tables.
Integer IDCT (`0x1801EC40` put, `0x1801EF80` 2x put, `0x1801F320` add): constants 2896, 2217, 3784, -5352 (`>> 11`),
columns first (a column whose AC is zero is flat), rows output `(x + 127) >> 8` truncated to 8 bits (no clamp).

### 3.5 Residue blocks
The list starts `{4:0, 24:0, 44:0, 0:2}`, `mask = 1 << read(3)`, halved after each of the `read(3)+1` passes. Each pass
first refines every coefficient found so far (1 bit: move it `mask` further from zero), then walks the list as in §3.4,
but new coefficients are `±mask` (sign bit). Stops after `masks + 1` applications. Values are 8-bit, added to the
motion source with wrap, in scan order.

## 4. Verification of the video decoder (`tests/bink_install.rs`, `--ignored`)
- `every_movie_decodes_completely`: 73/73 movies, 71,518 frames, no error, every plane boundary exact (27 s on 8
  threads, debug build).
- `golden_hashes`: FNV-64 of the Y/U/V planes at frames of `sega_logo_sting_hd`, `frontend2`, `nhb_01_arcole`. They are
  self-generated: there is no reference output (no ffmpeg on this machine, and we never run the DLL). Pictures checked
  by eye: the SEGA logo (correct blue), the front-end map background, the Arcole river landscape, fire in `ncs_07`.
- Speed (`bink_probe bench`, debug build, opt-level 1): 1.4 ms per 1280x720 frame to decode (budget 33 ms);
  `Frame::to_rgba` on the CPU 8.5 ms (a GPU shader is the better place, §6).

## 5. Audio (CONFIRMED structure; see the note in §0: the audio helper owns this code now)
- Init (`0x18013E10`): frame length 512 (< 22,050 Hz), 1024 (< 44,100 Hz), else 2048. RDFT variant: the rate and frame
  length are multiplied by the channel count and coded as one channel. Bands: critical frequencies `{0, 100, ..., 15500}`
  (25 values, `0x1802E018`) up to half the rate; `band[k] = max(1, f[k] * (len/2) / half_rate)`, last = `len/2`.
  Quantisers `exp(i * 0.1528916)` (96 floats, `0x1802E328`); run lengths `{2, ..., 16, 32, 64}` x 8 (`0x1802E314`).
- Packet = `u32 bytes_out`, then 32-bit-aligned blocks until that many bytes are produced. Block: the DCT variant skips
  2 bits; per channel 2 floats (5-bit power, 23-bit mantissa, sign; `0x1802E4A8`), one 8-bit quantiser index per band
  (clamped to 95), then coefficients from index 2 in runs (`bit ? run_len[read(4)]*8 : 8`) of `read(4)`-bit magnitudes
  + sign; band k's quantiser starts at coefficient `band[k]*2`.
- Transform: the DCT variant is the forward branch of `ddct` in the split-radix float FFT package inside the DLL
  (`0x1801A440`): `C[k] = sum a[j] cos(pi j (k+1/2) / n)`. The RDFT variant is an inverse real DFT (`0x1801A360`).
  Output `round_half_even(C * 2/sqrt(len))`, saturated to 16 bits, channels interleaved.
- Overlap (`0x180134F0`, SSE2 path): the first `len*ch/16` interleaved samples of each block (except the first block)
  become `prev + ((cur - prev) * i >> log2(len*ch/16))`; the last `len*ch/16` samples are kept as `prev`. Each block
  emits `(len - len/16) * ch` samples.


### 5.1 Verification (`tests/bink_audio_install.rs`, `--ignored`, about 5 s)
- `every_movie_audio_track_decodes`: CONFIRMED on the install: 229 sound tracks in 70 movies (9,634 s of sound) decode
  with no error; every packet's blocks end exactly at its last 32-bit word; the decoded sample count equals the packets'
  byte counts and equals **exactly** `frames * sample_rate / fps` sample frames (the sound lasts as long as the video,
  to the sample); no track is silent. All tracks are the DCT variant, stereo; 44,100 or 48,000 Hz (the RDFT path is
  implemented but no shipped file uses it, so it is untested on real data: INFERRED).
- `audio_tables_match_binkw32`: CONFIRMED byte for byte in `binkw32.dll` (read as bytes only): the 96 quantisers (file
  offset `0x22F28`), the 25 band edges as u32 (`0x22C18`) and the 16 run lengths as u8 (`0x22F14`).
- Frame 0 carries 18 or 19 blocks of preroll and later frames one 1,920-sample block or none, so the sound in the file
  runs up to ~0.4 s ahead of the pictures; sample `s` plays at `s / rate` seconds after frame 0 is shown.
- UNKNOWN: sample-exact equality with the DLL (float FFT rounding, §5.3); not measurable without running the DLL.

## 6. Colour conversion (CONFIRMED source: the game's own shader)
Napoleon does not call `BinkCopyToBuffer`. It registers 8-bit plane textures and draws them with `fx\sprite.fx`,
`pixel_yuv` (technique `normal_yuv_t0`; the `normal_yuv_t` string is in the exe). Bilinear samplers on Y, Cr, Cb, then
`rgb = 1.164123535*Y + (1.595794678*Cr, -0.813476563*Cr - 0.391448975*Cb, 2.017822266*Cb)
+ (-0.87065506, 0.529705048, -1.081668854)`, then `pow(abs(rgb), 2/GAMMA_VALUE) * g_brightness * (1/1.2)`.
`GAMMA_VALUE` is the renderer's gamma (`gfx_gamma_setting`, default 2, which makes the step a no-op), passed as a shader
macro at compile time (Napoleon.exe `0x011A7F10`); `g_brightness` is 1.2 (`gfx_brightness_setting` default 1.2).
`Frame::to_rgba` does this on the CPU (`YuvToRgb`); a player should do it in a shader from three R8 textures.
INFERRED: texel-centre alignment of the bilinear chroma upsampling (the D3D9 half-pixel offset of the quad is not
checked).

## 7. Open questions
- INFERRED: the initial frame-buffer contents (zero) and the quad half-pixel alignment (§6).
- Not supported: alpha planes and revisions before `'h'` (none shipped).
- UNKNOWN: the skip rule (which keys/clicks skip which movies). PROVISIONAL in the player: any key or mouse button skips
  a skippable full-screen movie. (`DismissMovieEvent` at `0x00A281C0` is a scripted UI event, not the intro skip.)
  Leads (0-C, 2026-10-04): while a full-screen movie object exists, the front-end UI's three mouse-button handlers (UI
  vtable `0x01312C2C` slots 20..22, `0x00DB20C0` / `0x00DB2100` / `0x00DB2140`) ask `0x00D9FB40` and swallow the click when it
  says no; the movie stop itself is `0x00D9FB70` (the movie's +0x28 "can stop" then +0x14 "stop"). The key path that ends
  an intro movie was not found (no `0x1B` Escape compare in the movie code). Front-end Lua: `root.luac` steals ESCAPE for
  Back, not for movies.
- UNKNOWN: sound group 4. Movie volume in the exe (`0x004831D0`, `0x004832F0`) = `group_volume(4) * 0.01 *
  group_volume(5)` (slot `+0xDC` of the sound manager returns the int at `+0xC140 + 8*group`; group 5 = master per
  AUDIO_FORMAT.md), passed to `BinkSetVolume(bink, track, volume)` (`0x0121609D`). What sets group 4 is not read yet.
- Intro order: CONFIRMED 2026-10-04 (§8.3). Front-end movie looping: INFERRED (the `1` argument of `0x004831D0`).
- INFERRED: ±1 colour differences on the GPU path (§8.2).

## 8. Player (`crates/napoleon/src/video/`, worker bink-audio)
### 8.1 Structure
- `source.rs`: `MovieSource` (next frame as Y/Cb/Cr planes + its sound; `BinkSource` streams through `Vfs::read_range`
  in 4 MB windows; `StubSource` is a generated test movie) and `MovieStream`, a decoder thread that runs up to 3 frames
  ahead and pushes the sound into a `PcmFeed` (`crate::audio::feed`).
- `mod.rs`: `PlayMovie` / `StopMovie` / `MovieFinished` messages, the `Movie` component, the frame clock (frame `n` is
  shown from `n / fps` seconds of real time after frame 0; the sound starts with frame 0), skip, letterboxed full-screen
  display on black (aspect kept: PROVISIONAL), `--intro`, `--frontend-movie`, and the test harnesses `--play-movie
  <name>` and `--movie-hold <frame>` (freeze at a frame, for `--screenshot` comparisons).
- Sound: `crate::audio::PlayMovieAudio` plays the feed as a 2D voice with gain = movie table volume (`sound_events`
  movie list, e.g. SEGA logo 0.5, `ncs_*` 0.9; others 1.0) x `MOVIE_VOLUME` setting (1.0) x master volume
  (`Group::Movie`). PROVISIONAL combination (§7).

### 8.2 Colour on the GPU
Each movie has three R8 plane textures and an RGBA render target; an off-screen camera draws a quad with
`MovieYuvMaterial` (`movie_yuv.wgsl`), the §6 maths with bilinear samplers, chroma sampled at `pixel / 2 / chroma_size`
(the CPU path's taps). The target is sRGB, so the shader returns `srgb_to_linear(rgb)` and the stored bytes are the
original's 8-bit values. Check (`--movie-hold`, 1:1 letterbox at 1280 wide, against `bink_probe decode` PNGs of the CPU
path): `ncs_09_waterloo` frame 90 max difference 6, mean 1.1 per pixel (R and B about +1, G about -0.6);
`sega_logo_sting_hd` frame 60 the same bias, max 31 on a few edge pixels. INFERRED cause: GPU sRGB encode/filter
precision; not yet resolved.

### 8.3 What the exe does (Napoleon.exe 1.3, own Ghidra project, CONFIRMED unless tagged)
- Sound track = player language: `BinkVideoController` reads `language.txt` (`0x011C3520`) and maps it with the table at
  `0x01419A58` (`0x0121AB70`): EN 0, FR 1, DE 2, ES 3, IT 4, RU 5, PO 6, CZ 7, unknown 8. The movie opener
  (`0x01215C10`) turns 7 into 0 and an id past the movie's track count into 0, calls `BinkSetSoundTrack(1, &id)`, then
  `BinkOpen(path, 0x804400)` and `BinkSetVolume(bink, id, volume)`; a movie opened without sound calls
  `BinkSetSoundTrack(0, 0)`. Ours: `video::language_track` (id 0 when the id is missing).
- Bink's sound goes out through its own Miles driver (`BinkOpenMiles`, 44,100 Hz 16-bit stereo) or DirectSound
  (`0x011C3520`), not through the game's sound groups; ours mixes it as one of our voices.
- Intro (`0x00484D30`, front-end UI vtable `0x01312C2C` slot 34, once: the flag at +0x54 is cleared): queues `NTW_Intro.bik`,
  `Corei7_Intro.bik`, `SEGA_logo_sting_HD.bik` unconditionally (a developer flag at +0x55 plays every file under `movies`
  instead). **Order CONFIRMED (0-C, 2026-10-04):** the queue player `0x0048B5B0` takes the last entry (count +0x1BC,
  0x14-byte entries at +0x1C0) and shrinks the count, so the movies play SEGA logo, Intel logo, intro (ours).
  Each entry: name, an int at +0xC (1 for all three: the wide-movie type), bytes at +0x10 (copied into the play parameters)
  and +0x11 (sets a flag on the queue owner when the movie starts); both 0 for the intro movies.
- **Full-screen size (CONFIRMED, `0x0048B5B0`):** a type-1 (wide) movie on a screen narrower than 1.4:1 is shown at
  (width, width × 0.5625), centred; a type-0 movie on a screen wider than 1.4:1 at (height × 4/3, height); otherwise it fills
  the screen (stretched). Ours: `video::fullscreen_size` (the type taken from the movie's own shape: INFERRED).
- **Defaults (2026-10-04):** the intro and the front-end movie are now on by default (`--no-intro`,
  `--no-frontend-movie` turn them off); harness runs (`--screenshot`, `--ui-click`, `--battle*`, `--campaign`, ...) skip
  both unless `--intro` / `--frontend-movie` are given. The skip rule stays PROVISIONAL (§7).
- Front end (`0x004858D0`): finds component `movie_bg` and plays `Frontend2.bik` there (`0x004831D0(name, 0, 1, 0)`).
  `movie_bg` is 1920x960 under the 1920x1200 `background` (`fe_background_2.tga` still); we stretch the 1280x720 movie
  over `movie_bg`'s rectangle (PROVISIONAL scaling).
- `MOVIE_LANGUAGE` is a tweak in `BinkVideoController.cpp` with default `DEFAULT` (`0x00440780`); its effect is UNKNOWN.

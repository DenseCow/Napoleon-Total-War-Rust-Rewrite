# Font formats: `.cuf` bitmap fonts and `ui\fontcategories.fc`

Reader: `crates/ntw_formats/src/font.rs`. Probe: `cargo run -p ntw_formats --release --example cuf_probe -- font/frontend_22.cuf "AVa"`.
Real-install test: `every_cuf_font_and_font_categories_parse` (all 74 `.cuf` in `local_en.pack` parse to the last byte).

## Where fonts live
- `font\<family>[_b]_<size>.cuf` in `local_<lang>.pack` (families: console, editor, frontend, ingame, ingame_b (bold), tooltip; sizes 6..38).
- Layout states name a font as `"<Family> <size>, <Normal|Bold>"` (e.g. `"Frontend 22, Normal"`). The mapping to a file
  (`frontend_22.cuf`, `"Ingame 14, Bold"` → `ingame_b_14.cuf`) is INFERRED from the file names; every font named in
  `fontcategories.fc` resolves to an existing file (test). The exe embeds the names `frontend_10..20.cuf` (0x0140CFE0, loading screen).
- I did not find the `.cuf` loader in Napoleon.exe (no `CUF0` immediate; it may live in a helper DLL). Layout is from the data.

## `.cuf` (CONFIRMED layout, all 74 files)
```
"CUF0"
i16 header[12]
u32 pixel_bytes
u16 char_map[65536]           UTF-16 code unit → glyph index, 0xFFFF = no glyph
glyph[header[11]]             4 bytes each: i8 top, u8 advance, u8 width, u8 height
u32 offset[header[11]]        start of each glyph's bitmap inside the pixel data (cumulative width*height)
u8  pixels[pixel_bytes]       8-bit coverage (alpha), row-major, width*height per glyph
u16 pair_count, u16 pair_first
u8  pairs[pair_count²]        pen advance for (left, right) character pairs; characters pair_first..pair_first+pair_count
```
Shipped fonts: 419 glyphs; pair table 350×350 starting at U+0021 (covers `!`..U+017E, Latin-1 + Latin Extended-A).

Header (frontend_22 / ingame_12): `[17,13,20,-1,0,15,-4,6,1,20,24,419]` / `[11,9,12,0,0,9,-2,3,1,12,16,419]`.
INFERRED: [0] line height, [1] ascent (baseline from top; glyph `top` of capitals equals it), [2]/[9] nominal size,
[5] cap/x-height, [6] descent, [7] space advance (the space glyph's advance equals it). [3], [4], [8], [10] UNKNOWN. [11] glyph count CONFIRMED.

Glyph `top` = rows above the baseline (INFERRED: capitals have top = ascent = height); `-128` (0x80) marks empty glyphs (control chars, space).

**Pen advance (PROVISIONAL):** in the data `advance == pairs[c][c'] + 1` for every unkerned pair, and kerned pairs are smaller
(`A` then `V`: 14 vs 16). Our rule: pair + 1 when both characters are in the table, else `advance`. How the state's
`font_tracking`/`font_leading` modify this is UNKNOWN (needs the text renderer in the exe).

## `ui\fontcategories.fc` (CONFIRMED by parsing, 1 file)
```
"Version044"
until EOF: str name ("sp_napoleon_battles.twui 2"), u32 index, str font, u32 leading, u32 tracking, u32 colour (ARGB)
```
(`str` = u16 length + ASCII.) The names refer to UIEd `.twui` sources; how the engine uses the categories at runtime is UNKNOWN.

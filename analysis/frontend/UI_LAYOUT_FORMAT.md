# UI layout format (`VersionNNN` binary layouts)

Reader: `crates/ntw_formats/src/ui_layout.rs` (`UiLayout::read`). Probe: `cargo run -p ntw_formats --release --example ui_probe -- all | tree "ui/frontend ui/main"`.
Real-install test: `every_ui_layout_parses` in `crates/ntw_formats/tests/real_install.rs` (`-- --ignored`).

**Result:** all 174 layouts in the install (versions 028, 029, 030, 032, 033, 039) parse and consume every byte (CONFIRMED).

## Where it comes from
The byte order and the version conditions were taken from the game's own loader in `Napoleon.exe` (Ghidra, worker1 project).
The decompiled code is not stored in the repo; this note is the spec.

| Function | Reads |
|---|---|
| `0x00DB21E0` | opens `data/UI/<folder>/<name>` and builds the root (skins folder `data/UI/<folder>/Skins/`) |
| `0x0101E270` | one component (recursive; callbacks `0x00DA92D0` child, `0x00DA9510` state, `0x00DA9470` image, `0x00DA9310` animation) |
| `0x01020A40` | component image |
| `0x01021410` | state |
| `0x01021010` | image metric (via `0x00DA6A10`) |
| `0x0103F4C0` / `0x0103F780` / `0x0103FF40` | animation / key frame / frame body |
| `0x004F3D00` | ASCII string: `u16 len` + bytes |
| `0x004F3D80` | UTF-16 string: `u16 count` + `2*count` bytes |
| `0x01024410` | two `i32` converted to a float vec2 |
| `0x0103A040` | UIComponent `GetProperty`/`SetProperty`: names the flag offsets (HotizontalResize +0xD4, VerticalResize +0xD5, Moveable +0xD6, Visible +0xD8, Priority +0xE0, RenderWhenDragged +0xE6, RenderLastOnFocused +0xE8) |

The stream object keeps the version at `+4` (parsed from the 10-byte `"VersionNNN"` header), the cursor at `+0x34`, the end at `+0x3C`
and an error flag at `+0x40` (reads past the end set the flag; our reader returns an error instead).

## Notation
`u8/u32/i32/f32` little-endian; `str` = ASCII string (u16 length); `wstr` = UTF-16 string (u16 unit count);
`v` = header version. Colours are `u32` ARGB (`0xAARRGGBB`); "colour(n)" means: if `v < n` 4 bytes R,G,B,A, else `u32`.
Field names: **C** = CONFIRMED by an exe string, **I** = INFERRED (twui field list / data), **U** = UNKNOWN meaning.

## File
```
"VersionNNN"   10 ASCII bytes
component      the root; nothing follows it
```

## Component
```
u32  this                     editor object id (images/states are referenced by these)
str  id                       C (scripts Find() by it)
i32  offset_x, i32 offset_y   C (vec2 via 0x01024410)
v<6:   u8 u8 u8               U legacy (three bytes: CONFIRMED in 0x0101E270; was two here before)
u8 allow_horizontal_resize    C +0xD4
u8 allow_vertical_resize      C +0xD5
u8 moveable                   C +0xD6
u8 visible                    C +0xD8
u8 unknown_da                 I +0xDA ClipChildren (1 exactly on the `*_clip` / `mask*` / `list_clip` components)
v>17:  u8 unknown_e5          U
v>21:  u8 render_when_dragged C
v>22:  str template, u32 template_version     I CreatedFromTemplate(+Version); version default -1
v>24:  wstr tooltip_text, wstr tooltip_label  I (label is a loc key; engine localises it)
v<16:  (v>3: skip 2 bytes)   else  u32 docking   I (values 0..9, dock point in the parent)
v>32:  u8 render_last_on_focused  C
v>37:  u32 default_state      I (a state `this`; 0 = first state)
9<=v<=12: skip 1
v<3 || v>7:  str script       I — inline Lua source attached to the component (e.g. dofile template.fe_button_standard.lua ...)
v<19:  str  (U legacy)
v<24:  skip 4
u32 n; n × image
v>27:  u32 mask_image         I (image this; resolved by id lookup 0x0105AC60)
v>31:  u32 unknown_140        U
u32 n; n × state
u32 n; n × (str key, str value)   I UserProperties
v<11:  u32 n; n × str          event function by slot index (first 30 slots)
v>=11: loop { str name; if name ∈ EVENT_NAMES: str function } until name == "events_end"   C
       u32 priority           C +0xE0
v>34:  u32 n; n × animation
u32 n; n × component          children (recursive)
v>6:   str script_override    I ScriptFileNameOverride
```
`EVENT_NAMES` (CONFIRMED, table at `0x01464170`, 30 entries): OnDrag, OnMouseMove, OnMouseLClickDown, OnMouseLClickUp, OnMouseLDblClick,
OnMouseRClickDown, OnMouseRClickUp, OnMouseRDblClick, OnMouseMClickDown, OnMouseMClickUp, OnMouseMDblClick, OnUpdatePulse, OnStartDrag,
OnMove, SetTooltip, OnDock, OnShortcut, OnAdoptChild, OnDivorceChild, OnDestroyed, OnKey, OnMouseOn, OnMouseOff, OnMouseWheelPull,
OnMouseWheelPush, OnInputFocusGain, OnInputFocusLose, OnTexturePopulate, OnAnimationFrameEnd, OnPreDraw.
A name not in the table (and not `events_end`) has no function string.

After loading, the engine looks up the component's Lua: `template.` + name, `<layout>_scripts/`, `data/ui/templates/%S`, `%S/%s.lua`
(strings at `0x013F22E4..0x013F2344`). Not yet implemented.

## Image
```
u32 this        (the loader skips it, then registers the image under the id)
str path        e.g. "UI/FrontEnd UI/Skins/logo.tga" or "data\UI\...". May be empty.
u32 width, u32 height
colour(14)      tint
```

## State
```
u32 this
str name                     NewState / normal / roll / down / inactive / selected ...
i32 width, i32 height        C (component size in this state)
v<11:  str text   else  wstr text, wstr tooltip_text
i32 text_align.0 (+0x5C, default 2), i32 text_align.1 (+0x58, default 0)    C HAlign / VAlign: 0 top 1 bottom 2 left 3 right 4 centre
i32 unknown_d4               U
i32 text_behaviour.0 (+0x90), i32 text_behaviour.1 (+0x94)   (defaults 1, 1)   C HBehaviour: 0 SplitByCharacter 1 SplitByWord 2 NeverSplit; .1 U
v>14:  u8 text_localised, wstr text_label, wstr tooltip_label   I (labels are loc keys; the engine localises them into text/tooltip)
v>4:   (v<27: u32 font_index  else  str font e.g. "Frontend 22, Normal")
       u32 font_leading (default 2), u32 font_tracking (default 1), colour(13) font_colour     I
i32 text_x_offset, i32 text_y_offset   C TextXOffset / TextYOffset (+0x60/+0x64), see "Update (s1-missions-ui)"
v<24:  skip 4
u32 unknown_d0                     U
v>5:   (v<17: 3 legacy bytes copied onto every image metric as tile/flipX/flipY)  u8 interactive   I
v>28:  u8 disabled   else derived: name == "disabled" or "inactive"                  I (the derivation is CONFIRMED)
v>11:  u8 pixel_collision          I
v>18:  str shader ("normal_t0"), 4 × f32 shader_vars     I
str enter_function, str exit_function                     I
u32 n; n × image_metric
i32, i32  editor_pos (+0xF0)       I editor-only (StateEditorDisplayPosx/y); NOT a text offset
u32 n; n × transition { u32 key, u32 value, v>33: str, u32, u32 }   I TransitionMap (U fields)
```

## Image metric
```
u32 image          image this (0x0105AC60 lookup)
i32 x, i32 y       offset inside the component
i32 width, i32 height
u32 colour
v>16:  u8 tile, u8 x_flipped, u8 y_flipped          I
v>19:  u32 dock_point, u8 can_resize_w, u8 can_resize_h   I
v>23:  f32 rotation, f32 pivot_x, f32 pivot_y        I (pivot defaults come from a global, likely 0.5)
v==30: skip 1;  v==31: skip 4
```

## Animation / frame
```
animation: str name, u8, u8, u32 n; n × frame
frame:     4 × i32 (I x, y, w, h), 4 × u8 (I colour R,G,B,A), v>35: 4 × u32 (U), u32 (I time), u32 (U),
           v>37: u32 n; n × (u32, str, str)   (U, likely frame events)
```

## Open questions (UNKNOWN)
- Meaning of state `unknown_d0/d4`; what the three DrawMode variants render differently; transition fields. (Resolved: `unknown_da`, `unknown_e5`, `unknown_140` use, state +0x60/+0x64; see the updates below.)
- Exact names of the text behaviour/align pairs and how the renderer uses them (needs the draw code).
- How docking is applied (dock point numbering) — next Ghidra target is the layout/position code using +0xDC.

## Templates (`.twui`, `uied.templates`) — status
- The binary layouts are self-contained: components created from templates are stored fully expanded (with the template name in
  `template` and `template_version`), so no template file is needed to show a layout (CONFIRMED: the main menu renders from
  `ui/frontend ui/layout` + `main` alone).
- Run-time templates (`CreateComponentFromTemplate`) are the 40 binary layouts in `ui\templates\` (all parse with this reader).
- `ui\templates\post_battle_entry.twui` is the only `.twui`: a Lua table text written by UIEd (`version = 39`), with the same fields
  under their editor names. It can be read with Lua itself if ever needed.
- `data\UI\Templates\uied.templates` (10 MB, loose file): u32 count (126), then entries starting with a 256-byte NUL-terminated name
  padded with 0xFD (e.g. "InputWindow", "BattleEditor"), then u32 values and component data in an old layout version (4); the first
  entry's component (with inline Lua) parses as layout v4 except for its framing. Entry framing is UNKNOWN; it looks like the UIEd editor
  library and is not used by any front-end layout (not needed so far).

## Update (campaign-data worker, BACKLOG §1)
- **v<6 components read three legacy bytes** (CONFIRMED in `0x0101E270`); with that fix the two oldest
  `uied.templates` entries read to their last byte: `BattleEditor` (stored version 4) and `InputWindow` (no stored
  version; v1/v2 read the same; its component carries the inline `CharacterInput` Lua). All 126 entries now read
  (`tests/real_install.rs::uied_templates_read_completely`).
- **The `uied.templates` u32 after the payload size is the entry's own absolute file offset** (CONFIRMED on all 126:
  4 for the first, then each previous offset + 264 + size). `UiTemplate::offset`.
- **`unknown_da` = ClipChildren** (INFERRED, strong): 1 on exactly the 276 components named `*_clip`, `mask*`,
  `list_clip`. `UiComponent::clips_children()`. This answers CAMPAIGN_UI.md question 7 (which children the engine
  clips).
- Value survey of the other unknowns over all 174 layouts (`cargo run -p ntw_formats --example twui_compare --
  stats | flagged`): `unknown_e5` and state `unknown_d0` are 0 everywhere; state `unknown_d4` is 0 or 2 (2 on 461
  states, mostly `Ingame` fonts); state `unknown_60` 0..28 and `unknown_64` 0..50 on text states (INFERRED text
  padding or shadow offsets; UNKNOWN). The `.twui` editor names that are not yet matched to a binary field:
  `Highlight`, `RenderIfRoot`, `UseGlobalClicks`, `DrawMode`, `CurrentState` (component) and `FocusType`,
  `Lighting`, `ZDepth`, `TextWidth`, `TextHeight`, `StateEditorDisplayPosx/y` (state); all are 0/false in the only
  `.twui`, so they cannot be matched from data; Ghidra (the draw and input code reading +0xE5, +0x140, state +0x60,
  +0x64, +0xD0, +0xD4) is needed.

## Update (shaders worker, Ghidra, BACKLOG §1 leftover)
- **`unknown_da` = ClipChildren: CONFIRMED.** The component draw (`0x01027D20`) pushes the component's own rectangle as
  the clip rectangle before it draws its children when +0xDA is set, and pops it afterwards.
- **`unknown_e5` = UseGlobalClicks: INFERRED (strong).** The mouse dispatcher (`0x00DB2350` switches on the event and
  calls one handler per mouse event, `0x0102E1F0` and 11 siblings) walks the children back to front; a component then
  takes the event when the cursor hits it **or** when +0xE5 is set, and only a component with +0xE5 clear consumes it
  (returns "handled"). So a set flag means "get every click, wherever it is, and let it through". It is 0 in all
  shipped layouts. Accessor `UiComponent::uses_global_clicks()`.
- **`unknown_140`: an inherited draw value, CONFIRMED; name `DrawMode` INFERRED.** In `0x01027D20` the child draw
  context gets `component+0x140` if it is non-zero, else the parent context's value (the same rule as the mask image
  at +0x134). The drag pass (`0x01027C30`) also copies it. Which effect mode 1 has (on `unscaled_bg`, `movie`, the
  options `image` and two tooltips) is UNKNOWN: the context is consumed inside virtual draw calls. At run time the
  loader's slot is also zeroed by the constructors (`0x0101E270`, `0x01022400`). Accessor `UiComponent::draw_mode()`.
- **State `unknown_60/64/d0/d4` stay UNKNOWN.** The state reader stores them at state +0x60, +0x64, +0xD0, +0xD4
  (CONFIRMED). These offsets are too common to scan for (hundreds of functions in the UI range use them as stack or
  other-struct offsets), and the 27 methods of the state vtable `0x013F2254` don't read them as state fields. The text
  layout code that consumes them was not found. Value facts: d0 is 0 everywhere; d4 is 0 or 2 (2 on 461 states, mostly
  `Ingame` fonts); 60 and 64 are small numbers on text states only. The remaining `.twui` names (`FocusType`,
  `Lighting`, `ZDepth`, `TextWidth`, `TextHeight`, `StateEditorDisplayPosx/y`) cannot be matched from data, since the
  one `.twui` has all of them at 0. Next step if needed: trace the text draw from the state's derived vtable (the
  reader sets the base vtable only).

## Update (s1-missions-ui worker, Ghidra, BACKLOG §1 leftover; details in `analysis/campaign/S1_MISSIONS_UI.md`)
- **State +0x60 / +0x64 = `TextXOffset` / `TextYOffset`: CONFIRMED.** The state's Lua table export (`0x0102B480`,
  `InitState`) writes `HAlign` +0x5C, `VAlign` +0x58, `DisplayWidth` +0x70, `DisplayHeight` +0x74, `TextXOffset` +0x60,
  `TextYOffset` +0x64, and reads the two offsets back; `GetStateTextDetails` / `SetStateTextDetails`
  (`0x01013EA0` / `0x01013930`, through `0x01036B20` / `0x01035CE0`) map `XOffset` / `YOffset` to the same fields;
  `SetStateTextXOffset` (`0x01013D00` → `0x01035C80`) sets +0x60 of the current state. Reader: `UiState::text_x_offset`
  / `text_y_offset`.
- **How the text is laid out and drawn: CONFIRMED.** The state's text layout (`0x010258A0`, run at load and on every
  text/size change) breaks the text into lines inside the box `width - TextXOffset` (x2 when HAlign is centre) by
  `height - TextYOffset` (x2 when VAlign is centre), passing "never split" when `HBehaviour` (+0x90) is 2, and stores
  the measured size as DisplayWidth/Height. The draw (`0x01028DB0`) places the block at `TextYOffset` (top),
  `height - DisplayHeight - TextYOffset` (VAlign 1, bottom) or `(height - DisplayHeight) / 2` (VAlign 4), and each line
  at `TextXOffset` (left), `width - line width - TextXOffset` (HAlign 3) or `(width - line width) / 2` (HAlign 4).
  Helpers `UiState::text_area / text_line_x / text_block_y / text_wraps`; applied in the front-end renderer and in
  `SetStateText`'s measure.
- **Alignment and behaviour values: CONFIRMED** from the exe's name tables: align `0x01464558` = top 0, bottom 1,
  left 2, right 3, centre 4; HBehaviour `0x01464570` = SplitByCharacter 0, SplitByWord 1, NeverSplit 2.
  (`VAlign` 2 occurs in 1546 states and draws as top.) `text_behaviour.1` (+0x94) stays UNKNOWN.
- **The +0xF0 pair is not a text offset.** Neither the layout nor the draw reads it, and 350 states carry the MSVC
  fill pattern 0xCDCDCDCD / 0xCDCDCDC0 there: INFERRED the editor-only `StateEditorDisplayPosx/y`. The front end used to
  add it to the text position (wrong); it is now `UiState::editor_pos` and unused.
- **DrawMode (+0x140), its effect: partly CONFIRMED.** It is passed down as draw context +0x34. The image draw
  (`0x01028510`, arg 20) calls one of three sprite-batch virtuals, `+0x5C` (mode 0), `+0x64` (1) or `+0x6C` (2), and
  draws nothing for other values; the text draw picks one of three render-state pairs of the device (`+0x274/+0x278`,
  `+0x288/+0x284`, `+0x27C/+0x280`). What differs between the variants is UNKNOWN (inside the sprite batch, not
  traced). INFERRED (weak): mode 1 = the `no_clip` sprite technique (technique table `0x01467D70` lists
  `no_clip_no_gamma_t0` and `no_clip_t0` at 30/31), which fits its users (full-screen background, movie, tooltips).
  Not applied in our renderer (we do not clip children to their parents yet, so nothing would change).
- **State +0xD0 / +0xD4 stay UNKNOWN.** Besides the loader and the state constructor, no UI function reads them
  (searched the UI code `0x01010000..0x01060000` for every function using those offsets together with other state
  fields; the hits are other classes). INFERRED: editor-only or unused at run time.

## Update (pathfinding worker, Ghidra, 2026-10-04): DrawMode decoded
- **What the three DrawMode variants do: CONFIRMED.** The sprite batch is the UI renderer with vtable `0x0140CAAC`
  (constructor `0x01133750`; +0x10 = the render device, vtable `0x0140CC30`). Its draw slots `+0x5C` (mode 0,
  `0x01165870`), `+0x64` (mode 1, `0x011668D0`) and `+0x6C` (mode 2, `0x01166210`) are the same code except for the
  two device virtuals that turn the image's size and position into screen (clip) space:
  - **mode 0 (normal):** size `+0x278` (`0x011617C0`) and position `+0x274` (`0x011881C0`) **scaled by the UI scale**
    (`0x0114EB20`, INFERRED: the window resolution over the layout's authored resolution) with the component's anchor;
    when the device's "no UI scaling" flag (device +0x69C70) is set they fall through to the mode 1 functions;
  - **mode 1 (unscaled):** size `+0x284` (`0x011AD330`) and position `+0x288` (`0x011AD3E0`) **1:1 in window pixels**
    (no UI scale), which matches its users' names (`unscaled_bg`, the movie, the options image, two tooltips);
  - **mode 2 (full screen):** size `+0x280` (`0x01161770`) is the whole screen and position `+0x27C` (`0x01188100`) its
    top-left corner: the image **stretches over the whole screen**, ignoring its own position and size. No shipped
    layout uses mode 2.
  The text draw (`0x01028DB0`) uses the same pairs for its placement. Our front end draws at the layouts' authored
  resolution (fixed window size), where modes 0 and 1 give the same picture; the UI scale and mode 2 are not
  implemented (PROVISIONAL, nothing ships with mode 2).
- **State +0xD0 / +0xD4:** also not read by the text layout `0x010258A0` or the text draw `0x01028DB0` (checked); still
  UNKNOWN, INFERRED editor-only.

## Update (save-compat worker, Ghidra, 2026-10-04): state +0xD0 / +0xD4 settled
- **CONFIRMED unused at run time (loaded only).** A scalar sweep of the whole UI code (`0x01000000..0x01060000`) for every
  instruction with displacement 0xD0 or 0xD4, restricted to functions that also use the state's text fields (+0x90, +0xBC,
  +0xC8), finds only: the state reader `0x01021410` storing them (`0x01021801` +0xD4, `0x01021B13` +0xD0) and zeroing them
  (`0x01021585` / `0x0102158F`), constructor-style writes in other classes, stack slots, and the component flag copy
  (`0x0101FD66`, component +0xD4 = HorizontalResize, not the state). No read of a state's +0xD0/+0xD4 exists, and the Lua
  state export `0x0102B480` does not name them. Shipped values: +0xD0 0 everywhere, +0xD4 0 or 2. The reader keeps them
  so a layout round-trips; nothing else is needed.

## Update (save-compat worker, Ghidra, 2026-10-04): `unknown_e5` = UseGlobalClicks, behaviour CONFIRMED
- Every use of component +0xE5 in the UI code (`0x01000000`..`0x01040000`): the loader `0x0101E270` (reads it), the
  copy `0x0101FC20`, and two tests in each of the 11 mouse handlers (`0x0102E1F0`, `0x0102E340`, `0x0102E4B0`, ...,
  called by the dispatcher `0x00DB2350`). In a handler, after the children have had the event, the component takes it
  when the cursor hits it **or** +0xE5 is set; it then fires its mouse event, and returns "handled" only when +0xE5 is
  clear. So a set flag means "receive every click anywhere, and let it through": CONFIRMED.
- The name: Napoleon.exe has no field-name string for this flag (nor `ClipChildren`; ASCII and UTF-16 searched). The
  name comes from the UIEd field list, where `UseGlobalClicks` is the only click-related field not yet matched; the
  same footing as `ClipChildren` (+0xDA). 0 in every shipped layout, so our front end needs no handling beyond the
  accessor `UiComponent::uses_global_clicks()`.

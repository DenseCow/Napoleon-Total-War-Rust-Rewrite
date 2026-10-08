//! Binary UI layout files (`"VersionNNN"` header): the original menus and HUD screens.
//!
//! These are the extensionless files under `ui\` in `data.pack` / `boot.pack`, for example
//! `ui\frontend ui\main` (the main menu) or `ui\campaign ui\layout`. Each file is one tree of
//! **components** (a panel, button, image or text box). A component has **images** (texture
//! paths), **states** (`"NewState"`, `"active"`, `"hover"`, ...) that say how to draw it, event
//! bindings to Lua functions, and child components.
//!
//! The field order was taken from the game's own loader (component reader at `0x0101E270`, state
//! reader `0x01021410`, image `0x01020A40`, image metric `0x01021010`, animation `0x0103F4C0`,
//! frame `0x0103F780`/`0x0103FF40`) and checked against every layout in the install. Full spec,
//! evidence tags and the version table: `analysis/frontend/UI_LAYOUT_FORMAT.md`.
//!
//! **Evidence:** the byte *order* and the version conditions are CONFIRMED (the loader, plus all
//! 174 files parse to their last byte). Field *names* are CONFIRMED where the exe names them
//! (`GetProperty` strings or the event table), otherwise INFERRED from the UIEd `.twui` text
//! format's field list; fields with no known meaning are kept as `unknown_*` (UNKNOWN).
//!
//! Colours are `u32` **ARGB** (`0xAARRGGBB`).

use std::fmt;

use crate::bytes::{Cursor, ReadError};

/// The 30 event slots a component can bind a Lua function to, in the exe's table order
/// (`0x01464170`, CONFIRMED). An event name that is not in this list carries no function string.
pub const EVENT_NAMES: [&str; 30] = [
    "OnDrag",
    "OnMouseMove",
    "OnMouseLClickDown",
    "OnMouseLClickUp",
    "OnMouseLDblClick",
    "OnMouseRClickDown",
    "OnMouseRClickUp",
    "OnMouseRDblClick",
    "OnMouseMClickDown",
    "OnMouseMClickUp",
    "OnMouseMDblClick",
    "OnUpdatePulse",
    "OnStartDrag",
    "OnMove",
    "SetTooltip",
    "OnDock",
    "OnShortcut",
    "OnAdoptChild",
    "OnDivorceChild",
    "OnDestroyed",
    "OnKey",
    "OnMouseOn",
    "OnMouseOff",
    "OnMouseWheelPull",
    "OnMouseWheelPush",
    "OnInputFocusGain",
    "OnInputFocusLose",
    "OnTexturePopulate",
    "OnAnimationFrameEnd",
    "OnPreDraw",
];

/// Terminates the named event list (CONFIRMED string at `0x013F22CC`).
pub const EVENTS_END: &str = "events_end";

/// A parsed layout file.
#[derive(Debug, Clone, PartialEq)]
pub struct UiLayout {
    /// The number in the `"VersionNNN"` header (28 to 39 in the shipped files).
    pub version: u32,
    /// The root component (usually id `"root"`).
    pub root: UiComponent,
}

/// One node of the UI tree.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiComponent {
    /// Editor object id ("this"); images and states refer to each other by these ids.
    pub this: u32,
    /// Component name, used by scripts (`Find("button_campaign")`).
    pub id: String,
    /// Position relative to the parent (pixels).
    pub offset: (i32, i32),
    /// CONFIRMED (`GetProperty "HotizontalResize"`, +0xD4).
    pub allow_horizontal_resize: bool,
    /// CONFIRMED (`"VerticalResize"`, +0xD5).
    pub allow_vertical_resize: bool,
    /// CONFIRMED (`"Moveable"`, +0xD6).
    pub moveable: bool,
    /// CONFIRMED (`"Visible"`, +0xD8).
    pub visible: bool,
    /// Byte at +0xDA: the UIEd field `ClipChildren` (see [`UiComponent::clips_children`]). CONFIRMED in
    /// the draw code (`0x01027D20`): when set, the component pushes its own rectangle as the clip
    /// rectangle before drawing its children and pops it after. It is 1 on exactly the components named
    /// `*_clip`, `mask*` and `list_clip` (276 of 7,383 in the install).
    pub unknown_da: u8,
    /// Byte at +0xE5, version > 17: the UIEd field `UseGlobalClicks` (see
    /// [`UiComponent::uses_global_clicks`]). Behaviour CONFIRMED: in the mouse handlers (`0x0102E1F0` and
    /// its 11 siblings, one per mouse event) a component with this set receives the click even when the
    /// cursor is not over it, and does not consume it (the components behind still get it). The only
    /// other uses in the UI code are the loader `0x0101E270` and the copy `0x0101FC20`. The name comes
    /// from the editor's field list: the exe holds no field-name string for it (nor for `ClipChildren`).
    /// 0 in every shipped layout.
    pub unknown_e5: u8,
    /// CONFIRMED (`"RenderWhenDragged"`, +0xE6), version > 21.
    pub render_when_dragged: bool,
    /// INFERRED "CreatedFromTemplate" (the template name), version > 22.
    pub template: String,
    /// INFERRED "CreatedFromTemplateVersion" (default -1), version > 22.
    pub template_version: u32,
    /// Tooltip text (UTF-16), version > 24. INFERRED.
    pub tooltip_text: String,
    /// Tooltip localisation key (UTF-16), version > 24. INFERRED (the engine looks it up in the loc tables).
    pub tooltip_label: String,
    /// INFERRED "Docking" (dock point 0..9 inside the parent), +0xDC; read for version >= 16.
    pub docking: u32,
    /// CONFIRMED (`"RenderLastOnFocused"`, +0xE8), version > 32.
    pub render_last_on_focused: bool,
    /// The state (`this` id) the component starts in; 0 means the first state. Version > 37.
    pub default_state: u32,
    /// INFERRED "script": a Lua file attached to the component.
    pub script: String,
    /// Images (textures) used by the states' image metrics.
    pub images: Vec<UiImage>,
    /// INFERRED "MaskImage" (an image `this` id, 0 = none), version > 27.
    pub mask_image: u32,
    /// u32 at +0x140, version > 31; 1 on `unscaled_bg`, `movie`, the options `image` and two tooltips,
    /// else 0. CONFIRMED (`0x01027D20`): a draw-context value (context +0x34) that children inherit;
    /// when non-zero it replaces the parent's value, 0 means "inherit". INFERRED: the UIEd field
    /// `DrawMode` (see [`UiComponent::draw_mode`]). CONFIRMED use: the mode (0, 1 or 2) picks which of
    /// three sprite-batch calls draws every image of the subtree (`0x01028510`: batch virtuals
    /// `+0x5C` / `+0x64` / `+0x6C`; any other value draws no images) and which of three render-state
    /// pairs the text uses (`0x01028DB0`). CONFIRMED effect (UI_LAYOUT_FORMAT.md, 2026-10-04): mode 0
    /// scales size and position with the UI scale, mode 1 draws 1:1 in window pixels (unscaled),
    /// mode 2 stretches the image over the whole screen.
    pub unknown_140: u32,
    /// The visual states. The first one is the default unless [`default_state`](Self::default_state) is set.
    pub states: Vec<UiState>,
    /// INFERRED "UserProperties": key/value string pairs.
    pub properties: Vec<(String, String)>,
    /// Event bindings: (event name from [`EVENT_NAMES`], Lua function name).
    pub events: Vec<(String, String)>,
    /// CONFIRMED (`"Priority"`, +0xE0): draw/input order among siblings.
    pub priority: u32,
    /// Animations, version > 34.
    pub animations: Vec<UiAnimation>,
    /// Child components, in file order.
    pub children: Vec<UiComponent>,
    /// INFERRED "ScriptFileNameOverride" (e.g. `template.row_template_post_battle.lua`), version > 6.
    pub script_override: String,
}

/// A texture used by a component.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiImage {
    /// Object id; [`UiImageMetric::image`] refers to it.
    pub this: u32,
    /// Path as written by the editor, e.g. `data\UI\FrontEnd UI\Skins\fe_background_2.tga`. May be empty.
    pub path: String,
    /// Texture size in pixels as recorded by the editor.
    pub width: u32,
    /// Texture size in pixels as recorded by the editor.
    pub height: u32,
    /// Tint colour (ARGB).
    pub colour: u32,
}

/// One visual state of a component.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiState {
    /// Object id.
    pub this: u32,
    /// State name: `NewState`, `active`, `hover`, `down`, `selected`, `inactive`, ...
    pub name: String,
    /// Component size in this state.
    pub width: i32,
    /// Component size in this state.
    pub height: i32,
    /// Literal text (UTF-16).
    pub text: String,
    /// Tooltip text (UTF-16). INFERRED.
    pub tooltip_text: String,
    /// Text alignment (horizontal `HAlign`, vertical `VAlign`) at +0x5C/+0x58 (defaults 2, 0).
    /// CONFIRMED values (the exe's name table `0x01464558` and the draw `0x01028DB0`): 0 top,
    /// 1 bottom, 2 left, 3 right, 4 centre ([`ALIGN_TOP`] ...). The draw treats any horizontal value
    /// other than 3 / 4 as left and any vertical value other than 1 / 4 as top.
    pub text_align: (i32, i32),
    /// UNKNOWN i32 (+0xD4; 0 or 2). No reader in the UI code besides the loader and the state
    /// constructor was found (round s1-missions-ui).
    pub unknown_d4: i32,
    /// (+0x90, +0x94; defaults 1, 1). `.0` is CONFIRMED `HBehaviour` (`Get/SetStateTextDetails`,
    /// name table `0x01464570`): 0 SplitByCharacter, 1 SplitByWord, 2 NeverSplit; the text layout
    /// (`0x010258A0`) passes "never split" (`.0 == 2`) to the line breaker. `.1` UNKNOWN (INFERRED
    /// the UIEd `TextVBehaviour`).
    pub text_behaviour: (i32, i32),
    /// INFERRED "TextLocalised": when set, [`text_label`](Self::text_label) is a loc key that replaces `text`.
    pub text_localised: bool,
    /// Loc key for the text (UTF-16). INFERRED "TextLabel".
    pub text_label: String,
    /// Loc key for the tooltip (UTF-16). INFERRED "TooltipLabel".
    pub tooltip_label: String,
    /// Font name such as `"Ingame 12, Normal"` (version >= 27; older files store an index, see `font_index`).
    pub font: String,
    /// Font index for version < 27 files.
    pub font_index: Option<u32>,
    /// INFERRED "Fontleading" (default 2).
    pub font_leading: u32,
    /// INFERRED "Fonttracking" (default 1).
    pub font_tracking: u32,
    /// Text colour (ARGB). INFERRED "Fontcolour".
    pub font_colour: u32,
    /// CONFIRMED `TextXOffset` (+0x60; the state's Lua table export `0x0102B480` and
    /// `Get/SetStateTextDetails` `XOffset`): the horizontal text inset, see [`UiState::text_line_x`].
    pub text_x_offset: i32,
    /// CONFIRMED `TextYOffset` (+0x64): the vertical text inset, see [`UiState::text_block_y`].
    pub text_y_offset: i32,
    /// UNKNOWN (+0xD0; 0 in every layout; no reader found besides the loader).
    pub unknown_d0: u32,
    /// INFERRED "Interactive" (+0x114).
    pub interactive: bool,
    /// INFERRED "Disabled" (+0x115). Older files derive it from the state name `disabled`/`inactive`.
    pub disabled: bool,
    /// INFERRED "PixelCollision" (+0x116), version > 11.
    pub pixel_collision: bool,
    /// Shader technique, e.g. `normal_t0`, version > 18.
    pub shader: String,
    /// Four shader variables, version > 18.
    pub shader_vars: [f32; 4],
    /// INFERRED "EnterStateFunction".
    pub enter_function: String,
    /// INFERRED "ExitStateFunction".
    pub exit_function: String,
    /// Where each image is drawn in this state, in draw order.
    pub image_metrics: Vec<UiImageMetric>,
    /// The pair at +0xF0 (read after the image metrics). Not a text offset: the text layout
    /// (`0x010258A0`) and draw (`0x01028DB0`) never read it (CONFIRMED), and 350 states hold the
    /// MSVC fill pattern 0xCDCDCDCD / 0xCDCDCDC0 in it. INFERRED: the editor-only
    /// `StateEditorDisplayPosx/y`.
    pub editor_pos: (i32, i32),
    /// INFERRED "TransitionMap" entries.
    pub transitions: Vec<UiTransition>,
}

/// Placement of one image inside a state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiImageMetric {
    /// The [`UiImage::this`] id of the image to draw (0 = none).
    pub image: u32,
    /// Position inside the component.
    pub offset: (i32, i32),
    /// Drawn size (pixels).
    pub width: i32,
    /// Drawn size (pixels).
    pub height: i32,
    /// Tint colour (ARGB).
    pub colour: u32,
    /// INFERRED "Tile", version > 16.
    pub tile: bool,
    /// INFERRED "X_Flipped", version > 16.
    pub x_flipped: bool,
    /// INFERRED "Y_Flipped", version > 16.
    pub y_flipped: bool,
    /// INFERRED "DockPoint", version > 19.
    pub dock_point: u32,
    /// INFERRED "CanResizeWidth", version > 19.
    pub can_resize_width: bool,
    /// INFERRED "CanResizeHeight", version > 19.
    pub can_resize_height: bool,
    /// INFERRED "rotation_angle", version > 23.
    pub rotation: f32,
    /// INFERRED "pivot_point", version > 23.
    pub pivot: (f32, f32),
}

/// A state transition entry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiTransition {
    /// UNKNOWN key (u32).
    pub key: u32,
    /// UNKNOWN value (u32; a nonzero value creates the entry).
    pub value: u32,
    /// Version > 33: UNKNOWN string.
    pub name: String,
    /// Version > 33: UNKNOWN.
    pub a: u32,
    /// Version > 33: UNKNOWN.
    pub b: u32,
}

/// A component animation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiAnimation {
    /// Animation name.
    pub name: String,
    /// UNKNOWN flags.
    pub flags: [u8; 2],
    /// Key frames.
    pub frames: Vec<UiAnimFrame>,
}

/// One animation key frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiAnimFrame {
    /// Four i32 (INFERRED x, y, width, height).
    pub rect: [i32; 4],
    /// Colour bytes as stored (INFERRED R, G, B, A).
    pub colour: [u8; 4],
    /// Version > 35: four more values (UNKNOWN).
    pub extra: [u32; 4],
    /// UNKNOWN (INFERRED time).
    pub a: u32,
    /// UNKNOWN.
    pub b: u32,
    /// Version > 37: (u32, string, string) entries, UNKNOWN meaning.
    pub events: Vec<(u32, String, String)>,
}

/// Errors from [`UiLayout::read`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiLayoutError {
    /// The file does not start with `"Version"` + 3 digits.
    BadHeader,
    /// The data ended early.
    UnexpectedEof {
        /// Where the read started.
        offset: usize,
        /// Bytes needed.
        needed: usize,
    },
    /// A UTF-16 string was malformed.
    InvalidUtf16 {
        /// Where the string starts.
        offset: usize,
    },
    /// Bytes remain after the root component.
    TrailingBytes {
        /// Where they start.
        offset: usize,
        /// How many.
        count: usize,
    },
    /// A count is larger than the file could hold (corrupt data).
    BadCount {
        /// Where the count was read.
        offset: usize,
        /// The value.
        count: u32,
    },
}

impl From<ReadError> for UiLayoutError {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Eof { offset, needed } => Self::UnexpectedEof { offset, needed },
            ReadError::Utf16 { offset } => Self::InvalidUtf16 { offset },
        }
    }
}

impl fmt::Display for UiLayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadHeader => write!(f, "not a UI layout (missing \"VersionNNN\" header)"),
            Self::UnexpectedEof { offset, needed } => {
                write!(f, "layout data ended at 0x{offset:x} (needed {needed} more bytes)")
            }
            Self::InvalidUtf16 { offset } => write!(f, "invalid UTF-16 string at 0x{offset:x}"),
            Self::TrailingBytes { offset, count } => write!(f, "{count} unexpected bytes at 0x{offset:x}"),
            Self::BadCount { offset, count } => write!(f, "implausible count {count} at 0x{offset:x}"),
        }
    }
}

impl std::error::Error for UiLayoutError {}

/// True if `bytes` starts with a layout header (`"Version"` + 3 ASCII digits).
pub fn is_layout(bytes: &[u8]) -> bool {
    bytes.len() >= 10 && &bytes[..7] == b"Version" && bytes[7..10].iter().all(u8::is_ascii_digit)
}

impl UiLayout {
    /// Parses a whole layout file. Every byte must be consumed.
    pub fn read(bytes: &[u8]) -> Result<Self, UiLayoutError> {
        if !is_layout(bytes) {
            return Err(UiLayoutError::BadHeader);
        }
        let version: u32 = std::str::from_utf8(&bytes[7..10]).map_err(|_| UiLayoutError::BadHeader)?.parse().map_err(|_| UiLayoutError::BadHeader)?;
        let mut r = Reader { c: Cursor::new(bytes), v: version, child_prefix: false, embedded: Vec::new() };
        r.c.take(10)?;
        let root = r.component()?;
        if r.c.remaining() != 0 {
            return Err(UiLayoutError::TrailingBytes { offset: r.c.pos(), count: r.c.remaining() });
        }
        Ok(Self { version, root })
    }
}

/// Reads one component tree (no `"VersionNNN"` header) written with layout `version`,
/// starting at the beginning of `bytes`. Returns the component and the number of bytes used.
/// Used for embedded component data such as the template library.
pub fn read_component(bytes: &[u8], version: u32) -> Result<(UiComponent, usize), UiLayoutError> {
    let mut r = Reader { c: Cursor::new(bytes), v: version, child_prefix: false, embedded: Vec::new() };
    let c = r.component()?;
    Ok((c, r.c.pos()))
}

/// Embedded images: {ASCII path, TGA file bytes}.
pub type EmbeddedImages = Vec<(String, Vec<u8>)>;

/// Like [`read_component`], for the template library `uied.templates`, where every child
/// component is preceded by its embedded images: u32 count, then each {ASCII path, TGA file}
/// (the same framing as the entry's own images, INFERRED from the `Tooltip` entry). Returns the
/// component, the bytes used and the children's embedded images.
pub fn read_template_component(bytes: &[u8], version: u32) -> Result<(UiComponent, usize, EmbeddedImages), UiLayoutError> {
    let mut r = Reader { c: Cursor::new(bytes), v: version, child_prefix: true, embedded: Vec::new() };
    let c = r.component()?;
    Ok((c, r.c.pos(), r.embedded))
}

/// [`UiState::text_align`] value `top` (CONFIRMED name table `0x01464558`).
pub const ALIGN_TOP: i32 = 0;
/// [`UiState::text_align`] value `bottom`.
pub const ALIGN_BOTTOM: i32 = 1;
/// [`UiState::text_align`] value `left`.
pub const ALIGN_LEFT: i32 = 2;
/// [`UiState::text_align`] value `right`.
pub const ALIGN_RIGHT: i32 = 3;
/// [`UiState::text_align`] value `centre`.
pub const ALIGN_CENTRE: i32 = 4;
/// [`UiState::text_behaviour`] `.0` value `SplitByCharacter` (CONFIRMED name table `0x01464570`).
pub const SPLIT_BY_CHARACTER: i32 = 0;
/// [`UiState::text_behaviour`] `.0` value `SplitByWord`.
pub const SPLIT_BY_WORD: i32 = 1;
/// [`UiState::text_behaviour`] `.0` value `NeverSplit`.
pub const NEVER_SPLIT: i32 = 2;

/// The text placement rules of the exe's text layout (`0x010258A0`) and draw (`0x01028DB0`),
/// CONFIRMED. Sizes are in layout pixels.
impl UiState {
    /// The box the text is broken into lines for, for a component of `w` x `h`: the size minus
    /// the text offsets, taken off both sides when the text is centred on that axis.
    pub fn text_area(&self, w: f32, h: f32) -> (f32, f32) {
        let inset = |off: i32, align: i32| off as f32 * if align == ALIGN_CENTRE { 2.0 } else { 1.0 };
        (
            w - inset(self.text_x_offset, self.text_align.0),
            h - inset(self.text_y_offset, self.text_align.1),
        )
    }

    /// Whether the text may be broken into lines (`HBehaviour` is not NeverSplit).
    pub fn text_wraps(&self) -> bool {
        self.text_behaviour.0 != NEVER_SPLIT
    }

    /// The x of a text line `line_w` wide, relative to the component's left edge (`w` wide):
    /// right: `w - line_w - TextXOffset`; centre: `(w - line_w) / 2`; otherwise `TextXOffset`.
    pub fn text_line_x(&self, w: f32, line_w: f32) -> f32 {
        match self.text_align.0 {
            ALIGN_RIGHT => w - line_w - self.text_x_offset as f32,
            ALIGN_CENTRE => (w - line_w) * 0.5,
            _ => self.text_x_offset as f32,
        }
    }

    /// The y of the first line of a text block `block_h` high, relative to the component's top
    /// edge (`h` high): bottom: `h - block_h - TextYOffset`; centre: `(h - block_h) / 2`;
    /// otherwise `TextYOffset`.
    pub fn text_block_y(&self, h: f32, block_h: f32) -> f32 {
        match self.text_align.1 {
            ALIGN_BOTTOM => h - block_h - self.text_y_offset as f32,
            ALIGN_CENTRE => (h - block_h) * 0.5,
            _ => self.text_y_offset as f32,
        }
    }
}

impl UiComponent {
    /// Depth-first search for a descendant (or self) with this id.
    pub fn find(&self, id: &str) -> Option<&UiComponent> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    /// The state the component starts in: [`default_state`](Self::default_state) if set, else the first.
    pub fn initial_state(&self) -> Option<&UiState> {
        if self.default_state != 0
            && let Some(s) = self.states.iter().find(|s| s.this == self.default_state)
        {
            return Some(s);
        }
        self.states.first()
    }

    /// A state by name.
    pub fn state(&self, name: &str) -> Option<&UiState> {
        self.states.iter().find(|s| s.name == name)
    }

    /// An image by its `this` id.
    pub fn image(&self, this: u32) -> Option<&UiImage> {
        self.images.iter().find(|i| i.this == this)
    }

    /// The Lua function bound to an event, if any.
    pub fn event(&self, name: &str) -> Option<&str> {
        self.events.iter().find(|(e, _)| e == name).map(|(_, f)| f.as_str())
    }

    /// True if the component clips its children to its own rectangle (INFERRED, see
    /// [`UiComponent::unknown_da`]).
    pub fn clips_children(&self) -> bool {
        self.unknown_da != 0
    }

    /// True if the component takes mouse clicks anywhere on screen without consuming them (CONFIRMED
    /// behaviour; `UseGlobalClicks`, see [`UiComponent::unknown_e5`]).
    pub fn uses_global_clicks(&self) -> bool {
        self.unknown_e5 != 0
    }

    /// The draw mode this component passes to its children (INFERRED `DrawMode`, see
    /// [`UiComponent::unknown_140`]): its own value, or `parent` when its own is 0.
    pub fn draw_mode(&self, parent: u32) -> u32 {
        if self.unknown_140 != 0 {
            self.unknown_140
        } else {
            parent
        }
    }

    /// Number of components in this subtree (including self).
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(UiComponent::count).sum::<usize>()
    }
}

struct Reader<'a> {
    c: Cursor<'a>,
    v: u32,
    /// Template-library data: a u32 before each child (see [`read_template_component`]).
    child_prefix: bool,
    /// Images embedded before template children: (path, TGA file bytes).
    embedded: Vec<(String, Vec<u8>)>,
}

impl Reader<'_> {
    fn b(&mut self) -> Result<bool, UiLayoutError> {
        Ok(self.c.u8()? != 0)
    }

    /// Reads a u32 count and rejects values the remaining bytes could not possibly hold.
    fn count(&mut self, min_item: usize) -> Result<usize, UiLayoutError> {
        let offset = self.c.pos();
        let n = self.c.u32()?;
        if (n as usize).saturating_mul(min_item.max(1)) > self.c.remaining() {
            return Err(UiLayoutError::BadCount { offset, count: n });
        }
        Ok(n as usize)
    }

    fn skip(&mut self, n: usize) -> Result<(), UiLayoutError> {
        self.c.take(n)?;
        Ok(())
    }

    /// Colour: v >= 14 a u32 ARGB, older files 4 bytes R, G, B, A.
    fn colour(&mut self, new_from: u32) -> Result<u32, UiLayoutError> {
        if self.v < new_from {
            let b = self.c.take(4)?;
            Ok(u32::from_le_bytes([b[2], b[1], b[0], b[3]]))
        } else {
            Ok(self.c.u32()?)
        }
    }

    fn component(&mut self) -> Result<UiComponent, UiLayoutError> {
        let v = self.v;
        let mut c = UiComponent { this: self.c.u32()?, id: self.c.ascii()?, ..Default::default() };
        c.offset = (self.c.i32()?, self.c.i32()?);
        if v < 6 {
            self.skip(3)?; // UNKNOWN legacy bytes: three u8 (CONFIRMED in 0x0101E270)
        }
        c.allow_horizontal_resize = self.b()?;
        c.allow_vertical_resize = self.b()?;
        c.moveable = self.b()?;
        c.visible = self.b()?;
        c.unknown_da = self.c.u8()?;
        if v > 17 {
            c.unknown_e5 = self.c.u8()?;
        }
        if v > 21 {
            c.render_when_dragged = self.b()?;
        }
        c.template_version = u32::MAX;
        if v > 22 {
            c.template = self.c.ascii()?;
            c.template_version = self.c.u32()?;
        }
        if v > 24 {
            c.tooltip_text = self.c.utf16()?;
            c.tooltip_label = self.c.utf16()?;
        }
        if v < 16 {
            if v > 3 {
                self.skip(2)?;
            }
        } else {
            c.docking = self.c.u32()?;
        }
        if v > 32 {
            c.render_last_on_focused = self.b()?;
        }
        if v > 37 {
            c.default_state = self.c.u32()?;
        }
        if (9..=12).contains(&v) {
            self.skip(1)?;
        }
        if !(3..=7).contains(&v) {
            c.script = self.c.ascii()?;
        }
        if v < 19 {
            self.c.ascii()?; // legacy string (UNKNOWN)
        }
        if v < 24 {
            self.skip(4)?;
        }
        for _ in 0..self.count(14)? {
            c.images.push(self.image()?);
        }
        if v > 27 {
            c.mask_image = self.c.u32()?;
        }
        if v > 31 {
            c.unknown_140 = self.c.u32()?;
        }
        for _ in 0..self.count(20)? {
            c.states.push(self.state()?);
        }
        for _ in 0..self.count(4)? {
            c.properties.push((self.c.ascii()?, self.c.ascii()?));
        }
        if v < 11 {
            for i in 0..self.count(2)? {
                let f = self.c.ascii()?;
                if let Some(name) = EVENT_NAMES.get(i)
                    && !f.is_empty()
                {
                    c.events.push(((*name).to_owned(), f));
                }
            }
        } else {
            loop {
                let name = self.c.ascii()?;
                if EVENT_NAMES.contains(&name.as_str()) {
                    let f = self.c.ascii()?;
                    c.events.push((name, f));
                } else if name == EVENTS_END {
                    break;
                }
            }
            c.priority = self.c.u32()?;
        }
        if v > 34 {
            for _ in 0..self.count(6)? {
                c.animations.push(self.animation()?);
            }
        }
        for _ in 0..self.count(20)? {
            if self.child_prefix {
                for _ in 0..self.count(3)? {
                    let path = self.c.ascii()?;
                    let rest = self.c.peek(self.c.remaining()).unwrap_or(&[]);
                    let len = crate::ui_templates::tga_len(rest).ok_or(UiLayoutError::BadCount { offset: self.c.pos(), count: 0 })?;
                    let bytes = self.c.take(len)?.to_vec();
                    self.embedded.push((path, bytes));
                }
            }
            c.children.push(self.component()?);
        }
        if v > 6 {
            c.script_override = self.c.ascii()?;
        }
        Ok(c)
    }

    fn image(&mut self) -> Result<UiImage, UiLayoutError> {
        Ok(UiImage {
            this: self.c.u32()?,
            path: self.c.ascii()?,
            width: self.c.u32()?,
            height: self.c.u32()?,
            colour: self.colour(14)?,
        })
    }

    fn state(&mut self) -> Result<UiState, UiLayoutError> {
        let v = self.v;
        let mut s = UiState { this: self.c.u32()?, name: self.c.ascii()?, ..Default::default() };
        s.width = self.c.i32()?;
        s.height = self.c.i32()?;
        if v < 11 {
            s.text = self.c.ascii()?;
        } else {
            s.text = self.c.utf16()?;
            s.tooltip_text = self.c.utf16()?;
        }
        s.text_align = (self.c.i32()?, self.c.i32()?);
        s.unknown_d4 = self.c.i32()?;
        s.text_behaviour = (self.c.i32()?, self.c.i32()?);
        if v > 14 {
            s.text_localised = self.b()?;
            s.text_label = self.c.utf16()?;
            s.tooltip_label = self.c.utf16()?;
        }
        s.font_leading = 2;
        s.font_tracking = 1;
        s.font_colour = 0xFF;
        if v > 4 {
            if v < 27 {
                s.font_index = Some(self.c.u32()?);
            } else {
                s.font = self.c.ascii()?;
            }
            s.font_leading = self.c.u32()?;
            s.font_tracking = self.c.u32()?;
            s.font_colour = self.colour(13)?;
        }
        s.text_x_offset = self.c.i32()?;
        s.text_y_offset = self.c.i32()?;
        if v < 24 {
            self.skip(4)?;
        }
        s.unknown_d0 = self.c.u32()?;
        if v > 5 {
            if v < 17 {
                self.skip(3)?; // legacy per-metric tile/flip flags, copied onto every metric (not kept)
            }
            s.interactive = self.b()?;
        }
        if v > 28 {
            s.disabled = self.b()?;
        } else {
            let n = s.name.to_ascii_lowercase();
            s.disabled = n == "disabled" || n == "inactive";
        }
        if v > 11 {
            s.pixel_collision = self.b()?;
        }
        if v > 18 {
            s.shader = self.c.ascii()?;
            for x in &mut s.shader_vars {
                *x = self.c.f32()?;
            }
        }
        s.enter_function = self.c.ascii()?;
        s.exit_function = self.c.ascii()?;
        for _ in 0..self.count(24)? {
            s.image_metrics.push(self.metric()?);
        }
        s.editor_pos = (self.c.i32()?, self.c.i32()?);
        for _ in 0..self.count(8)? {
            let mut t = UiTransition { key: self.c.u32()?, value: self.c.u32()?, ..Default::default() };
            if v > 33 {
                t.name = self.c.ascii()?;
                t.a = self.c.u32()?;
                t.b = self.c.u32()?;
            }
            s.transitions.push(t);
        }
        Ok(s)
    }

    fn metric(&mut self) -> Result<UiImageMetric, UiLayoutError> {
        let v = self.v;
        let mut m = UiImageMetric {
            image: self.c.u32()?,
            offset: (self.c.i32()?, self.c.i32()?),
            width: self.c.i32()?,
            height: self.c.i32()?,
            colour: self.c.u32()?,
            can_resize_width: true,
            can_resize_height: true,
            ..Default::default()
        };
        if v > 16 {
            m.tile = self.b()?;
            m.x_flipped = self.b()?;
            m.y_flipped = self.b()?;
        }
        if v > 19 {
            m.dock_point = self.c.u32()?;
            m.can_resize_width = self.b()?;
            m.can_resize_height = self.b()?;
        }
        if v > 23 {
            m.rotation = self.c.f32()?;
            m.pivot = (self.c.f32()?, self.c.f32()?);
        }
        match v {
            30 => self.skip(1)?,
            31 => self.skip(4)?,
            _ => {}
        }
        Ok(m)
    }

    fn animation(&mut self) -> Result<UiAnimation, UiLayoutError> {
        let mut a = UiAnimation { name: self.c.ascii()?, flags: [self.c.u8()?, self.c.u8()?], ..Default::default() };
        for _ in 0..self.count(24)? {
            let mut f = UiAnimFrame::default();
            for x in &mut f.rect {
                *x = self.c.i32()?;
            }
            for x in &mut f.colour {
                *x = self.c.u8()?;
            }
            if self.v > 35 {
                for x in &mut f.extra {
                    *x = self.c.u32()?;
                }
            }
            f.a = self.c.u32()?;
            f.b = self.c.u32()?;
            if self.v > 37 {
                for _ in 0..self.count(8)? {
                    f.events.push((self.c.u32()?, self.c.ascii()?, self.c.ascii()?));
                }
            }
            a.frames.push(f);
        }
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal v033 file: root with one image, one state, one event and one child.
    fn sample() -> Vec<u8> {
        fn s(b: &mut Vec<u8>, t: &str) {
            b.extend_from_slice(&(t.len() as u16).to_le_bytes());
            b.extend_from_slice(t.as_bytes());
        }
        fn u(b: &mut Vec<u8>, x: u32) {
            b.extend_from_slice(&x.to_le_bytes());
        }
        fn comp(b: &mut Vec<u8>, id: &str, with_child: bool) {
            u(b, 7);
            s(b, id);
            u(b, 10);
            u(b, (-5i32) as u32);
            b.extend_from_slice(&[1, 1, 0, 1, 0, 0, 0]); // resize h/v, moveable, visible, da, e5, render_when_dragged
            s(b, ""); // template
            u(b, u32::MAX);
            b.extend_from_slice(&[0, 0, 0, 0]); // two empty utf16
            u(b, 5); // docking
            b.push(0); // render_last_on_focused
            s(b, ""); // script
            u(b, 1); // images
            u(b, 99);
            s(b, "ui\\a.tga");
            u(b, 64);
            u(b, 32);
            u(b, 0xFFFF_FFFF);
            u(b, 0); // mask
            u(b, 0); // unknown_140
            u(b, 1); // states
            u(b, 8);
            s(b, "NewState");
            u(b, 64);
            u(b, 32);
            b.extend_from_slice(&[0, 0, 0, 0]);
            for x in [2, 0, 0, 1, 1] {
                u(b, x);
            }
            b.extend_from_slice(&[1, 0, 0, 0, 0]); // localised + 2 utf16
            s(b, "Ingame 12, Normal");
            u(b, 2);
            u(b, 1);
            u(b, 0xFF00_0000);
            u(b, 0);
            u(b, 0);
            u(b, 0);
            b.extend_from_slice(&[1, 0, 0]);
            s(b, "normal_t0");
            for _ in 0..4 {
                u(b, 0);
            }
            s(b, "");
            s(b, "");
            u(b, 1); // metrics
            u(b, 99);
            for x in [0, 0, 64, 32, 0xFFFF_FFFF] {
                u(b, x);
            }
            b.extend_from_slice(&[0, 0, 0]);
            u(b, 0);
            b.extend_from_slice(&[1, 1]);
            for _ in 0..3 {
                u(b, 0);
            }
            u(b, 0);
            u(b, 0); // text offset
            u(b, 0); // transitions
            u(b, 0); // properties
            s(b, "OnMouseLClickUp");
            s(b, "OnClick");
            s(b, "events_end");
            u(b, 3); // priority
            u(b, u32::from(with_child));
            if with_child {
                comp(b, "child", false);
            }
            s(b, ""); // script override
        }
        let mut b = b"Version033".to_vec();
        comp(&mut b, "root", true);
        b
    }

    #[test]
    fn parses_synthetic_v33() {
        let l = UiLayout::read(&sample()).unwrap();
        assert_eq!(l.version, 33);
        assert_eq!(l.root.id, "root");
        assert_eq!(l.root.offset, (10, -5));
        assert!(l.root.visible && !l.root.moveable);
        assert_eq!(l.root.docking, 5);
        assert_eq!(l.root.priority, 3);
        assert_eq!(l.root.event("OnMouseLClickUp"), Some("OnClick"));
        let st = l.root.initial_state().unwrap();
        assert_eq!(st.font, "Ingame 12, Normal");
        assert_eq!(st.image_metrics[0].width, 64);
        assert_eq!(l.root.image(99).unwrap().path, "ui\\a.tga");
        assert_eq!(l.root.find("child").unwrap().count(), 1);
        assert_eq!(l.root.count(), 2);
    }

    #[test]
    fn rejects_trailing_bytes_and_bad_header() {
        let mut b = sample();
        b.push(0);
        assert!(matches!(UiLayout::read(&b), Err(UiLayoutError::TrailingBytes { .. })));
        assert_eq!(UiLayout::read(b"Hello"), Err(UiLayoutError::BadHeader));
        let b = sample();
        assert!(UiLayout::read(&b[..b.len() - 3]).is_err());
    }

    #[test]
    fn text_placement_follows_the_exe_rules() {
        let mut s = UiState { text_x_offset: 6, text_y_offset: 2, text_align: (ALIGN_LEFT, ALIGN_TOP), ..Default::default() };
        // left / top: one inset each
        assert_eq!(s.text_area(100.0, 40.0), (94.0, 38.0));
        assert_eq!(s.text_line_x(100.0, 30.0), 6.0);
        assert_eq!(s.text_block_y(40.0, 10.0), 2.0);
        // right / bottom: measured from the far edge, one inset
        s.text_align = (ALIGN_RIGHT, ALIGN_BOTTOM);
        assert_eq!(s.text_area(100.0, 40.0), (94.0, 38.0));
        assert_eq!(s.text_line_x(100.0, 30.0), 64.0);
        assert_eq!(s.text_block_y(40.0, 10.0), 28.0);
        // centre: insets on both sides of the area, none in the placement
        s.text_align = (ALIGN_CENTRE, ALIGN_CENTRE);
        assert_eq!(s.text_area(100.0, 40.0), (88.0, 36.0));
        assert_eq!(s.text_line_x(100.0, 30.0), 35.0);
        assert_eq!(s.text_block_y(40.0, 10.0), 15.0);
        // values outside the table: left / top
        s.text_align = (0, 2);
        assert_eq!(s.text_line_x(100.0, 30.0), 6.0);
        assert_eq!(s.text_block_y(40.0, 10.0), 2.0);
        // wrapping
        s.text_behaviour = (SPLIT_BY_WORD, 1);
        assert!(s.text_wraps());
        s.text_behaviour = (NEVER_SPLIT, 1);
        assert!(!s.text_wraps());
    }
}

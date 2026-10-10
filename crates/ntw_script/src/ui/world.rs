//! The live UI component tree (an arena of nodes), shared by the UI script host and the renderer.
//!
//! The original keeps the same data on its `UIComponent` objects (component.cpp); scripts change
//! it through `SetVisible`, `SetState`, `MoveTo`, `Adopt`, ... Pure Rust, no Lua, no Bevy.

use ntw_formats::ui_layout::{UiComponent, UiState};

/// Index of a node in the [`UiWorld`]. Scripts see it as the component's "address".
pub type NodeId = usize;

/// Prefix of the image paths that name a [`RuntimeImage`] instead of a file.
pub const RUNTIME_IMAGE_PREFIX: &str = "__runtime/";

/// An image a script built at run time: the original's `UIPaletisedImage` (an 8-bit palette image,
/// e.g. a campaign map's `<map>_lookup.tga`, whose palette entries the script recolours one by one
/// with `SetPaletteEntry`; CONFIRMED calls in template.map_image.lua). Components show it through
/// `SetComponentTexture`, which stores [`RUNTIME_IMAGE_PREFIX`] + its key as the image path.
#[derive(Debug, Clone, Default)]
pub struct RuntimeImage {
    pub width: u32,
    pub height: u32,
    /// One palette index per pixel, rows top-down.
    pub indices: Vec<u8>,
    /// 256 RGBA entries.
    pub palette: Vec<[u8; 4]>,
    /// Bumped on every change, so a renderer knows when to rebuild its texture.
    pub version: u64,
    /// RGBA8 pixels of a true-colour image (rows top-down; e.g. a save header's region-ownership
    /// map, `GetExtendedSaveGameInfo().Maps`). Used instead of `indices` / `palette` when not empty.
    pub direct: Vec<u8>,
}

impl RuntimeImage {
    /// A true-colour image from RGBA8 pixels (rows top-down).
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> RuntimeImage {
        RuntimeImage { width, height, direct: rgba, ..Default::default() }
    }

    /// The pixels as RGBA8, rows top-down.
    pub fn rgba(&self) -> Vec<u8> {
        if !self.direct.is_empty() {
            return self.direct.clone();
        }
        let mut out = Vec::with_capacity(self.indices.len() * 4);
        for &i in &self.indices {
            out.extend_from_slice(&self.palette.get(i as usize).copied().unwrap_or([0; 4]));
        }
        out
    }

    /// The palette index at pixel (x, y) (origin top-left), if inside.
    pub fn query(&self, x: i64, y: i64) -> Option<u8> {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return None;
        }
        self.indices.get(y as usize * self.width as usize + x as usize).copied()
    }
}

/// An axis-aligned rectangle in UI pixels (origin top-left of the window, y down).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl UiRect {
    /// True if the point is inside.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }

    /// The overlap of two rectangles (zero-sized when they do not overlap).
    pub fn intersect(&self, o: &UiRect) -> UiRect {
        let (x0, y0) = (self.x.max(o.x), self.y.max(o.y));
        let (x1, y1) = ((self.x + self.w).min(o.x + o.w), (self.y + self.h).min(o.y + o.h));
        UiRect { x: x0, y: y0, w: (x1 - x0).max(0.0), h: (y1 - y0).max(0.0) }
    }
}

/// Pointer events that drive a state's transition map (`UiTransition::key`).
/// INFERRED from the shipped buttons' maps (normal→roll on 0, roll→normal on 1, *→down on 2,
/// down→roll on 3, down→mouse_off on 1, mouse_off→normal on 11, mouse_off→down on 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvent {
    /// The mouse moved onto the component (key 0).
    Enter = 0,
    /// The mouse left the component (key 1).
    Leave = 1,
    /// Left button pressed on it (key 2).
    LeftDown = 2,
    /// Left button released on it (key 3).
    LeftUp = 3,
    /// Left button released somewhere else after being pressed on it (key 11).
    LeftUpElsewhere = 11,
}

/// One live component.
#[derive(Debug, Clone)]
pub struct UiNode {
    /// The component as stored in the layout file (its `children` list is left empty; the live
    /// children are [`UiNode::children`]).
    pub data: UiComponent,
    /// Index into `data.states`.
    pub state: usize,
    /// Starts as the file's `visible` flag.
    pub visible: bool,
    /// Position relative to the parent's top-left (starts as the file offset; `MoveTo` changes it).
    pub offset: (f32, f32),
    /// Size override from `Resize` (None = the current state's size).
    pub size_override: Option<(f32, f32)>,
    /// Text set at run time (`SetStateText`), replaces the state's text.
    pub text_override: Option<String>,
    /// Where it is on screen (filled by [`UiWorld::layout`]).
    pub rect: UiRect,
    /// Parent node.
    pub parent: Option<NodeId>,
    /// Children in draw order.
    pub children: Vec<NodeId>,
    /// The layout file it came from, e.g. `ui/frontend ui/main` (used to find its scripts).
    pub layout_file: String,
    /// A script set an image colour (`SetImageColour`): images without a texture path are then
    /// drawn as solid colour (root.lua paints the empty `layout` backdrop black this way).
    /// INFERRED: untextured images are invisible in every shipped layout until a script paints them.
    pub painted: bool,
    /// Tooltip text set by a script (`SetTooltipText`); `None` = the layout's own tooltip.
    pub tooltip: Option<String>,
    /// Created by a script at a position it gave (CreateFromLayout with x, y; templates): keeps
    /// its own size even directly under the top root (not sized like a page).
    pub keep_size: bool,
    /// A script resized it (`Resize`): its images follow the new size (see the renderer).
    pub resized: bool,
    /// `SetInteractive(false)` makes the component ignore the mouse (CONFIRMED name; effect INFERRED).
    pub interactive: bool,
    /// Mouse events a script bound at run time with `SetEventCallback` (so hit-testing knows the
    /// component reacts, like one with the event bound in its layout).
    pub script_events: Vec<String>,
    /// How far the screen puts it from where the HUD scripts see it (see
    /// [`UiWorld::script_rect`]): its top-level ancestor's dock shift, set by [`UiWorld::layout`].
    pub screen_shift: (f32, f32),
}

impl UiNode {
    /// The current state.
    pub fn current(&self) -> Option<&UiState> {
        self.data.states.get(self.state)
    }

    /// The name of the current state.
    pub fn state_name(&self) -> &str {
        self.current().map(|s| s.name.as_str()).unwrap_or("")
    }

    /// Size in pixels.
    pub fn size(&self) -> (f32, f32) {
        self.size_override
            .unwrap_or_else(|| self.current().map(|s| (s.width as f32, s.height as f32)).unwrap_or((0.0, 0.0)))
    }

    /// Size in the state the file starts in (what the editor laid the children out against).
    fn design_size(&self) -> (f32, f32) {
        self.data.initial_state().map(|s| (s.width as f32, s.height as f32)).unwrap_or((0.0, 0.0))
    }

    /// What [`UiWorld::layout`] reads from this node (the debug check of
    /// [`UiWorld::update_appearance`]).
    #[cfg(debug_assertions)]
    fn layout_inputs(&self) -> LayoutInputs {
        // The children list as an order-sensitive hash (no copy per check): a child swapped for
        // another, added, removed or moved in place changes it (draw order changes go through
        // [`UiWorld::reorder`], not an appearance change).
        let children = self.children.iter().fold(self.children.len() as u64, |h, &c| h.wrapping_mul(0x0000_0100_0000_01B3).wrapping_add(c as u64 + 1));
        (self.offset, self.size(), self.design_size(), self.resized, self.data.docking, self.keep_size, self.parent, children)
    }
}

/// A node's layout inputs: offset, size, design size, resized, docking, keep size, parent, and the
/// children list (an order-sensitive hash of it).
#[cfg(debug_assertions)]
type LayoutInputs = ((f32, f32), (f32, f32), (f32, f32), bool, u32, bool, Option<NodeId>, u64);

/// Sets `*slot` to `value` and returns whether it differed (the change report of
/// [`UiWorld::update_appearance`]).
pub fn set_changed<T: PartialEq>(slot: &mut T, value: T) -> bool {
    let changed = *slot != value;
    if changed {
        *slot = value;
    }
    changed
}

/// All live components.
#[derive(Debug, Default)]
pub struct UiWorld {
    nodes: Vec<Option<UiNode>>,
    /// Bumped on every change, so a renderer knows when to redraw.
    pub generation: u64,
    /// Something the layout depends on changed (an offset, a size, docking, a state, the tree's
    /// shape) since the host last laid the tree out; [`update_appearance`](Self::update_appearance)
    /// changes leave it alone. The host clears it after a layout (`host.rs` `lay_out_if_stale`).
    pub layout_dirty: bool,
    /// Images built by scripts at run time, by key (see [`RuntimeImage`]).
    pub runtime_images: std::collections::HashMap<String, RuntimeImage>,
}

impl UiWorld {
    /// An empty world.
    pub fn new() -> Self {
        Self::default()
    }

    /// A node, if it exists.
    pub fn get(&self, id: NodeId) -> Option<&UiNode> {
        self.nodes.get(id).and_then(Option::as_ref)
    }

    /// Every live node id, in creation order.
    pub fn ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.iter().enumerate().filter(|(_, n)| n.is_some()).map(|(i, _)| i)
    }

    /// A node, mutably, for a change that may move or resize something (counts as a change and
    /// marks the layout dirty, see [`UiWorld::layout_dirty`]).
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut UiNode> {
        self.generation += 1;
        self.layout_dirty = true;
        self.nodes.get_mut(id).and_then(Option::as_mut)
    }

    /// Applies `change` to a node, a change that moves and resizes nothing (texts, colours,
    /// images, visibility, tooltips, input flags) and returns whether it changed anything: a
    /// redraw only then ([`UiWorld::generation`]), never a new layout. The layout reads only
    /// offsets, sizes (state sizes, overrides), docking and the tree's shape; debug builds check
    /// that `change` left those alone. False when the node does not exist.
    pub fn update_appearance(&mut self, id: NodeId, change: impl FnOnce(&mut UiNode) -> bool) -> bool {
        let Some(n) = self.nodes.get_mut(id).and_then(Option::as_mut) else { return false };
        #[cfg(debug_assertions)]
        let before = n.layout_inputs();
        let changed = change(n);
        #[cfg(debug_assertions)]
        debug_assert!(before == n.layout_inputs(), "an appearance change moved or resized component {}", n.data.id);
        if changed {
            self.generation += 1;
        }
        changed
    }

    /// Creates live nodes for `comp` and all its children; returns the new subtree's root.
    /// The new nodes are listed in `created` in pre-order (parents before children).
    pub fn instantiate(&mut self, comp: &UiComponent, parent: Option<NodeId>, layout_file: &str, created: &mut Vec<NodeId>) -> NodeId {
        let state = comp
            .initial_state()
            .and_then(|s| comp.states.iter().position(|x| x.this == s.this))
            .unwrap_or(0);
        let priority = self.inherited_priority(parent, comp.priority);
        let id = self.nodes.len();
        self.nodes.push(Some(UiNode {
            data: UiComponent { children: Vec::new(), priority, ..comp.clone() },
            state,
            visible: comp.visible,
            offset: (comp.offset.0 as f32, comp.offset.1 as f32),
            size_override: None,
            text_override: None,
            rect: UiRect::default(),
            parent,
            children: Vec::new(),
            layout_file: layout_file.to_owned(),
            painted: false,
            tooltip: None,
            keep_size: false,
            resized: false,
            interactive: true,
            script_events: Vec::new(),
            screen_shift: (0.0, 0.0),
        }));
        created.push(id);
        if let Some(p) = parent.and_then(|p| self.nodes[p].as_mut()) {
            p.children.push(id);
        }
        for c in &comp.children {
            self.instantiate(c, Some(id), layout_file, created);
        }
        self.generation += 1;
        self.layout_dirty = true;
        id
    }

    /// Copies the live subtree under `src` (its current states, texts, visibility and offsets,
    /// but not its script globals) as a new subtree under `parent`. Used by
    /// `Component.CreateFromComponent` (list rows are copies of a hidden example row).
    /// The new nodes are listed in `created` in pre-order.
    pub fn clone_subtree(&mut self, src: NodeId, parent: Option<NodeId>, created: &mut Vec<NodeId>) -> Option<NodeId> {
        let mut node = self.get(src)?.clone();
        node.data.priority = self.inherited_priority(parent, node.data.priority);
        let id = self.nodes.len();
        let kids = node.children.clone();
        self.nodes.push(Some(UiNode {
            parent,
            children: Vec::new(),
            rect: UiRect::default(),
            script_events: Vec::new(),
            screen_shift: (0.0, 0.0),
            ..node
        }));
        created.push(id);
        if let Some(p) = parent.and_then(|p| self.nodes.get_mut(p)).and_then(Option::as_mut) {
            p.children.push(id);
        }
        for c in kids {
            self.clone_subtree(c, Some(id), created);
        }
        self.generation += 1;
        self.layout_dirty = true;
        Some(id)
    }

    /// The priority a component made under `parent` takes: its own, or the parent's when that is
    /// higher and the parent has a parent of its own (CONFIRMED: the constructor `0x0101E270` at
    /// `0x0101F1A6` and the copy constructor `0x0101FC20` at `0x0101FDBC`). So the root's own
    /// children keep their layout value and everything below them is at least at their level.
    pub fn inherited_priority(&self, parent: Option<NodeId>, own: i32) -> i32 {
        match parent.and_then(|p| self.get(p)) {
            Some(p) if p.parent.is_some() => own.max(p.data.priority),
            _ => own,
        }
    }

    /// `ReorderChildren(list)` (CONFIRMED `0x01014850` → `0x01031ED0`): the new child order is
    /// `list` followed by the children it leaves out, in their current order. It is applied only
    /// when that comes to exactly the parent's child count, so a list with a duplicate or a
    /// component that is not a child changes nothing. Returns whether it was applied. Children
    /// draw in this order (`0x01027D20` walks them first to last). A list that is already the
    /// first children (the battle HUD's every frame) changes nothing: no allocation, and
    /// [`generation`](Self::generation) stays, so nothing is redrawn for it.
    pub fn reorder_children(&mut self, parent: NodeId, list: &[NodeId]) -> bool {
        let Some(p) = self.get(parent) else { return false };
        if p.children.starts_with(list) {
            return true;
        }
        let mut order = list.to_vec();
        order.extend(p.children.iter().filter(|c| !list.contains(c)));
        if order.len() != p.children.len() {
            return false;
        }
        if let Some(pn) = self.nodes[parent].as_mut() {
            pn.children = order;
        }
        self.generation += 1;
        true
    }

    /// Depth-first search for a descendant (or `from` itself) with this component id.
    pub fn find(&self, from: NodeId, id: &str) -> Option<NodeId> {
        let n = self.get(from)?;
        if n.data.id == id {
            return Some(from);
        }
        n.children.iter().find_map(|&c| self.find(c, id))
    }

    /// The top of the tree containing `id`.
    pub fn root_of(&self, mut id: NodeId) -> NodeId {
        while let Some(p) = self.get(id).and_then(|n| n.parent) {
            id = p;
        }
        id
    }

    /// Moves `child` under `parent`, last in draw order (CONFIRMED: `Adopt` → `0x01024F70` with
    /// index -1, which appends).
    pub fn adopt(&mut self, parent: NodeId, child: NodeId) {
        if self.get(parent).is_none() || self.get(child).is_none() || parent == child {
            return;
        }
        self.divorce_from_parent(child);
        if let Some(p) = self.nodes[parent].as_mut() {
            p.children.push(child);
        }
        if let Some(c) = self.nodes[child].as_mut() {
            c.parent = Some(parent);
        }
        self.generation += 1;
        self.layout_dirty = true;
    }

    /// Detaches `child` from its parent (it stays alive, without a parent).
    pub fn divorce_from_parent(&mut self, child: NodeId) {
        if let Some(p) = self.get(child).and_then(|c| c.parent)
            && let Some(pn) = self.nodes[p].as_mut()
        {
            pn.children.retain(|&c| c != child);
        }
        if let Some(c) = self.nodes.get_mut(child).and_then(Option::as_mut) {
            c.parent = None;
        }
        self.generation += 1;
        self.layout_dirty = true;
    }

    /// Moves the rectangles of `id` and its subtree by (dx, dy): a subtree outside the laid-out
    /// tree (no [`layout`](Self::layout) reaches it) follows a MoveTo this way.
    pub fn translate_subtree(&mut self, id: NodeId, dx: f32, dy: f32) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            if let Some(node) = self.nodes.get_mut(n).and_then(Option::as_mut) {
                node.rect.x += dx;
                node.rect.y += dy;
                stack.extend_from_slice(&node.children);
            }
        }
        self.generation += 1;
    }

    /// Destroys a node and its subtree. Returns the destroyed ids.
    pub fn destroy(&mut self, id: NodeId) -> Vec<NodeId> {
        self.divorce_from_parent(id);
        let mut gone = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            if let Some(node) = self.nodes.get_mut(n).and_then(Option::take) {
                stack.extend(node.children);
                gone.push(n);
            }
        }
        self.generation += 1;
        self.layout_dirty = true;
        gone
    }

    /// Switches a node to the named state. Returns true if it changed.
    pub fn set_state(&mut self, id: NodeId, name: &str) -> bool {
        let Some(n) = self.nodes.get_mut(id).and_then(Option::as_mut) else { return false };
        match n.data.states.iter().position(|s| s.name == name) {
            Some(i) if i != n.state => {
                n.state = i;
                self.generation += 1;
                self.layout_dirty = true;
                true
            }
            _ => false,
        }
    }

    /// Applies the current state's transition map for a pointer event (CONFIRMED data, INFERRED
    /// event numbering, see [`PointerEvent`]). Returns the state it left (its index) if the state
    /// changed.
    pub fn transition(&mut self, id: NodeId, event: PointerEvent) -> Option<usize> {
        let n = self.nodes.get_mut(id)?.as_mut()?;
        let state = n.data.states.get(n.state)?;
        let t = state.transitions.iter().find(|t| t.key == event as u32)?;
        match n.data.states.iter().position(|s| s.this == t.value) {
            Some(i) if i != n.state => {
                let old = std::mem::replace(&mut n.state, i);
                self.generation += 1;
                self.layout_dirty = true;
                Some(old)
            }
            _ => None,
        }
    }

    /// Computes every node's on-screen rectangle under `root`, which is placed at `rect`.
    ///
    /// A child's offset is its top-left inside the parent as the editor saved it. A docked child
    /// (`docking` 1..9 = top-left, top-centre, top-right, centre-left, centre, centre-right,
    /// bottom-left, bottom-centre, bottom-right) keeps its distance to that anchor of the parent
    /// when the parent's size differs from its design size. INFERRED from the data (e.g.
    /// `bottom_bar` dock 8 at y = 1200 - 122); the engine's exact rule is PROVISIONAL.
    pub fn layout(&mut self, root: NodeId, rect: UiRect) {
        let mut changed = false;
        if let Some(n) = self.nodes.get_mut(root).and_then(Option::as_mut) {
            changed |= n.rect != rect;
            n.rect = rect;
            n.screen_shift = (0.0, 0.0);
        }
        self.layout_children(root, None, &mut changed);
        // Only a real move counts as a change, so a renderer is not asked to redraw every frame.
        if changed {
            self.generation += 1;
        }
    }

    /// `shift` is the screen shift of `id`'s subtree, `None` when `id` is the root (each of its
    /// children then gets its own dock shift, see [`UiNode::screen_shift`]).
    fn layout_children(&mut self, id: NodeId, shift: Option<(f32, f32)>, changed: &mut bool) {
        let Some(n) = self.get(id) else { return };
        let (pw0, ph0) = n.design_size();
        let parent = n.rect;
        let children = n.children.clone();
        for c in children {
            let Some(cn) = self.nodes.get_mut(c).and_then(Option::as_mut) else { continue };
            let (w, h) = cn.size();
            // A child a script resized keeps its own dock point's distance to the parent's: a
            // resized edge docked at the top centre stays centred (INFERRED from the Tooltip
            // template, whose edges `t` / `b` (dock 2 / 8) and `l` / `r` (dock 4 / 6) are resized
            // with it; the rule for state size changes is left as it was, PROVISIONAL).
            let (cw0, ch0) = if cn.resized { cn.design_size() } else { (w, h) };
            let (fx, fy) = dock_fractions(cn.data.docking);
            let grow = (fx * (parent.w - pw0), fy * (parent.h - ph0));
            let x = parent.x + cn.offset.0 + grow.0 - fx * (w - cw0);
            let y = parent.y + cn.offset.1 + grow.1 - fy * (h - ch0);
            let r = UiRect { x, y, w, h };
            *changed |= cn.rect != r;
            cn.rect = r;
            // The screen shift a top-level component gets is kept for its whole subtree, and a
            // resized panel keeps the shift it was moved with (PROVISIONAL: untraced in the exe).
            let s = shift.unwrap_or(grow);
            cn.screen_shift = s;
            self.layout_children(c, Some(s), changed);
        }
    }

    /// The top-most visible component under a point that reacts to the mouse (it has mouse
    /// transitions or binds a mouse event). Later children are on top.
    pub fn hit(&self, id: NodeId, px: f32, py: f32) -> Option<NodeId> {
        let n = self.get(id)?;
        if !n.visible {
            return None;
        }
        // Children of a clipping component can only be hit inside it (INFERRED from the draw clip).
        let inside = !n.data.clips_children() || n.rect.contains(px, py);
        for &c in n.children.iter().rev().filter(|_| inside) {
            if let Some(h) = self.hit(c, px, py) {
                return Some(h);
            }
        }
        let reacts = n.data.events.iter().any(|(e, _)| e.starts_with("OnMouseL"))
            || n.script_events.iter().any(|e| e.starts_with("OnMouseL"))
            || n.current().is_some_and(|s| !s.transitions.is_empty());
        (n.interactive && reacts && n.rect.contains(px, py)).then_some(id)
    }

    /// The scripts' frame (CONFIRMED in the original, debugger sitting 2026-10-07 at 1920x1080,
    /// campaign HUD): the root keeps its layout size (1280x960) for the scripts, and each component
    /// sits where the scripts put it in that frame; on screen, a top-level component (a child of
    /// the root) is moved by its dock anchor's share of the screen's growth (the Lists panel,
    /// docked right-centre, read at (648, -1) and shown at (1288, 59); the HUD band, docked
    /// bottom-centre, read at (-1, 718)). Our layout already places top-level components that
    /// way (`layout_children`, which records each node's [`UiNode::screen_shift`]); this is the
    /// offset from the scripts' frame to the screen for `id` as of the last layout ((0, 0) for the
    /// root and when nothing grew; a divorced subtree keeps the shift it was laid out with, as it
    /// keeps its rectangles).
    pub fn script_to_screen(&self, id: NodeId) -> (f32, f32) {
        self.get(id).map_or((0.0, 0.0), |n| n.screen_shift)
    }

    /// A node's rectangle in the scripts' frame ([`Self::script_to_screen`]); the top root's
    /// (`root`) is its layout size.
    pub fn script_rect(&self, root: Option<NodeId>, id: NodeId) -> Option<UiRect> {
        let n = self.get(id)?;
        if Some(id) == root {
            let (w, h) = n.design_size();
            return Some(UiRect { x: 0.0, y: 0.0, w, h });
        }
        let (dx, dy) = n.screen_shift;
        Some(UiRect { x: n.rect.x - dx, y: n.rect.y - dy, ..n.rect })
    }

    /// The rectangle a node is drawn inside: the intersection of the rectangles of its ancestors
    /// that clip their children (`UiComponent::clips_children`, the layouts' ClipChildren flag;
    /// CONFIRMED in the exe's component draw 0x01027D20). None if no ancestor clips.
    pub fn clip_rect(&self, id: NodeId) -> Option<UiRect> {
        let mut out: Option<UiRect> = None;
        let mut p = self.get(id).and_then(|n| n.parent);
        while let Some(pid) = p {
            let Some(n) = self.get(pid) else { break };
            if n.data.clips_children() {
                out = Some(match out {
                    Some(c) => c.intersect(&n.rect),
                    None => n.rect,
                });
            }
            p = n.parent;
        }
        out
    }
    /// Visits `id` and its visible descendants in draw order.
    pub fn visit_visible(&self, id: NodeId, f: &mut impl FnMut(NodeId, &UiNode)) {
        let Some(n) = self.get(id) else { return };
        if !n.visible {
            return;
        }
        f(id, n);
        for &c in &n.children {
            self.visit_visible(c, f);
        }
    }
}

/// Anchor fractions (x, y) of a dock point. 0 = not docked.
fn dock_fractions(dock: u32) -> (f32, f32) {
    match dock {
        1..=9 => {
            let i = dock - 1;
            ((i % 3) as f32 * 0.5, (i / 3) as f32 * 0.5)
        }
        _ => (0.0, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::ui_layout::UiTransition;

    fn comp(id: &str, w: i32, h: i32, offset: (i32, i32), docking: u32) -> UiComponent {
        UiComponent {
            id: id.into(),
            offset,
            docking,
            visible: true,
            states: vec![UiState { this: 1, name: "NewState".into(), width: w, height: h, ..Default::default() }],
            ..Default::default()
        }
    }

    #[test]
    fn docked_child_follows_parent_resize() {
        let mut root = comp("root", 1280, 960, (0, 0), 0);
        root.children.push(comp("centre", 100, 100, (590, 430), 5));
        root.children.push(comp("bottom", 1280, 122, (0, 838), 8));
        root.children.push(comp("free", 10, 10, (5, 5), 0));
        let mut w = UiWorld::new();
        let r = w.instantiate(&root, None, "x", &mut Vec::new());
        w.layout(r, UiRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
        let rect = |id: &str| w.get(w.find(r, id).unwrap()).unwrap().rect;
        assert_eq!((rect("centre").x, rect("centre").y), (910.0, 490.0));
        assert_eq!(rect("bottom").y, 958.0);
        assert_eq!((rect("free").x, rect("free").y), (5.0, 5.0));
    }

    #[test]
    fn adopt_destroy_and_hit() {
        let mut root = comp("root", 100, 100, (0, 0), 0);
        let mut b = comp("button", 50, 20, (10, 10), 0);
        b.events.push(("OnMouseLClickUp".into(), "OnLeftClickUp".into()));
        root.children.push(b);
        let mut w = UiWorld::new();
        let mut created = Vec::new();
        let r = w.instantiate(&root, None, "x", &mut created);
        assert_eq!(created.len(), 2);
        w.layout(r, UiRect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 });
        let b = w.find(r, "button").unwrap();
        assert_eq!(w.hit(r, 15.0, 15.0), Some(b));
        assert_eq!(w.hit(r, 90.0, 90.0), None);
        let other = w.instantiate(&comp("page", 10, 10, (0, 0), 0), None, "y", &mut Vec::new());
        w.adopt(r, other);
        assert_eq!(w.get(r).unwrap().children, vec![b, other]);
        assert_eq!(w.destroy(other), vec![other]);
        assert_eq!(w.get(r).unwrap().children, vec![b]);
        assert_eq!(w.root_of(b), r);
    }

    /// A reorder to the order already there asks for no redraw (polish: the battle HUD reorders
    /// every frame, and each call bumped the redraw counter); a real change does.
    #[test]
    fn an_unchanged_reorder_asks_for_no_redraw() {
        let mut w = UiWorld::new();
        let r = w.instantiate(&comp("root", 100, 100, (0, 0), 0), None, "x", &mut Vec::new());
        let a = w.instantiate(&comp("a", 10, 10, (0, 0), 0), None, "y", &mut Vec::new());
        let b = w.instantiate(&comp("b", 10, 10, (0, 0), 0), None, "y", &mut Vec::new());
        w.adopt(r, a);
        w.adopt(r, b);
        let g = w.generation;
        assert!(w.reorder_children(r, &[a]));
        assert!(w.reorder_children(r, &[a, b]));
        assert_eq!(w.generation, g, "already in this order");
        assert!(w.reorder_children(r, &[b]));
        assert!(w.generation > g);
        assert_eq!(w.get(r).unwrap().children, vec![b, a]);
        assert!(!w.reorder_children(r, &[b, b]), "a duplicate changes nothing");
    }

    /// A component takes its parent's priority when that is higher, except under the root
    /// (`0x0101E270` / `0x0101FC20`): a -1 label under the root stays -1, a child of a 47 panel
    /// rises to 47, a higher one keeps its own; a copy follows the parent it is copied under.
    #[test]
    fn priority_is_inherited_below_the_roots_children() {
        let mut root = comp("root", 100, 100, (0, 0), 0);
        let mut panel = comp("panel", 50, 50, (0, 0), 0);
        panel.priority = 47;
        panel.children.push(comp("low", 10, 10, (0, 0), 0));
        let mut high = comp("high", 10, 10, (0, 0), 0);
        high.priority = 60;
        panel.children.push(high);
        root.children.push(panel);
        let mut label = comp("label", 10, 10, (0, 0), 0);
        label.priority = -1;
        label.children.push(comp("name", 10, 10, (0, 0), 0));
        root.children.push(label);
        let mut w = UiWorld::new();
        let r = w.instantiate(&root, None, "x", &mut Vec::new());
        let prio = |w: &UiWorld, id: &str| w.get(w.find(r, id).unwrap()).unwrap().data.priority;
        assert_eq!((prio(&w, "panel"), prio(&w, "low"), prio(&w, "high")), (47, 47, 60));
        assert_eq!((prio(&w, "label"), prio(&w, "name")), (-1, 0), "the root's child keeps -1; its child keeps its own 0");
        let copy = w.clone_subtree(w.find(r, "label").unwrap(), w.find(r, "panel"), &mut Vec::new()).unwrap();
        assert_eq!(w.get(copy).unwrap().data.priority, 47, "copied under the panel");
    }

    /// The scripts' frame (`script_rect`): the root at its layout size, a top-level component and
    /// its children moved back by the dock shift the layout recorded; a divorced component is not
    /// taken for the root (review 2026-10-07: it reported the layout size at (0, 0)).
    #[test]
    fn the_scripts_frame_undoes_the_dock_shift() {
        let mut root = comp("root", 1280, 960, (0, 0), 0);
        let mut panel = comp("panel", 624, 720, (648, -1), 6);
        panel.children.push(comp("inner", 10, 10, (5, 5), 0));
        root.children.push(panel);
        let mut w = UiWorld::new();
        let r = w.instantiate(&root, None, "x", &mut Vec::new());
        w.layout(r, UiRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
        let (p, i) = (w.find(r, "panel").unwrap(), w.find(r, "inner").unwrap());
        let at = |w: &UiWorld, id| w.script_rect(Some(r), id).map(|q| (q.x, q.y, q.w, q.h)).unwrap();
        assert_eq!(at(&w, r), (0.0, 0.0, 1280.0, 960.0));
        assert_eq!((w.get(p).unwrap().rect.x, w.get(p).unwrap().rect.y), (1288.0, 59.0));
        assert_eq!(at(&w, p), (648.0, -1.0, 624.0, 720.0));
        assert_eq!(at(&w, i), (653.0, 4.0, 10.0, 10.0));
        w.divorce_from_parent(p);
        assert_eq!(at(&w, p), (648.0, -1.0, 624.0, 720.0), "a divorced component keeps its place");
        assert_eq!(at(&w, i), (653.0, 4.0, 10.0, 10.0));
    }

    #[test]
    fn transitions_follow_the_state_map() {
        let mut c = comp("b", 10, 10, (0, 0), 0);
        c.states = vec![
            UiState { this: 10, name: "normal".into(), transitions: vec![UiTransition { key: 0, value: 11, ..Default::default() }], ..Default::default() },
            UiState { this: 11, name: "roll".into(), transitions: vec![UiTransition { key: 1, value: 10, ..Default::default() }], ..Default::default() },
        ];
        let mut w = UiWorld::new();
        let id = w.instantiate(&c, None, "x", &mut Vec::new());
        assert_eq!(w.transition(id, PointerEvent::Enter), Some(0));
        assert_eq!(w.get(id).unwrap().state_name(), "roll");
        assert_eq!(w.transition(id, PointerEvent::Enter), None);
        assert_eq!(w.transition(id, PointerEvent::Leave), Some(1));
        assert_eq!(w.get(id).unwrap().state_name(), "normal");
    }

    /// The debug check of `update_appearance` sees a change of the children list made in place
    /// (same buffer, same length): here two children swapped.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "an appearance change moved or resized component")]
    fn an_appearance_change_that_reorders_children_in_place_is_caught() {
        let mut root = comp("root", 100, 100, (0, 0), 0);
        root.children = vec![comp("a", 10, 10, (0, 0), 0), comp("b", 10, 10, (0, 0), 0)];
        let mut w = UiWorld::new();
        let r = w.instantiate(&root, None, "x", &mut Vec::new());
        w.update_appearance(r, |n| {
            n.children.swap(0, 1);
            true
        });
    }
}

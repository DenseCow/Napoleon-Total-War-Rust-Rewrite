//! `groupformations.bin` (data.pack): the group formation templates the battle uses to lay out
//! several units at once (default deployment, multiple-selection drag-out, AI groups).
//!
//! Layout CONFIRMED from the exe's loader (`0x0068E2D0` table, `0x0068E890` template, field use
//! in the element constructors `0x0068BF10`/`0x0068BFE0`/`0x0068C050`;
//! `analysis/fidelity/UNITS_TERRAIN_FIDELITY.md` §5.2) and by reading every shipped template to
//! the last byte (install test `ntw_formats/tests/group_formation_install.rs`). Little-endian:
//! ```text
//! u32 template_count
//! template:
//!   u16 n, n × u16 UTF-16 name
//!   f32 priority, u32 purpose bits, u32 × 3 least % of artillery / cavalry / infantry
//!   u32 faction_count, faction_count × (u16 n, UTF-16 faction key)   (empty = any faction)
//!   u32 element_count, element_count × element
//! element: u32 id, u32 kind, then by kind
//!   0 block      f32 priority, u32 arrangement, f32 spacing, u32 (UNKNOWN), f32 x, f32 y,
//!                i32 min units, i32 max units (-1 = any), u32 n, n × (f32 weight, u32 class)
//!   1 relative   f32 priority, u32 anchor element id, u32 arrangement, f32 spacing,
//!                u32 (UNKNOWN), f32 offset x, f32 offset y, i32 min, i32 max,
//!                u32 n, n × (f32 weight, u32 class)
//!   2 spanning   4 × u32 (not used by the shipped land templates)
//!   3 group      u32 n, n × u32 element ids
//! ```
//! The selection, assignment and layout rules the exe applies to these templates are in
//! [`choose`], [`assign`] and [`layout`].

use std::fmt;

/// One template.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupFormation {
    pub name: String,
    /// The five header words after the name, raw (`f32` priority first, INFERRED).
    pub header: [u32; 5],
    /// Faction keys allowed to use the template; empty = any.
    pub factions: Vec<String>,
    pub elements: Vec<FormationElement>,
}

/// One element of a template.
#[derive(Debug, Clone, PartialEq)]
pub struct FormationElement {
    pub id: u32,
    pub body: ElementBody,
}

/// What an element is (by its kind word).
#[derive(Debug, Clone, PartialEq)]
pub enum ElementBody {
    /// Kind 0 and 1: a block of units. `anchor` is only present in kind 1.
    Block {
        kind: u32,
        words: Vec<u32>,
        /// `(weight, unit class)` pairs, in file order.
        classes: Vec<(f32, u32)>,
    },
    /// Kind 2: four words.
    Spanning { words: [u32; 4] },
    /// Kind 3: a list of element ids.
    Group { members: Vec<u32> },
}

/// A reading error with the byte offset.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupFormationError {
    pub offset: usize,
    pub what: String,
}

impl fmt::Display for GroupFormationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "groupformations.bin at {}: {}", self.offset, self.what)
    }
}

impl std::error::Error for GroupFormationError {}

struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}

impl Cur<'_> {
    fn err(&self, what: impl Into<String>) -> GroupFormationError {
        GroupFormationError { offset: self.p, what: what.into() }
    }
    fn take(&mut self, n: usize) -> Result<&[u8], GroupFormationError> {
        if self.b.len() - self.p < n {
            return Err(self.err(format!("need {n} bytes")));
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, GroupFormationError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, GroupFormationError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn count(&mut self, max: u32) -> Result<usize, GroupFormationError> {
        let n = self.u32()?;
        if n > max {
            return Err(self.err(format!("count {n} > {max}")));
        }
        Ok(n as usize)
    }
    fn string(&mut self) -> Result<String, GroupFormationError> {
        let n = self.u16()? as usize;
        let raw = self.take(n * 2)?;
        let units: Vec<u16> = (0..n).map(|i| u16::from_le_bytes([raw[2 * i], raw[2 * i + 1]])).collect();
        Ok(String::from_utf16_lossy(&units))
    }
}

/// Reads the whole file (to the last byte).
pub fn read(bytes: &[u8]) -> Result<Vec<GroupFormation>, GroupFormationError> {
    let mut c = Cur { b: bytes, p: 0 };
    let n = c.count(10_000)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let name = c.string()?;
        let mut header = [0u32; 5];
        for h in &mut header {
            *h = c.u32()?;
        }
        let nf = c.count(1_000)?;
        let factions = (0..nf).map(|_| c.string()).collect::<Result<Vec<_>, _>>()?;
        let ne = c.count(10_000)?;
        let mut elements = Vec::with_capacity(ne);
        for _ in 0..ne {
            let id = c.u32()?;
            let kind = c.u32()?;
            let body = match kind {
                0 | 1 => {
                    let words = (0..if kind == 0 { 8 } else { 9 }).map(|_| c.u32()).collect::<Result<Vec<_>, _>>()?;
                    let nc = c.count(10_000)?;
                    let mut classes = Vec::with_capacity(nc);
                    for _ in 0..nc {
                        let weight = f32::from_bits(c.u32()?);
                        let class = c.u32()?;
                        classes.push((weight, class));
                    }
                    ElementBody::Block { kind, words, classes }
                }
                2 => ElementBody::Spanning { words: [c.u32()?, c.u32()?, c.u32()?, c.u32()?] },
                3 => {
                    let nm = c.count(10_000)?;
                    ElementBody::Group { members: (0..nm).map(|_| c.u32()).collect::<Result<Vec<_>, _>>()? }
                }
                k => return Err(c.err(format!("unknown element kind {k}"))),
            };
            elements.push(FormationElement { id, body });
        }
        out.push(GroupFormation { name, header, factions, elements });
    }
    if c.p != bytes.len() {
        return Err(c.err(format!("{} bytes left", bytes.len() - c.p)));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Typed templates and the default-deployment rules (UNITS_TERRAIN_FIDELITY.md §5.2).

/// The unit classes, by id: the exe maps `unit_stats_land` #3 `class` to these numbers
/// alphabetically (CONFIRMED, `0x00EED3E0`; unknown names → 0). Template class lists use the ids;
/// id 46 (one past the list) appears with tiny weights and matches no unit (INFERRED: a filler).
pub const UNIT_CLASSES: [&str; 46] = [
    "artillery_fixed", "artillery_foot", "artillery_horse", "cavalry_camels", "cavalry_heavy",
    "cavalry_irregular", "cavalry_lancers", "cavalry_light", "cavalry_missile", "cavalry_standard",
    "dragoons", "elephants", "general", "infantry_berserker", "infantry_elite", "infantry_grenadiers",
    "infantry_irregulars", "infantry_light", "infantry_line", "infantry_melee", "infantry_militia",
    "infantry_mob", "infantry_skirmishers", "naval_admiral", "naval_bomb_ketch", "naval_brig",
    "naval_dhow", "naval_fifth_rate", "naval_first_rate", "naval_fourth_rate", "naval_galleon",
    "naval_heavy_galley", "naval_indiaman", "naval_light_galley", "naval_lugger", "naval_medium_galley",
    "naval_over_first_rate", "naval_razee", "naval_rocket_ship", "naval_second_rate", "naval_sixth_rate",
    "naval_sloop", "naval_steam_ship", "naval_third_rate", "naval_xebec", "naval_transport",
];

/// A class name's id (the exe's mapping: unknown names are 0).
pub fn class_id(name: &str) -> u32 {
    UNIT_CLASSES.iter().position(|c| c.eq_ignore_ascii_case(name)).unwrap_or(0) as u32
}

/// Template purpose bits (header word 1). The exe tests `purposes & wanted == wanted`, and a
/// wanted value with bit 0x10 accepts any template (CONFIRMED, `0x006EFD50`). Bit 2 is what the
/// default deployment asks for (CONFIRMED caller `0x005BA640`); the name of bit 1 is INFERRED
/// (only "Multiple Selection Drag Out Land" has it alone; the naval templates have 0x60).
pub const PURPOSE_DRAG_OUT: u32 = 1;
pub const PURPOSE_DEPLOYMENT: u32 = 2;

/// The unit-count bound meaning "no limit".
pub const UNLIMITED: i32 = -1;

/// The three unit roles the composition test counts (CONFIRMED tests `0x0055AB90`,
/// `0x0055ABF0`, `0x0055C1C0`): artillery with guns; cavalry (cavalry, camels, mounted
/// dragoons); infantry (infantry, gunless artillery crews, dismounted dragoons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Artillery,
    Cavalry,
    Infantry,
}

/// Where an element goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Placement {
    /// Kind 0: centred on `pos` (x right, y forward, metres) in the group's frame.
    Block { pos: (f32, f32) },
    /// Kind 1: next to element `anchor`, `offset` metres beyond its edge (see [`layout`]).
    Relative { anchor: u32, offset: (f32, f32) },
    /// Kind 2: four words (unused by the shipped land templates; laid out as empty).
    Spanning { words: [u32; 4] },
    /// Kind 3: the union of its members.
    Group { members: Vec<u32> },
}

/// One template element.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub id: u32,
    pub placement: Placement,
    /// Scales the element's bids for units (blocks and relatives).
    pub priority: f32,
    /// 0 = line, 1 = column, 2/3 = naval curves (CONFIRMED vtable choice `0x006C2F70`).
    pub arrangement: u32,
    /// Gap between neighbouring units in the element (metres).
    pub spacing: f32,
    /// Least / most units (`UNLIMITED` = -1).
    pub min_units: i32,
    pub max_units: i32,
    /// `(class id, weight)`; a unit's weight in the element is the first matching entry's.
    pub classes: Vec<(u32, f32)>,
}

/// One template, typed.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub name: String,
    /// Multiplies the template's assignment score.
    pub priority: f32,
    pub purposes: u32,
    /// Least share (%) of artillery, cavalry and infantry in the units.
    pub min_percent: [u32; 3],
    /// Allowed factions; empty = any.
    pub factions: Vec<String>,
    pub elements: Vec<Element>,
    /// Least / most units (from the elements, see [`Template::from_raw`]).
    pub min_units: u32,
    pub max_units: u32,
}

impl Template {
    /// Types a raw template. The unit bounds follow the exe's loader: `min` sums the elements'
    /// minimums until the first element whose minimum is 0, which resets it to 0; `max` sums
    /// the maximums and becomes unlimited at the first unlimited one (CONFIRMED, `0x0068E890`).
    pub fn from_raw(g: &GroupFormation) -> Self {
        let mut elements = Vec::new();
        let (mut min, mut max, mut counting_min) = (0u32, 0u32, true);
        for e in &g.elements {
            let (placement, priority, arrangement, spacing, lo, hi, classes) = match &e.body {
                ElementBody::Block { kind, words, classes } => {
                    let f = |i: usize| f32::from_bits(words[i]);
                    let w = |i: usize| words[i] as i32;
                    if *kind == 0 {
                        (Placement::Block { pos: (f(4), f(5)) }, f(0), words[1], f(2), w(6), w(7), classes.clone())
                    } else {
                        (Placement::Relative { anchor: words[1], offset: (f(5), f(6)) }, f(0), words[2], f(3), w(7), w(8), classes.clone())
                    }
                }
                ElementBody::Spanning { words } => (Placement::Spanning { words: *words }, 0.0, 0, 0.0, 0, 0, Vec::new()),
                ElementBody::Group { members } => (Placement::Group { members: members.clone() }, 0.0, 0, 0.0, 0, 0, Vec::new()),
            };
            if counting_min {
                if lo == 0 {
                    min = 0;
                    counting_min = false;
                } else {
                    min = min.wrapping_add(lo as u32);
                }
            }
            if max != u32::MAX {
                max = if hi == UNLIMITED { u32::MAX } else { max.wrapping_add(hi as u32) };
            }
            // The file stores each pair as (weight, class).
            let classes = classes.iter().map(|&(w, c)| (c, w)).collect();
            elements.push(Element { id: e.id, placement, priority, arrangement, spacing, min_units: lo, max_units: hi, classes });
        }
        Self {
            name: g.name.clone(),
            priority: f32::from_bits(g.header[0]),
            purposes: g.header[1],
            min_percent: [g.header[2], g.header[3], g.header[4]],
            factions: g.factions.clone(),
            elements,
            min_units: min,
            max_units: max,
        }
    }

    fn element_index(&self, id: u32) -> Option<usize> {
        self.elements.iter().position(|e| e.id == id)
    }
}

/// One unit to be laid out: its class id, role and footprint (width across, depth).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupUnit {
    pub class: u32,
    pub role: Role,
    pub width: f32,
    pub depth: f32,
}

/// The template the exe picks for `units` (`0x006C66A0`, CONFIRMED): among templates with the
/// purpose bits, a unit count within the template's bounds, the faction allowed and the
/// composition shares met, the one with the highest `priority × assignment score` (first on
/// ties). A template whose assignment fails scores −1, so a priority-0 template only wins when
/// nothing scores above 0. When no template passes the filters the answer is index 0.
pub fn choose(templates: &[Template], units: &[GroupUnit], faction: &str, purpose: u32) -> usize {
    let mut best = (0usize, -1.0f32);
    for (i, t) in templates.iter().enumerate() {
        let n = units.len() as u32;
        let purpose_ok = purpose & 0x10 != 0 || t.purposes & purpose == purpose;
        if !purpose_ok || n < t.min_units || n > t.max_units {
            continue;
        }
        if !t.factions.is_empty() && !t.factions.iter().any(|f| f.eq_ignore_ascii_case(faction)) {
            continue;
        }
        if !composition_ok(t, units) {
            continue;
        }
        let score = assign(t, units).map_or(-1.0, |(s, _)| t.priority * s);
        if best.1 < score {
            best = (i, score);
        }
    }
    best.0
}

/// The composition test (`0x006EFD70`): integer shares `count * 100 / units` of artillery,
/// cavalry and infantry must reach the template's minimums.
pub fn composition_ok(t: &Template, units: &[GroupUnit]) -> bool {
    if units.is_empty() {
        return true;
    }
    let n = units.len() as u32;
    let share = |r: Role| units.iter().filter(|u| u.role == r).count() as u32 * 100 / n;
    share(Role::Artillery) >= t.min_percent[0] && share(Role::Cavalry) >= t.min_percent[1] && share(Role::Infantry) >= t.min_percent[2]
}

/// The weight of a unit class in an element: the first matching class entry (`0x006C31D0`).
fn class_weight(e: &Element, class: u32) -> f32 {
    e.classes.iter().find(|(c, _)| *c == class).map_or(0.0, |(_, w)| *w)
}

/// An element's bid for one unit (`0x006C3020`, CONFIRMED): −1 when the element is full; a
/// class weight of 0 becomes 0.001 while the element is under its minimum (else −1); the bid
/// is `priority / (assigned + 1) × weight`.
fn bid(e: &Element, assigned: usize, class: u32) -> f32 {
    if e.max_units != UNLIMITED && assigned as i32 == e.max_units {
        return -1.0;
    }
    let mut w = class_weight(e, class);
    if w == 0.0 {
        if (assigned as i32) >= e.min_units {
            return -1.0;
        }
        w = 0.001;
    }
    e.priority / (assigned as f32 + 1.0) * w
}

/// Assigns every unit to an element (`0x0069F3A0`, CONFIRMED). Repeatedly, every block or
/// relative element names its best remaining unit (first highest bid) with an urgency
/// `best + (best − second)` (`2 × best` when no second bid is ≥ 0; +1 while the element is under
/// its minimum, `0x006B4300`); the most urgent element (first on ties) takes its unit and its
/// bid is added to the score. Fails (`None`) when a unit is left that no element bids for, or
/// the count is outside the template's bounds. An element left under its minimum multiplies
/// the score by 0.01. With a single bidding element every unit goes to it and the score is
/// that element's priority (CONFIRMED). Returns the score and each unit's element index.
pub fn assign(t: &Template, units: &[GroupUnit]) -> Option<(f32, Vec<usize>)> {
    let n = units.len() as u32;
    if n < t.min_units || n > t.max_units {
        return None;
    }
    let bidders: Vec<usize> = (0..t.elements.len())
        .filter(|&i| matches!(t.elements[i].placement, Placement::Block { .. } | Placement::Relative { .. }))
        .collect();
    let mut out = vec![usize::MAX; units.len()];
    if bidders.len() == 1 {
        out.iter_mut().for_each(|o| *o = bidders[0]);
        return Some((t.elements[bidders[0]].priority, out));
    }
    let mut counts = vec![0usize; t.elements.len()];
    let mut left: Vec<usize> = (0..units.len()).collect();
    let mut score = 0.0f32;
    while !left.is_empty() {
        // (element, unit position in `left`, urgency, bid)
        let mut pick: Option<(usize, usize, f32, f32)> = None;
        for &ei in &bidders {
            let e = &t.elements[ei];
            let (mut best, mut second, mut best_at) = (-1.0f32, -1.0f32, None);
            for (k, &u) in left.iter().enumerate() {
                let b = bid(e, counts[ei], units[u].class);
                if b > best {
                    second = best.max(second);
                    best = b;
                    best_at = Some(k);
                } else if b > second {
                    second = b;
                }
            }
            let Some(k) = best_at else { continue };
            if best < 0.0 {
                continue;
            }
            let mut urgency = if second >= 0.0 { best - second + best } else { best + best };
            if (counts[ei] as i32) < e.min_units {
                urgency += 1.0;
            }
            if pick.is_none_or(|p| p.2 < urgency) {
                pick = Some((ei, k, urgency, best));
            }
        }
        let (ei, k, _, b) = pick?;
        score += b;
        out[left.remove(k)] = ei;
        counts[ei] += 1;
    }
    if bidders.iter().any(|&ei| (counts[ei] as i32) < t.elements[ei].min_units) {
        score *= 0.01;
    }
    Some((score, out))
}

/// A rectangle in the group frame: x right, y forward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: (f32, f32),
    pub max: (f32, f32),
}

impl Rect {
    pub fn width(&self) -> f32 {
        self.max.0 - self.min.0
    }
    pub fn depth(&self) -> f32 {
        self.max.1 - self.min.1
    }
    fn sized(&self) -> bool {
        self.width() > 0.0 && self.depth() > 0.0
    }
    fn shift(&mut self, dx: f32, dy: f32) {
        self.min = (self.min.0 + dx, self.min.1 + dy);
        self.max = (self.max.0 + dx, self.max.1 + dy);
    }
    fn union(&self, o: &Rect) -> Rect {
        Rect { min: (self.min.0.min(o.min.0), self.min.1.min(o.min.1)), max: (self.max.0.max(o.max.0), self.max.1.max(o.max.1)) }
    }
}

/// Lays out assigned units in the group frame (x right, y forward, metres). Returns each unit's
/// footprint centre and the group's bounding rectangle.
///
/// - Inside an element (CONFIRMED, arrangement functions `0x0067BE80` line and `0x0067BFF0`
///   column): a line is `Σ width + (n − 1) · spacing` wide and as deep as its deepest unit,
///   units side by side from the left with their fronts on the element's front edge; a column is
///   as wide as its widest unit, units one behind the other from the front. Units keep their
///   assignment order (the exe then sorts them by their current positions across the facing;
///   at deployment that order is INFERRED to be the same).
/// - An element's box starts centred on the origin; a block moves by its `pos` (`0x006DFEF0`).
/// - A relative element (`0x006DFF40`, CONFIRMED) is placed after its anchor (an anchor with no
///   size passes on to its own anchor, `0x006BFFD0`): with `offset.x > 0` its left edge sits
///   `offset.x` beyond the anchor's right edge (`< 0`: mirrored), and with `offset.y = 0` its
///   front lines up with the anchor's front; with `offset.y > 0` its back sits `offset.y` in
///   front of the anchor's front (`< 0`: its front `|offset.y|` behind the anchor's back), and
///   with `offset.x = 0` it is centred on the anchor's centre line.
/// - A group element's box is the union of its members (INFERRED); the group's box is the union
///   of the elements with a size (`0x006A7C10`).
pub fn layout(t: &Template, units: &[GroupUnit], assignment: &[usize]) -> (Vec<(f32, f32)>, Rect) {
    let ne = t.elements.len();
    let mut local = vec![(0.0f32, 0.0f32); units.len()];
    let mut boxes = vec![Rect { min: (0.0, 0.0), max: (0.0, 0.0) }; ne];
    for (ei, e) in t.elements.iter().enumerate() {
        let members: Vec<usize> = (0..units.len()).filter(|&u| assignment.get(u) == Some(&ei)).collect();
        if members.is_empty() {
            continue;
        }
        let gaps = (members.len() - 1) as f32 * e.spacing;
        let (w, d) = if e.arrangement == 1 {
            let w = members.iter().map(|&u| units[u].width).fold(0.0, f32::max);
            let d = members.iter().map(|&u| units[u].depth).sum::<f32>() + gaps;
            let mut front = d * 0.5;
            for &u in &members {
                local[u] = (0.0, front - units[u].depth * 0.5);
                front -= units[u].depth + e.spacing;
            }
            (w, d)
        } else {
            let w = members.iter().map(|&u| units[u].width).sum::<f32>() + gaps;
            let d = members.iter().map(|&u| units[u].depth).fold(0.0, f32::max);
            let mut left = -w * 0.5;
            for &u in &members {
                local[u] = (left + units[u].width * 0.5, d * 0.5 - units[u].depth * 0.5);
                left += units[u].width + e.spacing;
            }
            (w, d)
        };
        boxes[ei] = Rect { min: (-w * 0.5, -d * 0.5), max: (w * 0.5, d * 0.5) };
    }
    let mut placed = vec![false; ne];
    for ei in 0..ne {
        place(t, ei, &mut boxes, &mut placed, 0);
    }
    let mut out = vec![(0.0, 0.0); units.len()];
    for (u, &ei) in assignment.iter().enumerate() {
        if let Some(b) = boxes.get(ei) {
            let c = ((b.min.0 + b.max.0) * 0.5, (b.min.1 + b.max.1) * 0.5);
            out[u] = (c.0 + local[u].0, c.1 + local[u].1);
        }
    }
    let mut bounds: Option<Rect> = None;
    for (ei, b) in boxes.iter().enumerate() {
        if matches!(t.elements[ei].placement, Placement::Group { .. }) || !b.sized() {
            continue;
        }
        bounds = Some(bounds.map_or(*b, |r| r.union(b)));
    }
    (out, bounds.unwrap_or(Rect { min: (0.0, 0.0), max: (0.0, 0.0) }))
}

fn place(t: &Template, ei: usize, boxes: &mut [Rect], placed: &mut [bool], depth: usize) {
    if placed[ei] || depth > t.elements.len() {
        return;
    }
    placed[ei] = true;
    match &t.elements[ei].placement {
        Placement::Block { pos } => boxes[ei].shift(pos.0, pos.1),
        Placement::Spanning { .. } => {}
        Placement::Group { members } => {
            let mut u: Option<Rect> = None;
            for m in members {
                let Some(mi) = t.element_index(*m) else { continue };
                place(t, mi, boxes, placed, depth + 1);
                let b = boxes[mi];
                if b.sized() {
                    u = Some(u.map_or(b, |r| r.union(&b)));
                }
            }
            if let Some(u) = u {
                boxes[ei] = u;
            }
        }
        Placement::Relative { anchor, offset } => {
            // The anchor: the first one up the chain with a size.
            let mut a_index = t.element_index(*anchor);
            let mut steps = 0;
            let anchor_rect = loop {
                let Some(ai) = a_index else { break None };
                place(t, ai, boxes, placed, depth + 1);
                let b = boxes[ai];
                if b.sized() {
                    break Some(b);
                }
                a_index = match &t.elements[ai].placement {
                    Placement::Relative { anchor, .. } => t.element_index(*anchor),
                    _ => None,
                };
                steps += 1;
                if steps > t.elements.len() {
                    break None;
                }
            };
            let Some(a) = anchor_rect else { return };
            let (w, d) = (boxes[ei].width(), boxes[ei].depth());
            let (ox, oy) = *offset;
            if ox != 0.0 {
                let dx = if ox >= 0.0 { a.max.0 + w * 0.5 + ox } else { a.min.0 - w * 0.5 + ox };
                boxes[ei].shift(dx, 0.0);
                if oy == 0.0 {
                    boxes[ei].shift(0.0, a.max.1 - d * 0.5);
                }
            }
            if oy != 0.0 {
                let dy = if oy >= 0.0 { a.max.1 + d * 0.5 + oy } else { a.min.1 - d * 0.5 + oy };
                boxes[ei].shift(0.0, dy);
                if ox == 0.0 {
                    boxes[ei].shift((a.min.0 + a.max.0) * 0.5, 0.0);
                }
            }
        }
    }
}

/// Every template of a file, typed.
pub fn read_templates(bytes: &[u8]) -> Result<Vec<Template>, GroupFormationError> {
    Ok(read(bytes)?.iter().map(Template::from_raw).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(id: u32, placement: Placement, classes: &[(u32, f32)], spacing: f32) -> Element {
        Element { id, placement, priority: 1.0, arrangement: 0, spacing, min_units: 0, max_units: UNLIMITED, classes: classes.to_vec() }
    }

    fn template() -> Template {
        Template {
            name: "test".into(),
            priority: 1.0,
            purposes: PURPOSE_DEPLOYMENT,
            min_percent: [0, 0, 0],
            factions: Vec::new(),
            elements: vec![
                element(0, Placement::Block { pos: (0.0, 0.0) }, &[(18, 1.0)], 2.0),
                element(1, Placement::Relative { anchor: 0, offset: (5.0, 0.0) }, &[(1, 1.0)], 6.0),
                element(2, Placement::Relative { anchor: 0, offset: (0.0, -10.0) }, &[(12, 1.0)], 2.0),
            ],
            min_units: 0,
            max_units: u32::MAX,
        }
    }

    fn unit(class: &str, role: Role, width: f32, depth: f32) -> GroupUnit {
        GroupUnit { class: class_id(class), role, width, depth }
    }

    #[test]
    fn class_ids_follow_the_exe_order() {
        assert_eq!(class_id("artillery_foot"), 1);
        assert_eq!(class_id("general"), 12);
        assert_eq!(class_id("infantry_line"), 18);
        assert_eq!(class_id("naval_transport"), 45);
        assert_eq!(class_id("no_such_class"), 0);
    }

    #[test]
    fn assignment_and_layout() {
        let t = template();
        let units = [
            unit("infantry_line", Role::Infantry, 40.0, 6.0),
            unit("artillery_foot", Role::Artillery, 20.0, 10.0),
            unit("infantry_line", Role::Infantry, 40.0, 6.0),
            unit("general", Role::Cavalry, 10.0, 8.0),
        ];
        let (score, a) = assign(&t, &units).unwrap();
        assert_eq!(a, [0, 1, 0, 2]);
        assert!(score > 0.0);
        let (pos, bounds) = layout(&t, &units, &a);
        // Line of two 40 m units with a 2 m gap, centred.
        assert_eq!(pos[0], (-21.0, 0.0));
        assert_eq!(pos[2], (21.0, 0.0));
        // Artillery 5 m right of the line, fronts level (front edge y = 3).
        assert_eq!(pos[1], (56.0, -2.0));
        // General 10 m behind the line's back edge, centred.
        assert_eq!(pos[3], (0.0, -17.0));
        assert_eq!(bounds, Rect { min: (-41.0, -21.0), max: (66.0, 3.0) });
    }

    #[test]
    fn full_and_unwanted_elements_do_not_bid() {
        let mut t = template();
        t.elements[1].max_units = 1;
        let units = [unit("artillery_foot", Role::Artillery, 20.0, 10.0), unit("artillery_foot", Role::Artillery, 20.0, 10.0)];
        // The second gun has nowhere to go: the line wants infantry only and has no minimum.
        assert!(assign(&t, &units).is_none());
        // With a minimum on the line it takes the gun at weight 0.001.
        t.elements[0].min_units = 1;
        let (_, a) = assign(&t, &units).unwrap();
        assert_eq!(a.iter().filter(|&&e| e == 0).count(), 1);
    }

    #[test]
    fn choose_respects_filters_and_scores() {
        let mut low = template();
        low.name = "low".into();
        low.priority = 0.5;
        let mut cav = template();
        cav.name = "cav".into();
        cav.min_percent = [0, 40, 0];
        let units = [unit("infantry_line", Role::Infantry, 40.0, 6.0), unit("artillery_foot", Role::Artillery, 20.0, 10.0)];
        let ts = [template(), low.clone(), cav];
        assert_eq!(choose(&ts, &units, "france", PURPOSE_DEPLOYMENT), 0);
        assert_eq!(choose(&ts[1..], &units, "france", PURPOSE_DEPLOYMENT), 0, "only 'low' passes");
        assert_eq!(choose(&ts, &units, "france", PURPOSE_DRAG_OUT), 0, "nothing passes: index 0");
        let mut only_egypt = template();
        only_egypt.factions = vec!["egy_mamelukes".into()];
        assert_eq!(choose(&[low, only_egypt], &units, "france", PURPOSE_DEPLOYMENT), 0);
    }
}

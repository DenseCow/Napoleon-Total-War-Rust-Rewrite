//! The campaign AI block (`CAI_INTERFACE`) as far as the save writer touches it
//! (analysis/campaign/SAVE_COMPAT.md §2, §5).
//!
//! **Per-faction AI manager type** (CONFIRMED): `CAI_INTERFACE` #29 is a u32 array of pairs
//! (faction component id, manager type), loaded into the table the manager factory reads
//! (`FUN_00a88cb0` → `FUN_00a87f50`, which switches on the type and replaces a manager of another
//! type); each `CAI_INTERFACE_MANAGERS[]/CAI_FACTION_MANAGER` repeats it as {#0 component, #1 type}.
//! The component of a faction is the first plain value of its `CAI_WORLD/CAI_WORLD_FACTIONS[]`
//! item, and `CAI_FACTION` #6 there is the faction id (`FACTION` #8). Types in the order of the
//! exe's `CAIMT_*` names (INFERRED from the string order, AI_RESEARCH.md): 2 = REBELLION (the
//! rebels), 10 = HUMAN, 11 = DB. A startpos gives every faction but the rebels 11; the user's
//! saves give the human faction 10 and every other 11 (CONFIRMED in all six Britain saves).

use std::collections::{BTreeMap, BTreeSet};

use ntw_formats::esf::{EsfFile, EsfNode, EsfRecord};

/// `CAIMT_HUMAN`: the manager type of the human faction (CONFIRMED value).
pub const MANAGER_HUMAN: u32 = 10;
/// `CAIMT_DB`: the manager type of every AI faction in the startpos (CONFIRMED value).
pub const MANAGER_DB: u32 = 11;

/// A faction's AI manager as stored: (faction id, component id, type in #29, type in its
/// `CAI_FACTION_MANAGER`, `None` when absent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagerType {
    /// `FACTION` #8.
    pub faction: u32,
    /// The faction's CAI component id.
    pub component: u32,
    /// The type in `CAI_INTERFACE` #29.
    pub listed: Option<u32>,
    /// The type in the faction's `CAI_FACTION_MANAGER` #1.
    pub manager: Option<u32>,
}

/// Faction id → CAI component id, from `CAI_WORLD/CAI_WORLD_FACTIONS[]`.
fn faction_components(cai: &EsfRecord) -> Vec<(u32, u32)> {
    let Some(a) = cai.child("CAI_WORLD").and_then(|w| w.record_array("CAI_WORLD_FACTIONS")) else { return Vec::new() };
    a.items
        .iter()
        .filter_map(|it| {
            let component = it.iter().find(|n| !matches!(n, EsfNode::Record(_) | EsfNode::RecordArray(_))).and_then(EsfNode::as_u32)?;
            let faction = it.iter().filter_map(EsfNode::as_record).find(|r| r.name == "CAI_FACTION")?.get_u32(6)?;
            Some((faction, component))
        })
        .collect()
}

/// Every faction's AI manager type as stored in a save or startpos.
pub fn manager_types(esf: &EsfFile) -> Vec<ManagerType> {
    let Some(cai) = esf.root.find_path("CAMPAIGN_ENV/CAMPAIGN_MODEL/CAI_INTERFACE") else { return Vec::new() };
    let pairs: Vec<u32> = cai.get(29).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default();
    let managers: Vec<(u32, u32)> = cai
        .record_array("CAI_INTERFACE_MANAGERS")
        .into_iter()
        .flat_map(|a| a.items.iter())
        .filter_map(|it| it.iter().filter_map(EsfNode::as_record).find(|r| r.name == "CAI_FACTION_MANAGER"))
        .filter_map(|m| Some((m.get_u32(0)?, m.get_u32(1)?)))
        .collect();
    faction_components(cai)
        .into_iter()
        .map(|(faction, component)| ManagerType {
            faction,
            component,
            listed: pairs.chunks(2).find(|p| p.len() == 2 && p[0] == component).map(|p| p[1]),
            manager: managers.iter().find(|m| m.0 == component).map(|m| m.1),
        })
        .collect()
}

/// Gives the human faction (`FACTION` #8 = `human`) the HUMAN manager type and a faction that had
/// it before the DB type, in #29 and in the `CAI_FACTION_MANAGER`s. Other types are kept.
pub(crate) fn write_manager_types(cai: &mut EsfRecord, human: u32) {
    let comps = faction_components(cai);
    let want = |component: u32, old: u32| -> u32 {
        let is_human = comps.iter().any(|&(f, c)| c == component && f == human);
        if is_human {
            MANAGER_HUMAN
        } else if old == MANAGER_HUMAN {
            MANAGER_DB
        } else {
            old
        }
    };
    if let Some(EsfNode::U32Array(v)) = cai.children.get_mut(29) {
        for p in v.chunks_mut(2) {
            if let [c, t] = p {
                *t = want(*c, *t);
            }
        }
    }
    for c in &mut cai.children {
        let EsfNode::RecordArray(a) = c else { continue };
        if a.name != "CAI_INTERFACE_MANAGERS" {
            continue;
        }
        for it in &mut a.items {
            for n in it.iter_mut() {
                let EsfNode::Record(m) = n else { continue };
                if m.name != "CAI_FACTION_MANAGER" {
                    continue;
                }
                if let (Some(c), Some(t)) = (m.get_u32(0), m.get_u32(1))
                    && let Some(slot) = m.children.get_mut(1)
                {
                    *slot = EsfNode::U32(want(c, t));
                }
            }
        }
    }
}

/// The block every AI (BDI) component carries in a save, right after its
/// `CAI_BDI_COMPONENT_PROPERTY_SET` record (AI_RESEARCH.md, startpos schema; field meanings from
/// the component ctor `0x00C7BC90`, INFERRED where noted). Positions are relative to the
/// property set's node (`+1` = the id).
#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    /// The component id (save-time numbering: ids are reassigned on every save, CONFIRMED by the
    /// Coalition sequence where every world mirror's id changes between consecutive saves).
    pub id: u32,
    /// Record path of the node list holding the block, and the property set's index in it.
    pub path: String,
    /// The name of the record that follows the block (the component's class record), if any.
    pub class: String,
    /// The four u32 arrays at `+8`, `+9`, `+12`, `+13`.
    pub lists_a: [Vec<u32>; 4],
    /// `CAI_BDI_COMPONENT_BLOCK_OWNS` items (+14): {u32, u32, f32, f32, u32}.
    pub owns: Vec<(u32, u32, f32, f32, u32)>,
    /// The four u32 arrays at `+17`..`+20`.
    pub lists_b: [Vec<u32>; 4],
}

/// The node offsets (from the property set) and kinds of the component block.
const BLOCK_LEN: usize = 24;

fn is_block(nodes: &[EsfNode], p: usize) -> bool {
    use EsfNode as N;
    let k = |i: usize| nodes.get(p + i);
    matches!(k(0), Some(N::Record(r)) if r.name == "CAI_BDI_COMPONENT_PROPERTY_SET")
        && matches!(k(1), Some(N::U32(_)))
        && matches!((k(2), k(3), k(4), k(5)), (Some(N::F32(_)), Some(N::F32(_)), Some(N::F32(_)), Some(N::F32(_))))
        && matches!(k(14), Some(N::RecordArray(a)) if a.name == "CAI_BDI_COMPONENT_BLOCK_OWNS")
        && matches!((k(17), k(18), k(19), k(20)), (Some(N::U32Array(_)), Some(N::U32Array(_)), Some(N::U32Array(_)), Some(N::U32Array(_))))
}

fn arr(nodes: &[EsfNode], i: usize) -> Vec<u32> {
    nodes.get(i).and_then(EsfNode::as_u32_array).map(<[u32]>::to_vec).unwrap_or_default()
}

/// Every component block in the AI block, in file order.
pub fn components(cai: &EsfRecord) -> Vec<Component> {
    fn scan(nodes: &[EsfNode], path: &str, out: &mut Vec<Component>) {
        for p in 0..nodes.len() {
            if is_block(nodes, p) {
                let owns = match &nodes[p + 14] {
                    EsfNode::RecordArray(a) => a
                        .items
                        .iter()
                        .map(|it| {
                            let u = |i: usize| it.get(i).and_then(EsfNode::as_u32).unwrap_or(0);
                            let f = |i: usize| it.get(i).and_then(EsfNode::as_f32).unwrap_or(0.0);
                            (u(0), u(1), f(2), f(3), u(4))
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                let class = nodes[p + BLOCK_LEN..].iter().find_map(|n| n.as_record().map(|r| r.name.clone())).unwrap_or_default();
                out.push(Component {
                    id: nodes[p + 1].as_u32().unwrap_or(0),
                    path: format!("{path}@{p}"),
                    class,
                    lists_a: [arr(nodes, p + 8), arr(nodes, p + 9), arr(nodes, p + 12), arr(nodes, p + 13)],
                    owns,
                    lists_b: [arr(nodes, p + 17), arr(nodes, p + 18), arr(nodes, p + 19), arr(nodes, p + 20)],
                });
            }
        }
        for (i, n) in nodes.iter().enumerate() {
            match n {
                EsfNode::Record(r) => scan(&r.children, &format!("{path}/{}#{i}", r.name), out),
                EsfNode::RecordArray(a) => {
                    for (k, it) in a.items.iter().enumerate() {
                        scan(it, &format!("{path}/{}[{k}]", a.name), out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    scan(&cai.children, "", &mut out);
    out
}

/// The positions of the component blocks (their `CAI_BDI_COMPONENT_PROPERTY_SET`) in a node list.
/// Most lists hold at most one; structural records hold several (`CAI_WORLD` ends with the
/// `CAI_HISTORY` component, CONFIRMED).
pub fn block_positions(nodes: &[EsfNode]) -> Vec<usize> {
    (0..nodes.len()).filter(|&p| is_block(nodes, p)).collect()
}

/// The first component block of a node list.
pub fn block_pos(nodes: &[EsfNode]) -> Option<usize> {
    (0..nodes.len()).find(|&p| is_block(nodes, p))
}

/// The block a node at `i` belongs to: the last block at or before `i`, else the first block.
fn block_of(blocks: &[usize], i: usize) -> Option<usize> {
    blocks.iter().rev().find(|&&p| p <= i).or(blocks.first()).copied()
}

/// Offsets (from the property set) of the block's id lists (CONFIRMED: every value is a component
/// id in the original's saves).
pub const LINK_FLAT: [usize; 6] = [12, 13, 17, 18, 19, 20];
/// Offset of the list whose values are component ids or 0 (fixed slots; 0 = none, CONFIRMED).
pub const LINK_SLOTS: usize = 8;
/// Offset of the pair list ({target id, slot}; one pair per `BLOCK_OWNS` link, CONFIRMED: same
/// totals in every save).
pub const LINK_PAIRS: usize = 9;
/// Offset of the `CAI_BDI_COMPONENT_BLOCK_OWNS` array ({u32 id, u32 id, f32, f32, u32}).
pub const LINK_OWNS: usize = 14;

/// What kind of link position `rel` (offset from the property set) is, if any.
fn link_kind(rel: Option<usize>) -> Option<usize> {
    let r = rel?;
    (r == LINK_SLOTS || r == LINK_PAIRS || r == LINK_OWNS || LINK_FLAT.contains(&r)).then_some(r)
}

/// Where a value sits relative to its component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// In the component block's link lists or `BLOCK_OWNS` items.
    Link,
    /// A u32 directly in the component's node list or in records below it (no record array).
    Scalar,
    /// An element of a u32 array of the component's data.
    ArrayElement,
    /// Inside an item of a record array that belongs to the component's data.
    InDataItem,
}

fn pair_tag(k: usize) -> &'static str {
    if k.is_multiple_of(2) { "even" } else { "odd" }
}

/// The component id of the block at `p`.
fn block_id(nodes: &[EsfNode], p: usize) -> u32 {
    nodes[p + 1].as_u32().unwrap_or(0)
}

/// `CAI_TAS_ANALYSIS`: a belief whose long form holds three variable lists (CONFIRMED layout on
/// every long record of the user's saves): `[u32, u32, then 3 x (u32 n, n x {u32 component, f32,
/// f32, i32}), 42 x i32, bool, u32]`; the short form is `[u32, u32]`.
const TAS: &str = "CAI_TAS_ANALYSIS";
/// Index of the first list count in a long `CAI_TAS_ANALYSIS`.
const TAS_LISTS_AT: usize = 2;

/// The lists of a long `CAI_TAS_ANALYSIS`: (count index, first entry index, entry count).
fn tas_layout(c: &[EsfNode]) -> Option<Vec<(usize, usize, usize)>> {
    let mut i = TAS_LISTS_AT;
    let mut out = Vec::new();
    for _ in 0..3 {
        let n = c.get(i)?.as_u32()? as usize;
        let start = i + 1;
        for e in 0..n {
            let k = start + 4 * e;
            if !matches!((c.get(k), c.get(k + 1), c.get(k + 2), c.get(k + 3)), (Some(EsfNode::U32(_)), Some(EsfNode::F32(_)), Some(EsfNode::F32(_)), Some(EsfNode::I32(_)))) {
                return None;
            }
        }
        out.push((i, start, n));
        i = start + 4 * n;
    }
    matches!(c.get(i), Some(EsfNode::I32(_))).then_some(out)
}

/// Drops the entries of a long `CAI_TAS_ANALYSIS` that name a removed component and fixes the
/// counts; returns false when the record is not the long form.
fn tas_fix(r: &mut EsfRecord, removed: &BTreeSet<u32>) -> bool {
    let Some(lists) = tas_layout(&r.children) else { return false };
    let old = std::mem::take(&mut r.children);
    let mut out: Vec<EsfNode> = old[..TAS_LISTS_AT].to_vec();
    let mut end = TAS_LISTS_AT;
    for (_, start, n) in lists {
        let kept: Vec<&[EsfNode]> = old[start..start + 4 * n].chunks(4).filter(|e| e[0].as_u32().is_none_or(|v| !removed.contains(&v))).collect();
        out.push(EsfNode::U32(kept.len() as u32));
        for e in kept {
            out.extend_from_slice(e);
        }
        end = start + 4 * n;
    }
    out.extend_from_slice(&old[end..]);
    r.children = out;
    true
}

/// Walks the AI block: calls `f(owner, site, place, value)` for every u32 value and u32 array
/// element, where `owner` is the component the value belongs to (the block it follows in its node
/// list, or the one enclosing the list; 0 = none) and `site` the normalised path (`/A/B[]/C #i`,
/// array elements with `[even]` / `[odd]`, `BLOCK_OWNS` items ` owns`).
pub fn walk_values(nodes: &[EsfNode], path: &str, f: &mut dyn FnMut(u32, &str, Place, u32)) {
    fn go(nodes: &[EsfNode], path: &str, owner: u32, in_item: bool, f: &mut dyn FnMut(u32, &str, Place, u32)) {
        let blocks = block_positions(nodes);
        let in_item = in_item && blocks.is_empty();
        for (i, n) in nodes.iter().enumerate() {
            let b = block_of(&blocks, i);
            let owner = b.map_or(owner, |p| block_id(nodes, p));
            let rel = b.and_then(|p| i.checked_sub(p));
            let link = link_kind(rel).is_some();
            // The block's own scalars (id, priorities, counters, flags) are not references.
            if !link && rel.is_some_and(|r| r < BLOCK_LEN) && !matches!(n, EsfNode::Record(_)) {
                continue;
            }
            match n {
                EsfNode::Record(r) if r.name == TAS && tas_layout(&r.children).is_some() => {
                    let lists = tas_layout(&r.children).unwrap_or_default();
                    let p = format!("{path}/{}", r.name);
                    for (k, c) in r.children.iter().enumerate().take(TAS_LISTS_AT) {
                        if let Some(v) = c.as_u32() {
                            f(owner, &format!("{p} #{k}"), Place::Scalar, v);
                        }
                    }
                    for (_, start, len) in lists {
                        for e in 0..len {
                            if let Some(v) = r.children[start + 4 * e].as_u32() {
                                f(owner, &format!("{p} entry"), Place::ArrayElement, v);
                            }
                        }
                    }
                }
                EsfNode::Record(r) => go(&r.children, &format!("{path}/{}", r.name), owner, in_item, f),
                EsfNode::RecordArray(a) => {
                    let p = format!("{path}/{}[]", a.name);
                    for it in &a.items {
                        if link {
                            for x in it.iter().filter_map(EsfNode::as_u32).take(2) {
                                f(owner, &format!("{p} owns"), Place::Link, x);
                            }
                        } else {
                            go(it, &p, owner, true, f);
                        }
                    }
                }
                EsfNode::U32(v) => f(owner, &format!("{path} #{i}"), if in_item { Place::InDataItem } else { Place::Scalar }, *v),
                EsfNode::U32Array(v) => {
                    let place = if link {
                        Place::Link
                    } else if in_item {
                        Place::InDataItem
                    } else {
                        Place::ArrayElement
                    };
                    for (k, x) in v.iter().enumerate() {
                        f(owner, &format!("{path} #{i}[{}]", pair_tag(k)), place, *x);
                    }
                }
                _ => {}
            }
        }
    }
    go(nodes, path, 0, false, f);
}

/// What the original does with a data reference to a removed component (the site table,
/// `cai_sites.txt`, learnt by `cai_audit sites` from consecutive saves of the original).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteKind {
    /// The referencing component is removed too.
    Subject,
    /// The reference is dropped (array element / pair / data item removed, scalar set to 0).
    Entry,
    /// The original leaves the dangling id.
    Tolerated,
}

/// One data reference site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Site {
    /// What happens to a reference to a removed component.
    pub kind: SiteKind,
    /// 0 occurs at this site in the original's saves (so 0 is a valid "none").
    pub zero_seen: bool,
}

/// The site table, parsed once.
pub fn site_table() -> &'static BTreeMap<String, Site> {
    static T: std::sync::OnceLock<BTreeMap<String, Site>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        include_str!("cai_sites.txt")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                let kind = match *f.get(1)? {
                    "subject" => SiteKind::Subject,
                    "entry" => SiteKind::Entry,
                    _ => SiteKind::Tolerated,
                };
                Some((f[0].to_string(), Site { kind, zero_seen: f.get(3) == Some(&"1") }))
            })
            .collect()
    })
}

/// Whether a reference at `site` (placed `place`) makes its component die with the target.
fn kills_owner(site: &Site, place: Place) -> bool {
    match site.kind {
        SiteKind::Subject => true,
        SiteKind::Entry => place == Place::Scalar && !site.zero_seen,
        SiteKind::Tolerated => false,
    }
}

/// The world mirror classes: their existence follows the game world, never a reference.
pub const MIRROR_CLASSES: [&str; 3] = ["CAI_CHARACTER", "CAI_UNIT", "CAI_RESOURCE_MOBILE"];

/// The result of [`remove_components`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Removal {
    /// Every component removed (the requested ones and their dependants).
    pub removed: BTreeSet<u32>,
    /// Components that would have had to go but hold structure (not an item of a record
    /// array): kept, with the references dropped.
    pub structural_kept: BTreeSet<u32>,
    /// References to removed components left at sites the table does not know.
    pub unknown_sites: BTreeMap<String, usize>,
    /// Why each dependant was removed: the site of its reference (or `nested`).
    pub reasons: BTreeMap<u32, String>,
}

/// Every component: (id, enclosing component, is an item of a record array).
fn component_tree(nodes: &[EsfNode]) -> Vec<(u32, u32, bool)> {
    fn go(nodes: &[EsfNode], parent: u32, is_item: bool, out: &mut Vec<(u32, u32, bool)>) {
        let blocks = block_positions(nodes);
        for (k, &p) in blocks.iter().enumerate() {
            // Only the first block of an array item is the item's own component.
            out.push((block_id(nodes, p), parent, is_item && k == 0));
        }
        for (i, n) in nodes.iter().enumerate() {
            let parent = block_of(&blocks, i).map_or(parent, |p| block_id(nodes, p));
            match n {
                EsfNode::Record(r) => go(&r.children, parent, false, out),
                EsfNode::RecordArray(a) => {
                    for it in &a.items {
                        go(it, parent, true, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    go(nodes, 0, false, &mut out);
    out
}

/// Removes the components `ids` from the AI block (`CAI_INTERFACE` record) the way the original
/// does when the objects they stand for disappear (SAVE_COMPAT.md §10): the components that
/// depend on a removed one (a `subject` reference, a scalar reference that cannot be 0, or being
/// nested inside it) go too, transitively, except world mirrors (their existence follows the
/// world); every other reference is dropped from the link lists, the `BLOCK_OWNS` links, the data
/// arrays and data items, or set to 0; `tolerated` sites keep the dangling id as the original does.
pub fn remove_components(cai: &mut EsfRecord, ids: &BTreeSet<u32>) -> Removal {
    let table = site_table();
    let mut out = Removal::default();
    let tree = component_tree(&cai.children);
    let structural: BTreeSet<u32> = tree.iter().filter(|t| !t.2).map(|t| t.0).collect();
    let mirrors: BTreeSet<u32> = components(cai).into_iter().filter(|c| MIRROR_CLASSES.contains(&c.class.as_str())).map(|c| c.id).collect();
    let mut deps: BTreeMap<u32, Vec<(u32, String)>> = BTreeMap::new();
    for &(id, parent, _) in &tree {
        if parent != 0 {
            deps.entry(parent).or_default().push((id, "nested".into()));
        }
    }
    walk_values(&cai.children, "", &mut |owner, site, place, v| {
        if owner == 0 || owner == v || place == Place::Link || mirrors.contains(&owner) {
            return;
        }
        if table.get(site).is_some_and(|s| kills_owner(s, place)) {
            deps.entry(v).or_default().push((owner, site.to_string()));
        }
    });
    let mut stack: Vec<u32> = ids.iter().copied().collect();
    while let Some(c) = stack.pop() {
        if structural.contains(&c) {
            out.structural_kept.insert(c);
            continue;
        }
        if out.removed.insert(c) {
            for (d, why) in deps.get(&c).into_iter().flatten() {
                out.reasons.entry(*d).or_insert_with(|| why.clone());
                stack.push(*d);
            }
        }
    }
    let removed = out.removed.clone();
    let mut links = Vec::new();
    all_links(&cai.children, &mut links);
    let mut in_links: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (s, t, slot) in links {
        if removed.contains(&s) {
            in_links.entry((s, t)).or_default().push(slot);
        }
    }
    // The `BLOCK_OWNS` entries that go (their target is removed), per kept source, by index: the
    // other targets' incoming pairs index this list, so they are renumbered (SAVE_COMPAT.md §30).
    let mut owns = BTreeMap::new();
    owns_targets(&cai.children, &mut owns);
    let removed_owns: BTreeMap<u32, Vec<u32>> = owns
        .iter()
        .filter(|(s, _)| !removed.contains(s))
        .map(|(&s, targets)| (s, targets.iter().enumerate().filter(|(_, t)| removed.contains(t)).map(|(i, _)| i as u32).collect::<Vec<u32>>()))
        .filter(|(_, v)| !v.is_empty())
        .collect();
    let mut ctx = Fix { removed: &removed, table, unknown: &mut out.unknown_sites, in_links, removed_owns };
    fix_nodes(&mut cai.children, "", false, &mut ctx);
    out
}

/// What the fixer needs: the removed components, the site table, the unknown sites seen, the
/// slots of the links whose source is removed, by (source, target), and the indexes of the
/// `BLOCK_OWNS` entries each kept source loses.
struct Fix<'a> {
    removed: &'a BTreeSet<u32>,
    table: &'a BTreeMap<String, Site>,
    unknown: &'a mut BTreeMap<String, usize>,
    in_links: BTreeMap<(u32, u32), Vec<u32>>,
    removed_owns: BTreeMap<u32, Vec<u32>>,
}

/// True when a data item (no component block) holds an `entry` / `subject` reference to a removed
/// component (not looking into nested component items).
fn item_hit(nodes: &[EsfNode], path: &str, removed: &BTreeSet<u32>, table: &BTreeMap<String, Site>) -> bool {
    for (i, n) in nodes.iter().enumerate() {
        let hit = match n {
            EsfNode::Record(r) => item_hit(&r.children, &format!("{path}/{}", r.name), removed, table),
            EsfNode::RecordArray(a) => {
                let p = format!("{path}/{}[]", a.name);
                a.items.iter().any(|it| block_pos(it).is_none() && item_hit(it, &p, removed, table))
            }
            EsfNode::U32(v) => removed.contains(v) && table.get(&format!("{path} #{i}")).is_some_and(|s| s.kind != SiteKind::Tolerated),
            EsfNode::U32Array(v) => v.iter().enumerate().any(|(k, x)| {
                removed.contains(x) && table.get(&format!("{path} #{i}[{}]", pair_tag(k))).is_some_and(|s| s.kind != SiteKind::Tolerated)
            }),
            _ => false,
        };
        if hit {
            return true;
        }
    }
    false
}

/// Drops removed components from one block's link lists and keeps its link counters exact.
fn fix_block(nodes: &mut [EsfNode], p: usize, ctx: &mut Fix) {
    let me = block_id(nodes, p);
    let removed = ctx.removed;
    let mut dec_in = [0u32; 2];
    if let Some(EsfNode::U32Array(v)) = nodes.get_mut(p + LINK_PAIRS) {
        let mut keep = Vec::with_capacity(v.len());
        for pair in v.chunks(2) {
            if removed.contains(&pair[0]) {
                let slot = ctx.in_links.get_mut(&(pair[0], me)).and_then(Vec::pop).unwrap_or(0);
                dec_in[slot.min(1) as usize] += 1;
            } else {
                // (source, index into the source's BLOCK_OWNS): the index moves up for every
                // entry the source loses in front of it (the loader resolves it by position).
                keep.push(pair[0]);
                keep.push(ctx.removed_owns.get(&pair[0]).map_or(pair[1], |gone| renumbered_pair_index(gone, pair[1])));
            }
        }
        *v = keep;
    }
    let mut dec_out = [0u32; 2];
    if let Some(EsfNode::RecordArray(a)) = nodes.get_mut(p + LINK_OWNS) {
        a.items.retain(|it| {
            let target = it.first().and_then(EsfNode::as_u32).unwrap_or(0);
            if removed.contains(&target) {
                dec_out[it.get(4).and_then(EsfNode::as_u32).unwrap_or(0).min(1) as usize] += 1;
                false
            } else {
                true
            }
        });
    }
    for off in LINK_FLAT {
        if let Some(EsfNode::U32Array(v)) = nodes.get_mut(p + off) {
            v.retain(|x| !removed.contains(x));
        }
    }
    if let Some(EsfNode::U32Array(v)) = nodes.get_mut(p + LINK_SLOTS) {
        v.iter_mut().filter(|x| removed.contains(x)).for_each(|x| *x = 0);
    }
    for (offs, dec) in [(IN_SLOT, dec_in), (OUT_SLOT, dec_out)] {
        for k in 0..2 {
            if let Some(EsfNode::U32(c)) = nodes.get_mut(p + offs[k]) {
                *c = c.saturating_sub(dec[k]);
            }
        }
    }
}

fn fix_nodes(nodes: &mut [EsfNode], path: &str, in_item: bool, ctx: &mut Fix) {
    let blocks = block_positions(nodes);
    let in_item = in_item && blocks.is_empty();
    for &p in &blocks {
        fix_block(nodes, p, ctx);
    }
    let (removed, table) = (ctx.removed, ctx.table);
    #[allow(clippy::needless_range_loop)] // nodes[i] is mutated while `blocks` (indices) is read
    for i in 0..nodes.len() {
        let rel = block_of(&blocks, i).and_then(|p| i.checked_sub(p));
        if link_kind(rel).is_some() || (rel.is_some_and(|r| r < BLOCK_LEN) && !matches!(nodes[i], EsfNode::Record(_))) {
            continue;
        }
        match &mut nodes[i] {
            EsfNode::Record(r) => {
                let p = format!("{path}/{}", r.name);
                if r.name == TAS && tas_fix(r, removed) {
                    // The two leading scalars; the lists are done.
                    for k in 0..TAS_LISTS_AT {
                        if let Some(EsfNode::U32(v)) = r.children.get_mut(k)
                            && removed.contains(v)
                        {
                            match table.get(&format!("{p} #{k}")) {
                                Some(s) if s.kind == SiteKind::Tolerated => {}
                                Some(_) => *v = 0,
                                None => *ctx.unknown.entry(format!("{p} #{k}")).or_default() += 1,
                            }
                        }
                    }
                    continue;
                }
                fix_nodes(&mut r.children, &p, in_item, ctx);
            }
            EsfNode::RecordArray(a) => {
                let p = format!("{path}/{}[]", a.name);
                a.items.retain(|it| match block_pos(it) {
                    Some(q) => !removed.contains(&block_id(it, q)),
                    None => !item_hit(it, &p, removed, table),
                });
                for it in &mut a.items {
                    fix_nodes(it, &p, true, ctx);
                }
            }
            EsfNode::U32(v) if rel != Some(1) && removed.contains(v) => {
                let site = format!("{path} #{i}");
                match table.get(&site) {
                    Some(s) if s.kind == SiteKind::Tolerated => {}
                    Some(_) => *v = 0,
                    None => *ctx.unknown.entry(site).or_default() += 1,
                }
            }
            EsfNode::U32Array(v) if v.iter().any(|x| removed.contains(x)) => {
                let even = table.get(&format!("{path} #{i}[even]"));
                let odd = table.get(&format!("{path} #{i}[odd]"));
                let tolerated = |s: Option<&Site>| s.is_some_and(|s| s.kind == SiteKind::Tolerated);
                if tolerated(even) || tolerated(odd) {
                    continue;
                }
                match (even.is_some(), odd.is_some()) {
                    (true, true) => v.retain(|x| !removed.contains(x)),
                    (true, false) | (false, true) => {
                        let at = usize::from(even.is_none());
                        let mut keep = Vec::with_capacity(v.len());
                        for pr in v.chunks(2) {
                            if pr.get(at).is_none_or(|x| !removed.contains(x)) {
                                keep.extend_from_slice(pr);
                            }
                        }
                        *v = keep;
                    }
                    (false, false) => *ctx.unknown.entry(format!("{path} #{i}[]")).or_default() += 1,
                }
            }
            _ => {}
        }
    }
}

/// Sites where a vanilla save holds a non-zero value that is not a component, so the "every value
/// is a component" rule of [`check_block`] does not apply (SAVE_COMPAT.md §31: the user's
/// `nr20_naval.save`, written by the original, holds 2383 at this site of one intention; what the
/// value means there is UNKNOWN). The removal cascade still treats the site by its table row.
const DATA_RULE_EXEMPT: [&str; 1] = ["/CAI_INTERFACE_MANAGERS[]/CAI_BDI_POOL/CAI_BDI_POOL_INTENTIONS[]/CAI_BDI_COMPONENT_PROPERTY_SET #13"];

/// The AI block's own consistency, as every save of the original has it (SAVE_COMPAT.md §10,
/// CONFIRMED on the user's original saves): component ids unique and below the next-id counter
/// (`CAI_CENTRAL_BDI_POOL` #0); every id in a component's link lists and `BLOCK_OWNS` links is a
/// component (the slot list may hold 0); every non-zero value at a non-tolerated data site of the
/// site table is a component. One line per broken rule (at most `max` of each kind).
pub fn check_block(cai: &EsfRecord, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let tree = component_tree(&cai.children);
    let mut ids = BTreeSet::new();
    for t in &tree {
        if !ids.insert(t.0) && out.len() < max {
            out.push(format!("AI block: duplicate component id {}", t.0));
        }
    }
    let next = cai.child("CAI_CENTRAL_BDI_POOL").and_then(|p| p.get_u32(0)).unwrap_or(0);
    if let Some(&m) = ids.iter().next_back()
        && m >= next
    {
        out.push(format!("AI block: component id {m} >= the next-id counter {next}"));
    }
    let table = site_table();
    let mut data_bad = Vec::new();
    let mut zero_bad = Vec::new();
    walk_values(&cai.children, "", &mut |owner, site, place, v| {
        if v != 0
            && !ids.contains(&v)
            && place != Place::Link
            && !DATA_RULE_EXEMPT.contains(&site)
            && table.get(site).is_some_and(|s| s.kind != SiteKind::Tolerated)
        {
            data_bad.push((owner, site.to_string(), v));
        }
        // A scalar reference the original never leaves at 0 (SAVE_COMPAT.md §30: it drops the
        // referring component instead; a post-load that resolves the field without a 0 test
        // would fail the load). §31: `CAI_BDIM_WAIT_HERE #2` and `CAI_BDI_RECRUIT_GENERAL #0`
        // DO hold 0 in a vanilla save (the user's auto_save of 2026-10-04 11:10), so their rows
        // say zero_seen again and the rule no longer reports them.
        if v == 0 && place == Place::Scalar && table.get(site).is_some_and(|s| !s.zero_seen) {
            zero_bad.push((owner, site.to_string()));
        }
    });
    for (o, s, v) in data_bad.iter().take(max) {
        out.push(format!("AI block: component {o} names missing component {v} at {s}"));
    }
    if data_bad.len() > max {
        out.push(format!("AI block: {} more missing data references", data_bad.len() - max));
    }
    for (o, s) in zero_bad.iter().take(max) {
        out.push(format!("AI block: component {o} holds 0 at {s}, where the original never does"));
    }
    if zero_bad.len() > max {
        out.push(format!("AI block: {} more zeros where the original never has one", zero_bad.len() - max));
    }
    let mut link_bad = Vec::new();
    check_links(&cai.children, &ids, &mut link_bad);
    for (c, v) in link_bad.iter().take(max) {
        out.push(format!("AI block: component {c} links to missing component {v}"));
    }
    if link_bad.len() > max {
        out.push(format!("AI block: {} more missing links", link_bad.len() - max));
    }
    out.extend(check_link_counts(cai, max));
    out
}

fn check_links(nodes: &[EsfNode], ids: &BTreeSet<u32>, bad: &mut Vec<(u32, u32)>) {
    for p in block_positions(nodes) {
        let me = block_id(nodes, p);
        let list = |off: usize| nodes.get(p + off).and_then(EsfNode::as_u32_array).unwrap_or_default();
        for &off in &LINK_FLAT {
            bad.extend(list(off).iter().filter(|v| !ids.contains(v)).map(|&v| (me, v)));
        }
        bad.extend(list(LINK_SLOTS).iter().filter(|&&v| v != 0 && !ids.contains(&v)).map(|&v| (me, v)));
        bad.extend(list(LINK_PAIRS).chunks(2).filter(|pr| !ids.contains(&pr[0])).map(|pr| (me, pr[0])));
        if let Some(EsfNode::RecordArray(a)) = nodes.get(p + LINK_OWNS) {
            for it in &a.items {
                bad.extend(it.iter().filter_map(EsfNode::as_u32).take(2).filter(|v| !ids.contains(v)).map(|v| (me, v)));
            }
        }
    }
    for n in nodes {
        match n {
            EsfNode::Record(r) => check_links(&r.children, ids, bad),
            EsfNode::RecordArray(a) => a.items.iter().for_each(|it| check_links(it, ids, bad)),
            _ => {}
        }
    }
}

/// Offsets of the block's link counters: incoming links of slot 0 / 1 (`LINK_PAIRS` holds one
/// pair per incoming link), outgoing links of slot 0 / 1 (`LINK_OWNS` holds one item per outgoing
/// link: {target, this component, f32 add, f32 mult, u32 slot}). CONFIRMED by `cai_audit links` on
/// every original save (exact per-slot counts).
pub const IN_SLOT: [usize; 2] = [10, 11];
/// See [`IN_SLOT`].
pub const OUT_SLOT: [usize; 2] = [15, 16];

/// The `BLOCK_OWNS` targets of every block, in list order (source component -> targets): an
/// incoming pair (source, v) of another block is resolved by the original's loader to
/// `source.OWNS[v]` (`0x00CFD790`: v >= the list length fails the load), so v is an INDEX into
/// this list (CONFIRMED: `source.OWNS[v].target == this` for every pair of every original save,
/// SAVE_COMPAT.md §30).
pub fn owns_targets(nodes: &[EsfNode], out: &mut BTreeMap<u32, Vec<u32>>) {
    for p in block_positions(nodes) {
        let me = block_id(nodes, p);
        if let Some(EsfNode::RecordArray(a)) = nodes.get(p + LINK_OWNS) {
            out.insert(me, a.items.iter().map(|it| it.first().and_then(EsfNode::as_u32).unwrap_or(0)).collect());
        }
    }
    for n in nodes {
        match n {
            EsfNode::Record(r) => owns_targets(&r.children, out),
            EsfNode::RecordArray(a) => a.items.iter().for_each(|it| owns_targets(it, out)),
            _ => {}
        }
    }
}

/// The pair index `j` after the entries at `removed_before` (sorted indexes of a source's dropped
/// `BLOCK_OWNS` entries) are gone: one less per dropped entry in front of it.
fn renumbered_pair_index(removed_before: &[u32], j: u32) -> u32 {
    j - removed_before.iter().take_while(|&&i| i < j).count() as u32
}

/// Every link of the block: (source, target, slot), from the `BLOCK_OWNS` items.
fn all_links(nodes: &[EsfNode], out: &mut Vec<(u32, u32, u32)>) {
    for p in block_positions(nodes) {
        let me = block_id(nodes, p);
        if let Some(EsfNode::RecordArray(a)) = nodes.get(p + LINK_OWNS) {
            for it in &a.items {
                let u = |i: usize| it.get(i).and_then(EsfNode::as_u32).unwrap_or(0);
                out.push((me, u(0), u(4)));
            }
        }
    }
    for n in nodes {
        match n {
            EsfNode::Record(r) => all_links(&r.children, out),
            EsfNode::RecordArray(a) => a.items.iter().for_each(|it| all_links(it, out)),
            _ => {}
        }
    }
}

/// The link counters against the links (see [`IN_SLOT`]): one line per mismatch (at most `max`).
pub fn check_link_counts(cai: &EsfRecord, max: usize) -> Vec<String> {
    let mut links = Vec::new();
    all_links(&cai.children, &mut links);
    let mut incoming: BTreeMap<u32, [u32; 2]> = BTreeMap::new();
    let mut sources: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &(s, t, slot) in &links {
        if let Some(c) = incoming.entry(t).or_default().get_mut(slot.min(1) as usize) {
            *c += 1;
        }
        sources.entry(t).or_default().push(s);
    }
    let mut owns = BTreeMap::new();
    owns_targets(&cai.children, &mut owns);
    let mut out = Vec::new();
    fn go(nodes: &[EsfNode], incoming: &BTreeMap<u32, [u32; 2]>, sources: &BTreeMap<u32, Vec<u32>>, owns: &BTreeMap<u32, Vec<u32>>, out: &mut Vec<String>) {
        for p in block_positions(nodes) {
            let me = block_id(nodes, p);
            let u = |o: usize| nodes.get(p + o).and_then(EsfNode::as_u32).unwrap_or(u32::MAX);
            let inc = incoming.get(&me).copied().unwrap_or_default();
            if u(IN_SLOT[0]) != inc[0] || u(IN_SLOT[1]) != inc[1] {
                out.push(format!("AI block: component {me} counts {}/{} incoming links, has {}/{}", u(IN_SLOT[0]), u(IN_SLOT[1]), inc[0], inc[1]));
            }
            let pairs = nodes.get(p + LINK_PAIRS).and_then(EsfNode::as_u32_array).unwrap_or_default();
            let mut want: Vec<u32> = sources.get(&me).cloned().unwrap_or_default();
            let mut have: Vec<u32> = pairs.chunks(2).map(|c| c[0]).collect();
            want.sort_unstable();
            have.sort_unstable();
            if want != have {
                out.push(format!("AI block: component {me} pair list does not match its incoming links ({} vs {})", have.len(), want.len()));
            }
            // The loader resolves a pair (source, v) to source.BLOCK_OWNS[v] and fails the load
            // when v is beyond that list (0x00CFD790); the entry names this component in every
            // original save (SAVE_COMPAT.md §30).
            for pr in pairs.chunks(2).filter(|pr| pr.len() == 2) {
                match owns.get(&pr[0]).and_then(|t| t.get(pr[1] as usize)) {
                    None => out.push(format!("AI block: component {me} pair ({}, {}) indexes beyond the source's {} outgoing links (the load fails)", pr[0], pr[1], owns.get(&pr[0]).map_or(0, Vec::len))),
                    Some(&t) if t != me => out.push(format!("AI block: component {me} pair ({}, {}) indexes the source's link to {t}", pr[0], pr[1])),
                    _ => {}
                }
            }
            if let Some(EsfNode::RecordArray(a)) = nodes.get(p + LINK_OWNS)
                && let Some(it) = a.items.iter().find(|it| it.get(1).and_then(EsfNode::as_u32) != Some(me))
            {
                out.push(format!("AI block: component {me} has an outgoing link whose second id is {:?}, not itself", it.get(1).and_then(EsfNode::as_u32)));
            }
            let (mut o0, mut o1) = (0u32, 0u32);
            if let Some(EsfNode::RecordArray(a)) = nodes.get(p + LINK_OWNS) {
                for it in &a.items {
                    if it.get(4).and_then(EsfNode::as_u32) == Some(0) {
                        o0 += 1;
                    } else {
                        o1 += 1;
                    }
                }
            }
            if u(OUT_SLOT[0]) != o0 || u(OUT_SLOT[1]) != o1 {
                out.push(format!("AI block: component {me} counts {}/{} outgoing links, has {o0}/{o1}", u(OUT_SLOT[0]), u(OUT_SLOT[1])));
            }
        }
        for n in nodes {
            match n {
                EsfNode::Record(r) => go(&r.children, incoming, sources, owns, out),
                EsfNode::RecordArray(a) => a.items.iter().for_each(|it| go(it, incoming, sources, owns, out)),
                _ => {}
            }
        }
    }
    go(&cai.children, &incoming, &sources, &owns, &mut out);
    let n = out.len();
    out.truncate(max);
    if n > max {
        out.push(format!("AI block: {} more link count mismatches", n - max));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::renumbered_pair_index;

    /// An incoming pair's second value indexes the source's `BLOCK_OWNS` list (SAVE_COMPAT.md
    /// §30): when the source loses entries 1 and 3, the pairs that pointed at 0, 2, 4, 5 point at
    /// 0, 1, 2, 3.
    #[test]
    fn pair_indexes_follow_the_removed_owns_entries() {
        let gone = [1, 3];
        assert_eq!(renumbered_pair_index(&gone, 0), 0);
        assert_eq!(renumbered_pair_index(&gone, 2), 1);
        assert_eq!(renumbered_pair_index(&gone, 4), 2);
        assert_eq!(renumbered_pair_index(&gone, 5), 3);
        assert_eq!(renumbered_pair_index(&[], 7), 7);
    }
}

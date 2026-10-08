//! Battle animation tables: which `.anim` clip a man, rider or mount plays for each action.
//!
//! Two text formats in data.pack (`analysis/units/CAVALRY.md` §4):
//!
//! `animations\animation_tables\animation_tables.txt` lists the tables:
//! ```text
//! version 1
//! animation_table rider_sabre
//! {
//!     skeleton_type   man
//!     fragment        horse_rider_base_fragment   default_equipment_display = primary_weapon
//!     fragment        sabre_horse_rider_fragment  default_equipment_display = primary_weapon
//!     mount_table     mount_horse
//! }
//! ```
//! `animations\battleconfiguration\<name>.txt` fragments map slot names to clips:
//! ```text
//! version 1
//! STAND        filename = "Animations/MEN/.../MUS_Stand.anim",
//! STAND        filename = "Animations/MEN/.../MUS_stand_alt1.anim",      (repeat = alternative)
//! WALK_1       filename = "...", blend_in_time = 0.5
//! RIDER_MOUNT_LEFT filename = "..." primary_weapon = off, ambient = on
//! STAND_TRAINED_IDLE_3 cancel                                            (drops the slot)
//! // comment
//! ```
//! CONFIRMED by parsing every shipped file (`tests/real_install.rs`).
//!
//! **A repeated slot name is a set of ALTERNATIVES, not an override.** This corrects an
//! earlier INFERRED reading of ours, which claimed "a later fragment that names a slot
//! replaces that slot" and was REFUTED by the exe's per-frame resolver `0x00E5F760`: it picks
//! the alternative with `index % count`, where `count` is how many fragments the file gave
//! that slot, so with five alternatives every sixth slot plays the first one *again*. File
//! order is preserved and is the order the exe indexes; `cancel` leaves the slot with no clips
//! at all (the exe accepts it only as the slot's own line); a sixth fragment for one slot is
//! fatal in the exe and no shipped file has one. See [`fragment_index`].
//!
//! PROVISIONAL, and deliberately *not* the exe's rule: [`AnimationTables::resolve`] composes
//! the several fragment files of one `animation_table` by letting the last file that names a
//! slot win. The exe never merges them -- each `Animations/BattleConfiguration/<name>` file
//! becomes its own `0x1E610`-byte object and a battle entity is handed exactly one of them --
//! so that composition is our own data-side convenience, kept because the shipped tables need
//! it. It is not the rule above and must not be read as one.
//!
//! The tables are keyed by
//! `unit_stats_land` #9 (`man_animation_type`: `man_musket`, `rider_sabre` ...) and
//! `battle_personalities` col 2 (`personality_drummer` ...): every value in both columns
//! names a table here (CONFIRMED by a real-install test).

use std::collections::HashMap;

use crate::pack::Vfs;

/// Where the table list lives.
pub const ANIMATION_TABLES_PATH: &str = "animations/animation_tables/animation_tables.txt";

/// Folder of the fragment files.
pub const FRAGMENT_DIR: &str = "animations/battleconfiguration";

/// One `fragment` line of a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentRef {
    pub name: String,
    /// `default_equipment_display` values (`primary_weapon`, `secondary_weapon`, `ambient`,
    /// `personal`, `defensive`). Empty when the line has none.
    pub equipment_display: Vec<String>,
}

/// One `animation_table` block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnimationTable {
    pub name: String,
    /// `man`, `horse`, `camel`, `elephant`, `cannon` ...
    pub skeleton_type: String,
    pub fragments: Vec<FragmentRef>,
    /// The mount's table (`mount_horse`) for riders.
    pub mount_table: Option<String>,
}

/// One clip line of a fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentClip {
    /// Clip path as written (mixed case, `/` or `\`); `Vfs` lookups normalise it.
    pub filename: String,
    pub blend_in_time: Option<f32>,
    /// Other `key = value` pairs on the line (`equipment_usage = sword`,
    /// `primary_weapon = off` ...).
    pub attributes: Vec<(String, String)>,
}

/// A parsed fragment file: slot name -> alternatives, in file order. A slot that is
/// cancelled maps to an empty list. The order is the exe's order and matters: the exe
/// indexes into it by [`fragment_index`], it never replaces an entry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fragment {
    pub slots: Vec<(String, Vec<FragmentClip>)>,
}

impl Fragment {
    /// Parses a fragment text file.
    pub fn parse(text: &str) -> Self {
        let mut slots: Vec<(String, Vec<FragmentClip>)> = Vec::new();
        for raw in text.lines() {
            let line = raw.split("//").next().unwrap_or("").trim();
            let Some((slot, rest)) = line.split_once(|c: char| c.is_whitespace()) else {
                continue;
            };
            if slot.eq_ignore_ascii_case("version") {
                continue;
            }
            let rest = rest.trim();
            let entry = match slots.iter().position(|(s, _)| s == slot) {
                Some(i) => i,
                None => {
                    slots.push((slot.to_owned(), Vec::new()));
                    slots.len() - 1
                }
            };
            if rest.eq_ignore_ascii_case("cancel") {
                slots[entry].1.clear();
                continue;
            }
            let Some(clip) = parse_clip(rest) else {
                continue;
            };
            slots[entry].1.push(clip);
        }
        Self { slots }
    }

    pub fn get(&self, slot: &str) -> Option<&[FragmentClip]> {
        self.slots
            .iter()
            .find(|(s, _)| s.eq_ignore_ascii_case(slot))
            .map(|(_, v)| v.as_slice())
    }

    /// The alternative of `slot` the exe plays for `index`: the entry at
    /// `index % clips.len()`, i.e. [`fragment_index`] applied to this slot's clips in file
    /// order. `None` when the slot is absent, cancelled, or the exe would be dividing by a
    /// zero count.
    ///
    /// CONFIRMED rule (`0x00E5F760`); see the module docs. `index` is the exe's dividend, and
    /// at both call sites read this round (`0x006613A0`, `0x0065FB00`) it is the *entity's*
    /// selector field `+0x1E0`, not the slot.
    pub fn fragment(&self, slot: &str, index: usize) -> Option<&FragmentClip> {
        let clips = self.get(slot)?;
        fragment_index(index, clips.len()).and_then(|i| clips.get(i))
    }

    /// The runtime [`SlotGroup`] this file gives `slot`: one handle per alternative, in
    /// file order, and the count the exe's `DIV` divides by.
    ///
    /// Handles are numbered **from 1**, because `0` is the exe's "empty" marker: the clip
    /// lookup `0x007F1730` reads `queue[i]` and falls back to `queue[0]` when that entry
    /// is 0, and every runtime reader gates on the same non-zero test. A cancelled or
    /// absent slot therefore gives [`SlotGroup::EMPTY`], which is the `has_clip` clear.
    ///
    /// This is the array the exe's resolver walks, so it is the whole of
    /// `0x00E5F760` for one fragment file: the count is `clips.len()` and the handles are
    /// 1-based positions in that list. What the exe's handles actually *point at* -- a
    /// loaded clip object -- is the one part we do not build here; see §3.
    pub fn runtime_group(&self, slot: &str) -> SlotGroup {
        let mut first = 1u32;
        for (name, clips) in &self.slots {
            let last = first + clips.len() as u32 - 1;
            if name.eq_ignore_ascii_case(slot) {
                return SlotGroup::new(&(first..=last).collect::<Vec<u32>>());
            }
            first = last + 1;
        }
        SlotGroup::EMPTY
    }

    /// The whole 864-slot load-time image for this file, ready for
    /// [`populate_runtime_table`].
    ///
    /// One [`SlotBlock`] per action slot, in slot order, holding the alternatives in file
    /// order. This is the parsed form of the `0x1E610`-byte object the exe builds before
    /// it realises the runtime array: 864 blocks of [`ACTION_SLOT_BYTES`] (five
    /// [`FRAGMENT_BYTES`] fragments plus the count at [`SLOT_BLOCK_FRAGMENT_COUNT`]).
    /// Slots this file does not mention stay zeroed, which is what gives `0x00E5F760`
    /// its group-0 fallback downstream.
    pub fn slot_image(&self) -> Vec<SlotBlock> {
        // Handles are numbered across the whole file in the order it gave the clips -- a
        // line whose slot name is not one of the 864 still consumes a number, so that
        // `handle_clip` and this agree -- and a block's `count` is how many of the five
        // the file wrote for that slot.
        let mut blocks = vec![SlotBlock::EMPTY; ACTION_SLOT_COUNT];
        let mut n = 0u32;
        for (slot_name, clips) in &self.slots {
            let slot = action_slot(slot_name);
            for _clip in clips {
                n += 1;
                // A sixth fragment for one slot is the exe's fatal "Max entries exceeded
                // for type '...'"; it is dropped here rather than allowed to overrun.
                let Some(block) = slot.and_then(|s| blocks.get_mut(s)) else {
                    continue;
                };
                if (block.count as usize) < MAX_FRAGMENTS_PER_SLOT {
                    block.handles[block.count as usize] = n;
                    block.count += 1;
                }
            }
        }
        blocks
    }

    /// The whole 864-slot runtime clip array for this file, ready for
    /// [`RuntimeClipTable::resolve`].
    ///
    /// Exactly what the exe does: [`populate_runtime_table`] over [`Self::slot_image`].
    ///
    /// Slots this file does not mention keep [`SlotGroup::EMPTY`], which is exactly what
    /// gives `0x00E5F760` its group-0 fallback: asking for a slot the file has no line
    /// for divides by group 0's count and returns one of *its* handles. Note this
    /// inverts the shipped [`has_clip`] default for the 156 slots the name table marks
    /// clear, which is the point -- `has_clip()` is the table's shipped opinion,
    /// `RuntimeClipTable::has_clip` is what this file actually filled in.
    pub fn runtime_table(&self) -> RuntimeClipTable {
        populate_runtime_table(&self.slot_image())
    }

    /// The clip a 1-based handle from [`Self::runtime_table`] points at, by counting every
    /// alternative of this file in the order it gave them. This is our stand-in for the
    /// exe's handle -> clip-object pointer.
    pub fn handle_clip(&self, handle: u32) -> Option<&FragmentClip> {
        let mut n = 0u32;
        for (_, clips) in &self.slots {
            for clip in clips {
                n += 1;
                if n == handle {
                    return Some(clip);
                }
            }
        }
        None
    }
}

fn parse_clip(rest: &str) -> Option<FragmentClip> {
    let after = rest
        .strip_prefix("filename")?
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let after = after.strip_prefix('"')?;
    let (filename, tail) = after.split_once('"')?;
    // `key = value` pairs, separated by commas or (sometimes) only by spaces:
    // `, blend_in_time = 0.25 equipment_usage = sword`.
    let spaced = tail.replace(',', " ").replace('=', " = ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut blend_in_time = None;
    let mut attributes = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens.get(i + 1) == Some(&"=") {
            let key = tokens[i];
            // No value when the next token is itself a key (followed by "=") or missing.
            let value = match tokens.get(i + 2) {
                Some(v) if *v != "=" && tokens.get(i + 3) != Some(&"=") => *v,
                _ => "",
            };
            if key.eq_ignore_ascii_case("blend_in_time") {
                blend_in_time = value.parse().ok();
            } else {
                attributes.push((key.to_owned(), value.to_owned()));
            }
            i += if value.is_empty() { 2 } else { 3 };
        } else {
            i += 1;
        }
    }
    Some(FragmentClip {
        filename: filename.to_owned(),
        blend_in_time,
        attributes,
    })
}

/// Parses `animation_tables.txt`.
pub fn parse_tables(text: &str) -> Vec<AnimationTable> {
    let mut out = Vec::new();
    let mut current: Option<AnimationTable> = None;
    for raw in text.lines() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() || line == "{" {
            continue;
        }
        if line == "}" {
            out.extend(current.take());
            continue;
        }
        let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        match key.to_ascii_lowercase().as_str() {
            "animation_table" => {
                out.extend(current.take());
                current = Some(AnimationTable {
                    name: rest.to_owned(),
                    ..Default::default()
                });
            }
            "skeleton_type" => {
                if let Some(t) = current.as_mut() {
                    t.skeleton_type = rest.to_owned();
                }
            }
            "mount_table" => {
                if let Some(t) = current.as_mut() {
                    t.mount_table = Some(rest.to_owned());
                }
            }
            "fragment" => {
                if let Some(t) = current.as_mut() {
                    let (name, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                    let equipment_display = tail
                        .split_once('=')
                        .map(|(_, v)| {
                            v.split(',')
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default();
                    t.fragments.push(FragmentRef {
                        name: name.to_owned(),
                        equipment_display,
                    });
                }
            }
            _ => {}
        }
    }
    out.extend(current);
    out
}

/// A clip chosen for a slot, with the equipment the fragment shows by default. One
/// resolved entry is one *alternative*: a slot with several of them plays one of them,
/// picked by [`fragment_index`], not the last one.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedClip {
    pub slot: String,
    pub fragment: String,
    pub clip: FragmentClip,
    pub equipment_display: Vec<String>,
}

/// All tables and fragments, read once.
#[derive(Debug, Clone, Default)]
pub struct AnimationTables {
    tables: HashMap<String, AnimationTable>,
    fragments: HashMap<String, Fragment>,
}

impl AnimationTables {
    /// Reads the table list and every fragment it names. Missing fragments are skipped
    /// (and reported by [`Self::missing_fragments`]).
    pub fn from_vfs(vfs: &Vfs) -> Result<Self, crate::pack::PackError> {
        let text = String::from_utf8_lossy(&vfs.read(ANIMATION_TABLES_PATH)?).into_owned();
        Ok(Self::from_text(&text, |name| {
            vfs.read(&format!("{FRAGMENT_DIR}/{name}.txt"))
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        }))
    }

    /// Builds from the table text and a fragment loader (by fragment name).
    pub fn from_text(tables_text: &str, mut load: impl FnMut(&str) -> Option<String>) -> Self {
        let mut tables = HashMap::new();
        let mut fragments = HashMap::new();
        for t in parse_tables(tables_text) {
            for f in &t.fragments {
                let key = f.name.to_ascii_lowercase();
                if !fragments.contains_key(&key)
                    && let Some(text) = load(&key)
                {
                    fragments.insert(key, Fragment::parse(&text));
                }
            }
            tables.insert(t.name.to_ascii_lowercase(), t);
        }
        Self { tables, fragments }
    }

    pub fn table(&self, name: &str) -> Option<&AnimationTable> {
        self.tables.get(&name.to_ascii_lowercase())
    }

    pub fn table_names(&self) -> impl Iterator<Item = &str> {
        self.tables.values().map(|t| t.name.as_str())
    }

    pub fn fragment(&self, name: &str) -> Option<&Fragment> {
        self.fragments.get(&name.to_ascii_lowercase())
    }

    /// Fragment names referenced by some table but not found.
    pub fn missing_fragments(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .tables
            .values()
            .flat_map(|t| t.fragments.iter())
            .map(|f| f.name.to_ascii_lowercase())
            .filter(|n| !self.fragments.contains_key(n))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The alternatives for `slot` in `table`: the last fragment *file* of the table that
    /// names the slot wins. PROVISIONAL -- this is our own composition of the shipped files,
    /// **not** the exe's rule; see the module docs. Inside one file the alternatives are
    /// kept in order and picked by [`fragment_index`]. Empty if no fragment file has the
    /// slot or it was cancelled.
    pub fn resolve(&self, table: &str, slot: &str) -> Vec<ResolvedClip> {
        let Some(t) = self.table(table) else {
            return Vec::new();
        };
        for f in t.fragments.iter().rev() {
            let Some(frag) = self.fragment(&f.name) else {
                continue;
            };
            if let Some(clips) = frag.get(slot) {
                return clips
                    .iter()
                    .map(|c| ResolvedClip {
                        slot: slot.to_owned(),
                        fragment: f.name.clone(),
                        clip: c.clone(),
                        equipment_display: f.equipment_display.clone(),
                    })
                    .collect();
            }
        }
        Vec::new()
    }

    /// First slot of `slots` that resolves to at least one clip.
    pub fn resolve_first(&self, table: &str, slots: &[&str]) -> Vec<ResolvedClip> {
        slots
            .iter()
            .map(|s| self.resolve(table, s))
            .find(|v| !v.is_empty())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// The exe's contract for these files.
//
// CONFIRMED by decompiling Napoleon.exe 1.3 in Ghidra (own sandbox copy); the
// addresses and the xref census are in analysis/fidelity/MIDDLEWARE_VERIFY.md §3.
// One `Animations/BattleConfiguration/<name>` file becomes one `0x1E610`-byte
// object, made and cached by `0x00E611E0` and filled by the parser `0x00E62760`.
// The object is a header plus one fixed block per action slot, five fragments
// each; the slot name is resolved through `0x00E5FA30`, a hash over the 864-name
// table in [`SLOT_NAMES`] whose entry order *is* the slot index. A name the table
// does not hold is the fatal error "Unrecognised animation type".
//
// Grammar, in the order the parser tests the keywords of one fragment:
//   `version = <int>` once, then per slot, per fragment:
//   `filename = <path>` [`blend_in_time = <float>`] [`equipment_usage = <name>`]
//   [`primary_weapon`|`secondary_weapon`|`defensive`|`ambient`|`personal` `= on|off`]
//   [`special = <name>`] `,`
// `equipment_usage` and `special` are closed sets ([`EquipmentUsage`],
// [`SpecialUsage`]); the five on/off keywords are bits 0..4 of the fragment's flag
// word. `cancel` writes 11 into a fragment's equipment field and leaves its tag at
// `0x360` so the fragment is never materialised, and the exe only accepts it as the
// slot's own line ("Cancel must be only entry in fragment for slot type"). Every other
// mistake -- an unknown keyword, a sixth fragment for one slot, a fragment with no
// filename, a filename on a cancelled fragment -- calls `TerminateProcess`, so the
// shipped files are all well formed. What the exe does with the fragments it *does*
// accept is [`fragment_index`]: they are alternatives, and a repeated slot line adds to
// the list rather than replacing what is already there.
// ---------------------------------------------------------------------------

/// Action slots per object: the `0x1E610`-byte object is this header plus
/// [`ACTION_SLOT_COUNT`] blocks of [`ACTION_SLOT_BYTES`], and the constructor
/// `0x00E50930` runs the per-slot initialiser exactly `0x360` times.
pub const ACTION_SLOT_COUNT: usize = 864;

/// Bytes of one slot block: [`MAX_FRAGMENTS_PER_SLOT`] fragments plus the u32 count
/// that lives in the last four bytes.
pub const ACTION_SLOT_BYTES: usize = 0x90;

/// Bytes of one fragment -- everything one `filename` line can say.
pub const FRAGMENT_BYTES: usize = 0x1C;

/// Byte offset of the "how many fragments does this slot have" u32 inside one
/// [`ACTION_SLOT_BYTES`] slot block -- five [`FRAGMENT_BYTES`] fragments is `0xBC`, so the
/// count is the last four bytes of the block.
///
/// CONFIRMED: the load-time object is `0x1E610` = a `0x10` header plus
/// [`ACTION_SLOT_COUNT`] blocks of [`ACTION_SLOT_BYTES`], and the per-slot initialiser
/// `0x00E50960` (13 bytes, sole caller `0x00E50930`, run exactly `0x360` times) writes
/// `*(in_ECX + 0x8C) = 0` on each block in turn.
pub const SLOT_BLOCK_FRAGMENT_COUNT: usize = 0x8C;

/// The loop that turns the load-time image into the runtime array walks **every** slot,
/// not just the filled ones, and its outer bound is `0x1E600` in `0x90` strides.
pub const LOAD_IMAGE_BYTES: usize = ACTION_SLOT_COUNT * ACTION_SLOT_BYTES;

/// Most fragments one slot may hold. A sixth is the fatal "Max entries exceeded
/// for type '...'".
pub const MAX_FRAGMENTS_PER_SLOT: usize = 5;

/// What a fragment's tag field holds while the fragment is still empty. It is also
/// the "no link" sentinel in the exe's action name table, and the parser tests for
/// it before writing a slot index, so a cancelled fragment keeps it and is skipped.
pub const EMPTY_FRAGMENT_TAG: u32 = 0x360;

/// The `blend_in_time` the parser writes into every fragment it materialises, so a
/// line that does not say it still gets a blend time.
pub const DEFAULT_BLEND_IN_TIME: f32 = 1.0;

/// Byte offsets inside one [`FRAGMENT_BYTES`]-byte fragment.
pub mod fragment_field {
    /// u32. The slot index once the fragment is materialised, else
    /// [`super::EMPTY_FRAGMENT_TAG`].
    pub const TAG: usize = 0x00;
    /// A `CA::UniString` (three words) holding the clip path.
    pub const FILENAME: usize = 0x04;
    /// f32, seconds.
    pub const BLEND_IN_TIME: usize = 0x10;
    /// u32: an [`super::EquipmentUsage`] code, or one of the two sentinels.
    pub const EQUIPMENT_USAGE: usize = 0x14;
    /// u16: the five on/off bits [`super::FLAG_PRIMARY_WEAPON`] ..
    /// [`super::FLAG_PERSONAL`] in the low five bits, then the
    /// [`super::SpecialUsage`] mask.
    pub const FLAGS: usize = 0x18;
}

// ---------------------------------------------------------------------------
// How one of several alternatives is picked (CONFIRMED, `0x00E5F760`).
//
// The exe keeps each slot's fragments in the order the file wrote them and, at runtime,
// picks one with a single unsigned `DIV`: `remainder = dividend % count`. The five
// fragments of a slot are therefore ALTERNATIVES, not a priority list -- a later
// fragment does not replace an earlier one, it takes its turn in the cycle. This
// corrects an INFERRED reading of ours that said the opposite; see the module docs.
// ---------------------------------------------------------------------------

/// Which alternative the exe plays out of a group of `count`: `index % count`, the
/// remainder of the exe's unsigned `DIV` in `0x00E5F760`.
///
/// `None` when `count` is 0. The exe has no such branch -- it would fault on the divide
/// -- so a caller must never ask; [`Fragment::fragment`] returns `None` there rather
/// than pretending a fragment exists.
///
/// **Both operands are 32-bit**, and that is load-bearing. The exact 53 bytes of
/// `0x00E5F760` (CONFIRMED, instruction level) are
/// `MOV EAX,[ESP+0x8]` / `XOR EDX,EDX` / ... / `DIV dword ptr [ESI+0x14]`: a 32-bit load
/// into `EAX` (which zeroes the upper half of `RAX`) and a `DIV dword`, so only the low
/// 32 bits of `index` and of `count` can affect the result. `index` is the entity's
/// `+0x1E0` selector, a dword field, so in the exe it is always below `2^32`; this
/// wrapper truncates so a Rust caller cannot silently differ by passing a 64-bit value.
/// Use [`fragment_index_u32`] when the inputs are already 32-bit.
///
/// Note which argument is which. At the three call sites read for this rule
/// (`0x006613A0`, `0x0065FB00` and `0x0067A050`, all CONFIRMED) the call is
/// `0x00E5F760(slot, entity + 0x1E0)`: the *group* being indexed is the action slot and
/// the *dividend* is the entity's `+0x1E0` selector, its equipment/weapon code. Both are
/// passed as plain ints and the callee cannot tell them apart, so the same
/// `index % count` shape holds whichever way round a caller feeds them.
pub fn fragment_index(index: usize, count: usize) -> Option<usize> {
    fragment_index_u32(index as u32, count as u32).map(|i| i as usize)
}

/// [`fragment_index`] in the exe's own widths: a 32-bit dividend and a 32-bit divisor,
/// one unsigned `DIV`.
pub fn fragment_index_u32(index: u32, count: u32) -> Option<u32> {
    if count == 0 {
        return None;
    }
    Some(index % count)
}

/// [`fragment_index`] with `0x00E5F760`'s group-0 fallback folded in: when the slot's own
/// group holds no fragments (`slot_count == 0`, the "has a clip" predicate clear) the exe
/// divides by **group 0's** count and reads group 0's handles instead, so a slot that is
/// not filled for this equipment still plays the base fragment.
///
/// `None` only when both counts are 0, which is the divide the exe does not survive.
pub fn fragment_index_with_fallback(
    slot_count: usize,
    group0_count: usize,
    index: usize,
) -> Option<usize> {
    fragment_index(
        index,
        if slot_count == 0 {
            group0_count
        } else {
            slot_count
        },
    )
}

/// One [`DESCRIPTOR_BYTES`]-byte element of the **runtime** clip array the exe's resolver
/// reads at `*(this + 0x2C)`: the five 32-bit clip handles in the file's order, and the
/// count that the `DIV` divides by.
///
/// CONFIRMED: `0x18` = 24 = six dwords = **five clip handles plus the count**, and the
/// handles are 32-bit *pointers* -- two independent per-frame callers dereference the
/// resolver's result (`0x0065FB00` reads `*(handle + 0x44)`, `0x0067A050` reads
/// `*(short *)(handle + 0x50)`), and this is a 32-bit exe, so a dword *is* a pointer.
///
/// This is the exe's **runtime** copy of the shipped name table's group, not the shipped
/// table itself: the static descriptor at `0x013AEBE0` has a *name* where the two lowest
/// dwords of a runtime group are handles, and the count field is the same `+0x14`.
/// [`crate::battle_animation::has_clip`] tests the shipped default of that field;
/// [`RuntimeClipTable::has_clip`] tests one entity's filled-in copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SlotGroup {
    /// The five clip handles, in the order the fragment file gave them. Only the first
    /// `count` are live; the rest are the exe's zero fill.
    pub handles: [u32; MAX_FRAGMENTS_PER_SLOT],
    /// How many of `handles` this slot holds. `0` is both "no clip" (`has_clip` clear)
    /// and the trigger for `0x00E5F760`'s group-0 fallback.
    pub count: u32,
}

impl SlotGroup {
    /// A group with no clips: the "has a clip" predicate clear.
    pub const EMPTY: Self = Self {
        handles: [0; MAX_FRAGMENTS_PER_SLOT],
        count: 0,
    };

    /// A group holding `handles`, in order, as many as were given (a sixth would be the
    /// exe's fatal "Max entries exceeded for type '...'"). Fewer than five leaves the
    /// rest zero, which is what the exe leaves too.
    pub fn new(handles: &[u32]) -> Self {
        let mut group = Self::EMPTY;
        for (dst, src) in group.handles.iter_mut().zip(handles) {
            *dst = *src;
        }
        // Clamped, because a sixth fragment for one slot is the exe's fatal "Max entries
        // exceeded for type '...'" -- the group cannot hold it, so the extra line is
        // dropped rather than allowed to overstate the count.
        group.count = (handles.len() as u32).min(MAX_FRAGMENTS_PER_SLOT as u32);
        group
    }

    /// `0x00E5F760`'s "has a clip" predicate on this group: the count, non-zero.
    pub fn has_clip(&self) -> bool {
        self.count != 0
    }

    /// The clip this group points at for `selector`: the exe's
    /// `selector % count`, with the group-0 fallback when this group is empty.
    ///
    /// `None` for the divide the exe does not survive (group 0 empty too) and for a count
    /// larger than the five handles a group can hold -- which cannot come from a
    /// well-formed file, and which the exe would read past the end of the group.
    pub fn resolve(&self, group0: &Self, selector: u32) -> Option<u32> {
        let (handles, count) = if self.count == 0 {
            (group0.handles, group0.count)
        } else {
            (self.handles, self.count)
        };
        let i = fragment_index_u32(selector, count)?;
        if i as usize >= MAX_FRAGMENTS_PER_SLOT {
            return None;
        }
        Some(handles[i as usize])
    }
}

/// The runtime clip array an entity reaches as `**(this + 0x218) + 0x2C`: one
/// [`SlotGroup`] per action slot, indexed by the slot.
///
/// This is the piece round 11 could not find the writer for; the shape is CONFIRMED
/// (see §3) and [`crate::battle_animation::Fragment::runtime_table`] builds one straight
/// out of a fragment file, so the resolver can be exercised end to end without the exe.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeClipTable {
    groups: Vec<SlotGroup>,
}

impl Default for RuntimeClipTable {
    /// A full 864-slot table with every group empty: the state an entity starts in, and
    /// the size the exe's resolver indexes against without a bounds check.
    fn default() -> Self {
        Self {
            groups: vec![SlotGroup::EMPTY; ACTION_SLOT_COUNT],
        }
    }
}

impl RuntimeClipTable {
    /// An empty table with one group per action slot.
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty table with `slots` groups, for tests that do not want all 864.
    pub fn with_slots(slots: usize) -> Self {
        Self {
            groups: vec![SlotGroup::EMPTY; slots],
        }
    }

    pub fn len(&self) -> usize {
        self.groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// The group of `slot`, or `None` past the end of the table. The exe has no such
    /// branch: `0x00E5F760` would read out of bounds, which is why every caller of it in
    /// the exe is gated on `has_clip` first.
    pub fn group(&self, slot: usize) -> Option<&SlotGroup> {
        self.groups.get(slot)
    }

    /// Fills `slot`'s group. Ignores a slot past the end, or a sixth handle.
    pub fn set_group(&mut self, slot: usize, group: SlotGroup) {
        if let Some(dst) = self.groups.get_mut(slot) {
            *dst = group;
        }
    }

    /// The "does this entity have a clip in this slot" predicate the five runtime readers
    /// test, on *this* table rather than on the shipped default.
    pub fn has_clip(&self, slot: usize) -> bool {
        self.group(slot).is_some_and(SlotGroup::has_clip)
    }

    /// The exe's per-frame resolver, `0x00E5F760(slot, selector)`: the clip handle for
    /// `slot`, chosen by `selector % count` over that slot's alternatives, falling back
    /// to group 0's when the slot has none.
    ///
    /// CONFIRMED. `selector` is the entity's `+0x1E0` equipment/weapon code. Note this
    /// returns the *handle*, i.e. a value from the runtime array; in the exe that value
    /// is a clip object pointer, here it is whatever the table was built with.
    pub fn resolve(&self, slot: usize, selector: u32) -> Option<u32> {
        let group = self.group(slot)?;
        let group0 = self.group(0)?;
        group.resolve(group0, selector)
    }
}

/// `equipment_usage` is a closed set of ten names; anything else is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum EquipmentUsage {
    None = 0,
    Rifle = 1,
    RifleButt = 2,
    RifleBayonet = 3,
    Sword = 4,
    Axe = 5,
    Pike = 6,
    Lance = 7,
    Longsword = 8,
    Shield = 9,
}

/// What the exe's `equipment_usage` reader returns for a name it does not know, and
/// the value it pre-fills a fresh fragment with. Ten is therefore "no usage".
pub const EQUIPMENT_USAGE_UNRECOGNISED: u32 = 10;

/// What the `cancel` keyword writes into a fragment's equipment field instead of a
/// name. It is a marker, not an eleventh usage.
pub const EQUIPMENT_USAGE_CANCELLED: u32 = 11;

impl EquipmentUsage {
    pub const ALL: [Self; 10] = [
        Self::None,
        Self::Rifle,
        Self::RifleButt,
        Self::RifleBayonet,
        Self::Sword,
        Self::Axe,
        Self::Pike,
        Self::Lance,
        Self::Longsword,
        Self::Shield,
    ];

    /// The name as it is written in a file.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Rifle => "rifle",
            Self::RifleButt => "rifle_butt",
            Self::RifleBayonet => "rifle_bayonet",
            Self::Sword => "sword",
            Self::Axe => "axe",
            Self::Pike => "pike",
            Self::Lance => "lance",
            Self::Longsword => "longsword",
            Self::Shield => "shield",
        }
    }

    /// The name as the exe spells it, which is what appears in a file.
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// The reader is case sensitive, like the exe's string compare.
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.keyword() == keyword)
    }
}

/// `special` is a closed set of ten names, each one bit of the fragment's flag word.
/// They are OR-ed, so one fragment may name several.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SpecialUsage {
    MusketRamrod = 0x0020,
    CannonRamrod = 0x0040,
    ArrowMan = 0x0080,
    ArrowRider = 0x0100,
    Axe = 0x0200,
    CannonBall = 0x0400,
    Rocket = 0x0800,
    WoodenStake = 0x1000,
    Grenade = 0x2000,
    Bayonet = 0x4000,
}

/// What the exe's `special` reader returns for a name it does not know. It lands in
/// the flag word like any other value, so a fragment that ORs it in is marked
/// unusable rather than silently accepted.
pub const SPECIAL_USAGE_UNRECOGNISED: u16 = 0x8000;

impl SpecialUsage {
    pub const ALL: [Self; 10] = [
        Self::MusketRamrod,
        Self::CannonRamrod,
        Self::ArrowMan,
        Self::ArrowRider,
        Self::Axe,
        Self::CannonBall,
        Self::Rocket,
        Self::WoodenStake,
        Self::Grenade,
        Self::Bayonet,
    ];

    /// The name as it is written in a file.
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::MusketRamrod => "musket_ramrod",
            Self::CannonRamrod => "cannon_ramrod",
            Self::ArrowMan => "arrow_man",
            Self::ArrowRider => "arrow_rider",
            Self::Axe => "axe",
            Self::CannonBall => "cannon_ball",
            Self::Rocket => "rocket",
            Self::WoodenStake => "wooden_stake",
            Self::Grenade => "grenade",
            Self::Bayonet => "bayonet",
        }
    }

    /// This usage's bit in the fragment's flag word.
    pub const fn bit(self) -> u16 {
        self as u16
    }

    /// The reader is case sensitive, like the exe's string compare.
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.keyword() == keyword)
    }
}

/// `primary_weapon = on` sets bit 0 of the fragment's flag word, and so on: the five
/// on/off keywords are the low five bits, above them sits the [`SpecialUsage`] mask.
/// The parser clears the bit for `= off` and sets it for `= on`, and only accepts
/// those two words.
pub const FLAG_PRIMARY_WEAPON: u16 = 1 << 0;
pub const FLAG_SECONDARY_WEAPON: u16 = 1 << 1;
pub const FLAG_DEFENSIVE: u16 = 1 << 2;
pub const FLAG_AMBIENT: u16 = 1 << 3;
pub const FLAG_PERSONAL: u16 = 1 << 4;

/// The five on/off keywords with the bit each one sets, in the order the parser
/// tests them.
pub const DISPLAY_FLAGS: [(u16, &str); 5] = [
    (FLAG_PRIMARY_WEAPON, "primary_weapon"),
    (FLAG_SECONDARY_WEAPON, "secondary_weapon"),
    (FLAG_DEFENSIVE, "defensive"),
    (FLAG_AMBIENT, "ambient"),
    (FLAG_PERSONAL, "personal"),
];

/// The words the five on/off keywords accept. Anything else is the fatal
/// "Expecting 'on' or 'off'".
pub const ON_OFF: [&str; 2] = ["on", "off"];

// ---------------------------------------------------------------------------
// The per-slot "has a clip" predicate, descriptor +0x14 (CONFIRMED).
//
// Two places in the exe are 0x18 bytes per action slot and both put a count at
// +0x14, and every one of the five runtime readers tests it as a *predicate*
// ("does this entity have a clip in this slot") rather than as an index:
//
//  * the entity's runtime array `**(entity + 0x218) + 0x2C`, whose first five dwords
//    of each 0x18 group hold clip handles -- read by `0x00E5F760` (the count is the
//    divisor), `0x006613A0`, `0x006618D0`, `0x0065FB00` and `0x0064D6F0`;
//  * the static action-name table at `0x013AEBE0`, 864 descriptors of 0x18 bytes,
//    whose +0x00 is the slot index and +0x04 the name pointer. Its `+0x08`, `+0x0C`
//    and `+0x10` are read by the loader (see [`descriptor_field`]) but decoded nowhere.
//
// The *shipped* values of the second table are transcribed here as
// [`SLOTS_WITHOUT_CLIP`]: +0x14 is 0 or 1 and nothing else, 1 for 708 slots and 0 for
// 156 (measured out of the exe by reading all 864 dwords, not transcribed by eye).
// -----------------------------------------------------------------------------

/// Bytes of one 0x18-stride slot group / name-table descriptor.
pub const DESCRIPTOR_BYTES: usize = 0x18;

/// Byte offsets inside one [`DESCRIPTOR_BYTES`]-byte descriptor.
pub mod descriptor_field {
    /// u32: the slot index. For all 864 entries of the shipped table `entry[i] == i`, so
    /// the table order *is* the index.
    pub const INDEX: usize = 0x00;
    /// A `char*`: the `ACTION_<name>` suffix, the value the resolver `0x00E5FA30` looks
    /// up with `strcmp` (so case sensitive).
    pub const NAME: usize = 0x04;
    /// u32: a category code, 0..27, 32 possible values. **UNKNOWN**: the names, and what
    /// the code selects. Read by the population loop `0x00E60260`, which compares it
    /// against `0x0F` and takes a different path when it matches.
    ///
    /// Round 10 called this field unread, from a census of `xref:` on the table's *base*
    /// address. That was the wrong measurement: the loader reads the descriptors through
    /// the labelled bases `0x013AEBE0 + 8` and `0x013AEBE0 + 0x10`, which a census of the
    /// base does not report. Corrected in §3 round 13.
    pub const CATEGORY: usize = 0x08;
    /// u32: an INFERRED link index, `864` = "none". Read by the record constructor
    /// `0x00E506D0`, which copies all 864 of these into the record's own `+0x3C` array, and
    /// by `0x00E61020`, which resets the linked slot's first clip through it.
    /// CONFIRMED read; the meaning is INFERRED.
    pub const LINK_A: usize = 0x0C;
    /// u32: a second INFERRED link index, `864` = "none". Read by the population loop,
    /// which records which slot actually supplied the clip when this is in range.
    pub const LINK_B: usize = 0x10;
    /// u32: the per-slot "has a clip" predicate, 0 or 1. See [`super::has_clip`].
    pub const HAS_CLIP: usize = 0x14;
}

/// Slots whose descriptor `+0x14` is set in the shipped name table, i.e. the slots the
/// engine treats as having a clip. A strong regression check on the whole table decode:
/// both census numbers are read out of the exe.
pub const SLOTS_WITH_CLIP: usize = 708;

/// Slots whose descriptor `+0x14` is clear in the shipped name table. The census
/// counterpart of [`SLOTS_WITH_CLIP`].
pub const SLOTS_WITHOUT_CLIP_COUNT: usize = 156;

/// Slots whose descriptor `+0x14` is clear: inclusive ranges, sorted and disjoint, 156
/// slots in 32 runs. What they are, by name: the transitions into locomotion and the
/// locomotion slots themselves (`10..46` = `WALK_1` .. `TURN_RIGHT_180`), the
/// `TURN_*_TO_WALK_*` turns `0x006618D0` scans (`53..58`), the trained and stand-for-stoke
/// gait families, the stand-no-weapon transitions, the `*_TO_WALK_*` rider turns, the
/// engine limber, and a set of one-offs: `SNEAK`, `STOKE`, `NAVAL_STOKE`, `MORTAR_STOKE`,
/// `CHARGE`, `RETURN_TO_FIRING_POSITION`, `RELOAD_1`, `KNEEL_RELOAD`,
/// `STAND_IDLE_11_TELESCOPE`, the crouch-to-walk turns, `FALL_FLAILING1`, `GRAPPLE_SWING`,
/// `GRAPPLE_PULL`, `KNOCKED_FLYING_1` and the `CLIMB_*` family (`787..804`).
#[rustfmt::skip]
pub const SLOTS_WITHOUT_CLIP: [(u32, u32); 32] = [
    (3, 3), (10, 46), (53, 58), (62, 75), (82, 87), (96, 99), (112, 115), (125, 125),
    (131, 131), (134, 135), (137, 137), (149, 149), (159, 163), (170, 175), (185, 190),
    (197, 202), (367, 368), (370, 371), (386, 386), (391, 391), (395, 395), (465, 470),
    (581, 582), (603, 603), (605, 608), (614, 619), (638, 641), (646, 649), (653, 653),
    (655, 655), (787, 804), (815, 815),
];

/// Does the shipped table say this action slot has a clip? Descriptor `+0x14` of the
/// [`SLOT_NAMES`] entry, and the predicate the exe's five runtime readers test.
///
/// CONFIRMED as the shipped data (708 set / 156 clear over the 864 slots, read out of
/// `0x013AEBE0`). PROVISIONAL as an engine rule: the exe tests the *runtime* copy of the
/// table, which an entity's loader fills in from the fragment files it actually loaded, so
/// this is the shipped default and not necessarily one entity's answer.
pub fn has_clip(slot: usize) -> bool {
    if slot >= ACTION_SLOT_COUNT {
        return false;
    }
    let s = slot as u32;
    !SLOTS_WITHOUT_CLIP
        .binary_search_by(|&(lo, hi)| {
            if s < lo {
                std::cmp::Ordering::Greater
            } else if s > hi {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

// ---------------------------------------------------------------------------
// Building the runtime array (CONFIRMED, `0x00E60260`, instruction level).
//
// This is the store round 11 could not find and round 12 called "one decompile away". The
// writer is not `0x00E52480` (which only makes the 0xA8 record and its bones) but
// `0x00E60260`, which `0x00E52480` calls last; it is also the callee of all four sibling
// record builders, so it is the one population loop. Its exact bytes, per slot:
//
//     00e605c4  MOV ESI,dword ptr [EBX + 0x2c]   ; the 864-group array
//     00e605c7  ADD ESI,EDI                      ; + slot * 0x18
//     00e605c9  MOV EDX,dword ptr [ESI + 0x14]   ; the group's count
//     00e605cc  LEA ECX,[EDX + 0x1]
//     00e605cf  MOV dword ptr [ESI + 0x14],ECX   ; count = count + 1
//     00e605d6  MOV dword ptr [ESI + EDX*0x4],EAX ; handles[old count] = the loaded clip
//
// with the inner loop walking the fragments at `0x1C` and the outer loop
// `ADD EAX,0x90` / `ADD EDI,0x18` bounded by `CMP EAX,0x1e600` -- `0x1E600 / 0x90 = 864`
// slots, `0x18` per runtime group. `EAX` is the return of `0x00E60660`, the clip loader.
// The count field it increments was zeroed for all 864 groups by the record's constructor
// `0x00E506D0`, whose own loop is `*(dword *)(base + 0x14) = 0; base += 0x18` 864 times.
// So the rule is exactly *append, in file order*: no gaps, no dedup, no sort.
// ---------------------------------------------------------------------------

/// One slot block of the load-time image, as `0x00E60260` reads it: the alternatives in
/// the order the file wrote them, and the count that lives at
/// [`SLOT_BLOCK_FRAGMENT_COUNT`] of the [`ACTION_SLOT_BYTES`]-byte block.
///
/// `count` is separate from `handles` on purpose. The loop iterates `count` times, not
/// `handles.len()`, and a well-formed file cannot make them differ; a count above
/// [`MAX_FRAGMENTS_PER_SLOT`] cannot come from one either, and is what
/// [`populate_slot_group`] refuses rather than the overrun the exe would take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotBlock {
    /// The alternatives' clip handles, in file order.
    pub handles: [u32; MAX_FRAGMENTS_PER_SLOT],
    /// How many of `handles` the block actually holds.
    pub count: u32,
}

impl SlotBlock {
    /// A zeroed block: no alternatives, the count the constructor writes.
    pub const EMPTY: Self = Self {
        handles: [0; MAX_FRAGMENTS_PER_SLOT],
        count: 0,
    };

    /// A block holding `handles`, in order. A sixth handle is dropped and the count is
    /// clamped, which is the exe's fatal "Max entries exceeded for type '...'" avoided.
    pub fn new(handles: &[u32]) -> Self {
        let mut block = Self::EMPTY;
        for (dst, src) in block.handles.iter_mut().zip(handles) {
            *dst = *src;
        }
        block.count = (handles.len() as u32).min(MAX_FRAGMENTS_PER_SLOT as u32);
        block
    }
}

impl Default for SlotBlock {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// The store `0x00E60260` performs for one slot: copy the block's `count` handles into the
/// group in order and set the group's count to the same number.
///
/// This is [`SlotGroup::new`] over the block's first `count` handles, with one difference
/// the loop makes load-bearing: handles *past* `count` are ignored even when non-zero,
/// because the loop's bound is the count in the block and not the array it reads. A count
/// of 0 gives [`SlotGroup::EMPTY`] -- the "no clip" case, and what makes every unfilled
/// slot in the 864 take `0x00E5F760`'s group-0 fallback.
pub fn populate_slot_group(block: &SlotBlock) -> SlotGroup {
    let n = (block.count as usize).min(MAX_FRAGMENTS_PER_SLOT);
    let mut group = SlotGroup::EMPTY;
    group.handles[..n].copy_from_slice(&block.handles[..n]);
    group.count = n as u32;
    group
}

/// The whole of `0x00E60260`: the runtime clip array the per-frame resolver indexes.
///
/// Walks **every** slot in `image`, in order, exactly as the exe does -- including the
/// ones with no alternatives, whose groups stay empty. `image` shorter than
/// [`ACTION_SLOT_COUNT`] leaves the remaining groups empty; longer is truncated, which the
/// exe would instead over-read by (`0x18` per slot, no bound).
pub fn populate_runtime_table(image: &[SlotBlock]) -> RuntimeClipTable {
    let mut table = RuntimeClipTable::new();
    for (slot, block) in image.iter().enumerate().take(ACTION_SLOT_COUNT) {
        table.set_group(slot, populate_slot_group(block));
    }
    table
}

// ---------------------------------------------------------------------------
// The stance / state machine around the clip table (CONFIRMED, see §3 round 12).
//
// The per-frame battle-entity update does not pick an action slot by name. It picks a
// *state*, and each state owns a contiguous range of action slots whose clips it cycles
// through. The ranges are static data in the exe: 46 eight-byte `{first_slot, count}`
// pairs at `0x014520B0`, each naming one animation cycle family (an idle loop, an
// attack cycle, a family of deaths, a combat cycle, ...).
//
// The state chooser `0x00817BB0` picks a *transition clip* for a change of state by
// indexing `from * 73 + to`, so the state space is 73 wide. Only 10 of those 73 states
// are given a clip range by the stance driver `0x006631A0`'s switch, and they are the ten
// below. The other 63 states queue no clips at all -- they still record the new state and
// still ask `0x00817BB0` for a transition clip. Their names are UNKNOWN and are not in
// the exe as strings; see §3.
// ---------------------------------------------------------------------------

/// How many states the transition table is wide: `0x00817BB0` indexes
/// `from * 73 + to`, so both arguments run `0..73` and the array holds 73 x 73 blocks.
pub const STATE_COUNT: u32 = 73;

/// Bytes of one `(from, to)` block of the transition table: the u32 count followed by
/// [`TRANSITION_ENTRIES_PER_BLOCK`] eight-byte `{weight, action_slot}` entries.
pub const TRANSITION_BLOCK_BYTES: usize = 0x1C;

/// How many weighted entries one `(from, to)` block can hold: `(0x1C - 4) / 8`.
pub const TRANSITION_ENTRIES_PER_BLOCK: usize = 3;

/// The "no transition" entry `0x00817BB0` returns when a block's count is 0: the static
/// 8-byte pair at `0x01454398` = `{weight 0, slot EMPTY_FRAGMENT_TAG}`. `0x006631A0`
/// tests the slot against [`EMPTY_FRAGMENT_TAG`] and the weight's low byte against 0,
/// which is how it skips a state change that has nothing to play.
pub const NO_TRANSITION_SLOT: u32 = EMPTY_FRAGMENT_TAG;

/// The transition-table block of `from` -> `to`, `None` if either state is outside
/// `0..73` (the exe would index out of the array; it has no bounds check).
pub fn transition_index(from: u32, to: u32) -> Option<usize> {
    if from >= STATE_COUNT || to >= STATE_COUNT {
        return None;
    }
    Some((from * STATE_COUNT + to) as usize)
}

/// Which of a block's `count` weighted entries the exe picks, CONFIRMED from the exact
/// bytes of `0x00817BB0`: `seed = seed * 0x343FD + 0x269EC3` (the MSVC `rand` LCG),
/// then `((seed >> 16) * count) / 0xFFFF`, clamped down to `count - 1` by a `CMOVNC`.
///
/// `seed` is the caller's LCG state -- at the one call site that is the entity's
/// `*(this + 0x28) + 8 + 0x50`. `None` for `count == 0`, the case the exe answers with
/// [`NO_TRANSITION_SLOT`] instead of dividing.
pub fn pick_weighted(count: u32, seed: &mut u32) -> Option<usize> {
    if count == 0 {
        return None;
    }
    *seed = seed.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
    let pick = ((*seed >> 16) as u64 * count as u64) / 0xFFFF;
    Some(pick.min(count as u64 - 1) as usize)
}

/// Entries in the static cycle-family table at `0x014520B0`, read out of the exe: 46
/// eight-byte pairs, the 47th onward being floats (the blend tables at `0x01452190`).
pub const STANCE_TABLE_ENTRIES: usize = 46;

/// The ten states the stance driver `0x006631A0` gives a clip range to, as
/// `(state, first_slot, count)` -- the eight standing idle loops and the two wounded
/// ones. CONFIRMED: the switch reads `DAT_014520B0 + entry * 8` for states 0..7 and
/// `DAT_014521D0` / `DAT_014521D8` for 60 and 61, and the table was read in full.
pub const STANCE_CLIP_RANGES: [(u32, u32, u32); 10] = [
    (0, 139, 11), // STAND_IDLE_1 .. STAND_IDLE_11_TELESCOPE
    (1, 210, 5),  // STAND_ALT_1_IDLE_1 .. _5
    (2, 217, 5),  // STAND_ALT_2_IDLE_1 .. _5
    (3, 224, 5),  // STAND_ALT_3_IDLE_1 .. _5
    (4, 150, 6),  // STAND_TRAINED_IDLE_1 .. _6
    (5, 626, 5),  // CROUCH_IDLE_1 .. _5
    (6, 176, 5),  // STAND_FOR_STOKE_IDLE_1 .. _5
    (7, 203, 4),  // STAND_NO_WEAPON_IDLE_1 .. _4
    (60, 817, 5), // WOUNDED_FRONT_IDLE .. WOUNDED_FRONT_CRAWL
    (61, 822, 4), // WOUNDED_BACK_IDLE .. WOUNDED_BACK_CRAWL
];

/// The clip range of `state` as `(first_slot, count)`, or `None` for the 63 states that
/// have none. See [`STANCE_CLIP_RANGES`].
pub fn stance_clip_range(state: u32) -> Option<(usize, usize)> {
    STANCE_CLIP_RANGES
        .iter()
        .find(|(s, _, _)| *s == state)
        .map(|(_, first, count)| (*first as usize, *count as usize))
}

// ---------------------------------------------------------------------------
// The pose code that drives the state (CONFIRMED, `0x00663730`).
//
// Round 12 concluded that the state ids are nameable only if the 13 call sites of
// `0x006631A0` pass a usable enum. Read, they do not: only six of them pass a constant,
// and four pass a computed value. What *is* in the exe is the state **writer**, and it is
// a pure function of one 73-wide field.
//
// `0x00663730` (715 bytes, 8 callers, **no callees**) is exactly
// `switch (entity + 0x1D8) { ... entity + 0x1B8 = <immediate>; }`: 73 cases, one per
// value `0x00`..`0x48`, each storing one literal. `+0x1D8` is therefore the per-frame pose
// / activity code and `+0x1B8` is the animation state it selects -- and the map reaches
// **63 distinct states**, which is round 12's "63 unnamed states" counted a second way.
//
// The states stay UNNAMED: the exe has no string or enum table for them, and the map only
// says which pose picks which state. The one thing it does give is the pose behind each of
// the ten named families -- see [`STANCE_STATE_POSES`].
// ---------------------------------------------------------------------------

/// Width of `entity + 0x1D8`, the pose / activity code `0x00663730` switches on: its cases
/// run `0x00` .. `0x48`, i.e. [`STATE_COUNT`] values, which is also the width of the
/// transition table `0x00817BB0` indexes.
pub const POSE_COUNT: u32 = STATE_COUNT;

/// The value [`POSE_STATE`] holds for a pose that leaves the state alone. Every case in
/// `0x00663730` stores, so this never occurs in the shipped function; it exists so the
/// table can be a plain array and [`pose_state`] can still answer `Option`.
pub const POSE_LEAVES_STATE: u32 = u32::MAX;

/// `0x00663730` in full: the state stored at `entity + 0x1B8` for each pose code at
/// `entity + 0x1D8`, transcribed from its 73-case switch (not derived). Index = the pose,
/// value = the state; [`POSE_LEAVES_STATE`] would mean "unchanged".
///
/// Six poses share their state with the next three (`0x28`..`0x2A` all give 63, and so on
/// for `0x2B`..`0x2D`, `0x2E`..`0x30`, `0x31`..`0x33`, `0x3C`..`0x3D`, `0x44`..`0x45`), which
/// is why the 73 poses name only 63 states. Note the values run to 82 while
/// [`STATE_COUNT`] is 73: `0x006631A0` passes them to `0x00817BB0` with no range check at
/// all, so whether the states above 72 are ever reached in play is UNKNOWN -- see §3.
#[rustfmt::skip]
pub const POSE_STATE: [u32; POSE_COUNT as usize] = [
     0,  5,  6,  7,  1,  2,  3,  4, 14, 15, 16, 17, 19, 18, 25, 26, 27, 28,
    30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 53, 54, 55, 56, 57, 58,
    59, 60, 61, 62, 63, 63, 63, 64, 64, 64, 65, 65, 65, 66, 66, 66, 67, 68,
    69, 70, 71, 72, 73, 74, 75, 75, 76, 77, 78, 79, 80, 81, 44, 44, 48, 49,
    82,
];

/// The state `0x00663730` stores for `pose`, or `None` for a pose outside `0..73`.
///
/// CONFIRMED for every case of the switch. `None` is ours: the exe indexes nothing here,
/// it simply falls out of the switch and returns with the state untouched.
pub fn pose_state(pose: u32) -> Option<u32> {
    POSE_STATE
        .get(pose as usize)
        .copied()
        .filter(|&s| s != POSE_LEAVES_STATE)
}

/// Which poses give the eight standing idle states, in [`STANCE_CLIP_RANGES`] order.
///
/// CONFIRMED by composing the two tables: [`STANCE_CLIP_RANGES`] names the state and
/// `0x00663730` names the pose, and they agree for all eight. The poses are `0x00..0x07`
/// in a permuted order -- the states `0,1,2,3,4,5,6,7` come from the poses
/// `0,4,5,6,7,1,2,3`, so the idle families are not in pose order. Wounded states 60 and 61
/// come from poses 37 and 38.
pub const STANCE_STATE_POSES: [u32; 8] = [0, 4, 5, 6, 7, 1, 2, 3];

/// Pose codes behind the two wounded states, which the stance driver also names.
pub const WOUNDED_STATE_POSES: [u32; 2] = [37, 38];

/// The action slot index the exe's resolver `0x00E5FA30` gives `name`: its position in
/// [`SLOT_NAMES`], or `None` for a name the table does not hold (the exe's fatal
/// "Unrecognised animation type").
///
/// The lookup is `strcmp`, so it is **case sensitive**; [`Fragment::get`] folds case
/// instead, which is a reader-side convenience and not the exe's rule.
pub fn action_slot(name: &str) -> Option<usize> {
    // The exe compares with strcmp, so the lookup is case sensitive. Everything else
    // in this module folds case, which is why this one does not.
    SLOT_NAMES.iter().position(|n| *n == name)
}

/// Every action slot name the exe knows, in slot order: `SLOT_NAMES[i]` is the name
/// that resolves to slot `i`, and no name appears twice. Transcribed from the exe's
/// table at `0x013AEBE0`, stride `0x18`, entry `+0x04` (CONFIRMED, see §3).
///
/// The lookup is `strcmp`, so it is **case sensitive**; [`Fragment::get`] folds case
/// instead, which is a reader-side convenience and not the exe's rule.
#[rustfmt::skip]
pub const SLOT_NAMES: [&str; ACTION_SLOT_COUNT] = [
    "MISSING_ANIM", "STAND", "STAND_TO_STAND_TRAINED", "STAND_TO_WALK",
    "STAND_TO_COMBAT_READY", "STAND_TO_PRONE", "STAND_TO_STAND_FOR_STOKE", "STAND_TO_STAND_ALT_1",
    "STAND_TO_STAND_ALT_2", "STAND_TO_STAND_ALT_3", "WALK_1", "WALK_2",
    "WALK_3", "WALK_4", "WALK_5", "WALK_TO_STAND",
    "WALK_TO_RUN", "WALK_TO_TROT", "WALK_JUMP", "RUN_1",
    "RUN_2", "RUN_3", "RUN_4", "RUN_5",
    "RUN_TO_STAND", "RUN_TO_WALK", "RUN_TO_GALLOP", "RUN_JUMP",
    "TROT", "TROT_TO_STAND", "TROT_TO_WALK", "TROT_TO_CANTER",
    "TROT_JUMP", "CANTER", "CANTER_TO_STAND", "CANTER_TO_TROT",
    "CANTER_TO_GALLOP", "CANTER_JUMP", "GALLOP", "GALLOP_TO_STAND",
    "GALLOP_TO_RUN", "GALLOP_TO_CANTER", "GALLOP_JUMP", "STEP_FORWARD",
    "STEP_BACKWARD", "STEP_LEFT", "STEP_RIGHT", "TURN_LEFT_45",
    "TURN_RIGHT_45", "TURN_LEFT_90", "TURN_RIGHT_90", "TURN_LEFT_180",
    "TURN_RIGHT_180", "TURN_LEFT_TO_WALK_45", "TURN_RIGHT_TO_WALK_45", "TURN_LEFT_TO_WALK_90",
    "TURN_RIGHT_TO_WALK_90", "TURN_LEFT_TO_WALK_180", "TURN_RIGHT_TO_WALK_180", "STAND_TRAINED",
    "STAND_TRAINED_TO_STAND", "STAND_TRAINED_TO_COMBAT_READY", "WALK_TRAINED_1", "WALK_TRAINED_2",
    "WALK_TRAINED_3", "WALK_TRAINED_4", "WALK_TRAINED_5", "RUN_TRAINED_1",
    "RUN_TRAINED_2", "RUN_TRAINED_3", "RUN_TRAINED_4", "RUN_TRAINED_5",
    "STEP_FORWARD_TRAINED", "STEP_BACKWARD_TRAINED", "STEP_LEFT_TRAINED", "STEP_RIGHT_TRAINED",
    "TURN_LEFT_TRAINED_45", "TURN_RIGHT_TRAINED_45", "TURN_LEFT_TRAINED_90", "TURN_RIGHT_TRAINED_90",
    "TURN_LEFT_TRAINED_180", "TURN_RIGHT_TRAINED_180", "TURN_LEFT_TO_WALK_TRAINED_45", "TURN_RIGHT_TO_WALK_TRAINED_45",
    "TURN_LEFT_TO_WALK_TRAINED_90", "TURN_RIGHT_TO_WALK_TRAINED_90", "TURN_LEFT_TO_WALK_TRAINED_180", "TURN_RIGHT_TO_WALK_TRAINED_180",
    "COMBAT_READY", "COMBAT_READY_TO_STAND", "COMBAT_READY_TO_STAND_TRAINED", "COMBAT_READY_TO_COMBAT_READY_KNEEL",
    "COMBAT_READY_TO_AIM", "COMBAT_READY_TO_RELOAD_1", "COMBAT_READY_TO_RELOAD_2", "MISFIRE",
    "COMBAT_READY_ADVANCE", "COMBAT_READY_RETREAT", "COMBAT_READY_STEP_LEFT", "COMBAT_READY_STEP_RIGHT",
    "COMBAT_READY_TURN_LEFT_45", "COMBAT_READY_TURN_RIGHT_45", "COMBAT_READY_TURN_LEFT_90", "COMBAT_READY_TURN_RIGHT_90",
    "COMBAT_READY_TURN_LEFT_180", "COMBAT_READY_TURN_RIGHT_180", "COMBAT_READY_KNEEL", "COMBAT_READY_KNEEL_TO_COMBAT_READY",
    "COMBAT_READY_KNEEL_TO_PRONE", "COMBAT_READY_KNEEL_TO_KNEEL_AIM", "COMBAT_READY_KNEEL_TO_KNEEL_RELOAD", "COMBAT_READY_MELEE",
    "COMBAT_READY_MELEE_ADVANCE", "COMBAT_READY_MELEE_RETREAT", "COMBAT_READY_MELEE_STEP_LEFT", "COMBAT_READY_MELEE_STEP_RIGHT",
    "COMBAT_READY_MELEE_TURN_LEFT_45", "COMBAT_READY_MELEE_TURN_RIGHT_45", "COMBAT_READY_MELEE_TURN_LEFT_90", "COMBAT_READY_MELEE_TURN_RIGHT_90",
    "COMBAT_READY_MELEE_TURN_LEFT_180", "COMBAT_READY_MELEE_TURN_RIGHT_180", "PRONE", "PRONE_TO_STAND",
    "PRONE_TO_COMBAT_READY_KNEEL", "CHARGE", "AIM", "AIM_TO_COMBAT_READY",
    "KNEEL_AIM", "KNEEL_AIM_TO_COMBAT_READY_KNEEL", "FIRE", "RETURN_TO_FIRING_POSITION",
    "POST_FIRE_IDLE", "KNEEL_FIRE", "RELOAD_1", "RELOAD_2",
    "RELOAD_TO_COMBAT_READY", "KNEEL_RELOAD", "KNEEL_RELOAD_TO_COMBAT_READY_KNEEL", "STAND_IDLE_1",
    "STAND_IDLE_2", "STAND_IDLE_3", "STAND_IDLE_4", "STAND_IDLE_5",
    "STAND_IDLE_6", "STAND_IDLE_7", "STAND_IDLE_8", "STAND_IDLE_9",
    "STAND_IDLE_10", "STAND_IDLE_11_TELESCOPE", "STAND_TRAINED_IDLE_1", "STAND_TRAINED_IDLE_2",
    "STAND_TRAINED_IDLE_3", "STAND_TRAINED_IDLE_4", "STAND_TRAINED_IDLE_5", "STAND_TRAINED_IDLE_6",
    "STAND_FOR_STOKE", "STAND_FOR_STOKE_TO_STAND", "STAND_FOR_STOKE_TO_STOKE", "STAND_FOR_STOKE_WALK",
    "STAND_FOR_STOKE_STEP_FORWARD", "STAND_FOR_STOKE_STEP_BACKWARD", "STAND_FOR_STOKE_STEP_LEFT", "STAND_FOR_STOKE_STEP_RIGHT",
    "STAND_FOR_STOKE_TURN_LEFT_45", "STAND_FOR_STOKE_TURN_RIGHT_45", "STAND_FOR_STOKE_TURN_LEFT_90", "STAND_FOR_STOKE_TURN_RIGHT_90",
    "STAND_FOR_STOKE_TURN_LEFT_180", "STAND_FOR_STOKE_TURN_RIGHT_180", "STAND_FOR_STOKE_TURN_LEFT_TO_WALK_45", "STAND_FOR_STOKE_TURN_RIGHT_TO_WALK_45",
    "STAND_FOR_STOKE_TURN_LEFT_TO_WALK_90", "STAND_FOR_STOKE_TURN_RIGHT_TO_WALK_90", "STAND_FOR_STOKE_TURN_LEFT_TO_WALK_180", "STAND_FOR_STOKE_TURN_RIGHT_TO_WALK_180",
    "STAND_FOR_STOKE_IDLE_1", "STAND_FOR_STOKE_IDLE_2", "STAND_FOR_STOKE_IDLE_3", "STAND_FOR_STOKE_IDLE_4",
    "STAND_FOR_STOKE_IDLE_5", "STAND_FOR_FIRE_PUCKLE_GUN", "STAND_NO_WEAPON", "STAND_NO_WEAPON_TO_STAND",
    "STAND_TO_STAND_NO_WEAPON", "STAND_NO_WEAPON_WALK", "STAND_NO_WEAPON_RUN", "STAND_NO_WEAPON_STEP_FORWARD",
    "STAND_NO_WEAPON_STEP_BACKWARD", "STAND_NO_WEAPON_STEP_LEFT", "STAND_NO_WEAPON_STEP_RIGHT", "STAND_NO_WEAPON_TURN_LEFT_45",
    "STAND_NO_WEAPON_TURN_RIGHT_45", "STAND_NO_WEAPON_TURN_LEFT_90", "STAND_NO_WEAPON_TURN_RIGHT_90", "STAND_NO_WEAPON_TURN_LEFT_180",
    "STAND_NO_WEAPON_TURN_RIGHT_180", "STAND_NO_WEAPON_TURN_LEFT_TO_WALK_45", "STAND_NO_WEAPON_TURN_RIGHT_TO_WALK_45", "STAND_NO_WEAPON_TURN_LEFT_TO_WALK_90",
    "STAND_NO_WEAPON_TURN_RIGHT_TO_WALK_90", "STAND_NO_WEAPON_TURN_LEFT_TO_WALK_180", "STAND_NO_WEAPON_TURN_RIGHT_TO_WALK_180", "STAND_NO_WEAPON_IDLE_1",
    "STAND_NO_WEAPON_IDLE_2", "STAND_NO_WEAPON_IDLE_3", "STAND_NO_WEAPON_IDLE_4", "STAND_NO_WEAPON_IDLE_5",
    "STAND_ALT_1", "STAND_ALT_1_TO_STAND", "STAND_ALT_1_IDLE_1", "STAND_ALT_1_IDLE_2",
    "STAND_ALT_1_IDLE_3", "STAND_ALT_1_IDLE_4", "STAND_ALT_1_IDLE_5", "STAND_ALT_2",
    "STAND_ALT_2_TO_STAND", "STAND_ALT_2_IDLE_1", "STAND_ALT_2_IDLE_2", "STAND_ALT_2_IDLE_3",
    "STAND_ALT_2_IDLE_4", "STAND_ALT_2_IDLE_5", "STAND_ALT_3", "STAND_ALT_3_TO_STAND",
    "STAND_ALT_3_IDLE_1", "STAND_ALT_3_IDLE_2", "STAND_ALT_3_IDLE_3", "STAND_ALT_3_IDLE_4",
    "STAND_ALT_3_IDLE_5", "COMBAT_1", "COMBAT_2", "COMBAT_3",
    "COMBAT_4", "COMBAT_5", "COMBAT_6", "COMBAT_7",
    "COMBAT_8", "COMBAT_9", "COMBAT_10", "COMBAT_11",
    "COMBAT_12", "COMBAT_13", "COMBAT_14", "COMBAT_15",
    "COMBAT_16", "COMBAT_17", "COMBAT_18", "COMBAT_19",
    "COMBAT_20", "COMBAT_21", "COMBAT_22", "COMBAT_23",
    "COMBAT_24", "COMBAT_25", "COMBAT_26", "COMBAT_27",
    "COMBAT_28", "COMBAT_29", "COMBAT_30", "COMBAT_31",
    "COMBAT_32", "COMBAT_33", "COMBAT_34", "COMBAT_35",
    "COMBAT_36", "COMBAT_37", "COMBAT_38", "COMBAT_39",
    "COMBAT_40", "COMBAT_41", "COMBAT_42", "COMBAT_43",
    "COMBAT_44", "COMBAT_45", "COMBAT_46", "COMBAT_47",
    "COMBAT_48", "COMBAT_49", "COMBAT_50", "COMBAT_51",
    "COMBAT_52", "COMBAT_53", "COMBAT_54", "COMBAT_55",
    "COMBAT_56", "COMBAT_57", "COMBAT_58", "COMBAT_59",
    "COMBAT_60", "COMBAT_61", "COMBAT_62", "COMBAT_63",
    "COMBAT_64", "COMBAT_65", "COMBAT_66", "COMBAT_67",
    "COMBAT_68", "COMBAT_69", "COMBAT_70", "COMBAT_71",
    "COMBAT_72", "COMBAT_73", "COMBAT_74", "COMBAT_75",
    "COMBAT_76", "COMBAT_77", "COMBAT_78", "COMBAT_79",
    "COMBAT_80", "COMBAT_81", "COMBAT_82", "COMBAT_83",
    "COMBAT_84", "COMBAT_85", "COMBAT_86", "COMBAT_87",
    "COMBAT_88", "COMBAT_89", "COMBAT_90", "COMBAT_IDLE_1",
    "COMBAT_IDLE_2", "COMBAT_IDLE_3", "COMBAT_IDLE_4", "COMBAT_IDLE_5",
    "COMBAT_IDLE_6", "COMBAT_IDLE_7", "COMBAT_IDLE_8", "COMBAT_IDLE_9",
    "COMBAT_IDLE_10", "CHARGE_COMBAT_1", "CHARGE_COMBAT_2", "CHARGE_COMBAT_3",
    "CHARGE_COMBAT_4", "CHARGE_COMBAT_5", "CHARGE_COMBAT_6", "CHARGE_COMBAT_7",
    "CHARGE_COMBAT_8", "CHARGE_COMBAT_9", "CHARGE_COMBAT_10", "ATTACK_1",
    "ATTACK_2", "ATTACK_3", "ATTACK_4", "ATTACK_5",
    "ATTACK_6", "ATTACK_7", "ATTACK_8", "ATTACK_9",
    "ATTACK_10", "DEFEND_1", "DEFEND_2", "DEFEND_3",
    "DEFEND_4", "DEFEND_5", "DEFEND_6", "DEFEND_7",
    "DEFEND_8", "DEFEND_9", "DEFEND_10", "MOUNT_ATTACK_1",
    "MOUNT_ATTACK_2", "MOUNT_ATTACK_3", "MOUNT_ATTACK_4", "MOUNT_ATTACK_5",
    "STAND_TO_PUSH_ENGINE_LEFT_READY", "STAND_TO_PUSH_ENGINE_RIGHT_READY", "PUSH_ENGINE_LEFT_READY", "PUSH_ENGINE_LEFT_FORWARDS",
    "PUSH_ENGINE_LEFT_BACKWARDS", "PUSH_ENGINE_RIGHT_READY", "PUSH_ENGINE_RIGHT_FORWARDS", "PUSH_ENGINE_RIGHT_BACKWARDS",
    "FACIAL_ANIMATION_BLINK_1", "FACIAL_ANIMATION_FROWN_1", "FACIAL_ANIMATION_NARROW_EYES_1", "FACIAL_ANIMATION_SHOUT_1",
    "FACIAL_ANIMATION_SHOUT_2", "FACIAL_ANIMATION_SPEAK_1", "FACIAL_ANIMATION_SPEAK_2", "FACIAL_ANIMATION_SPEAK_3",
    "FACIAL_ANIMATION_SURPRISE_1", "COVER_EARS_L", "COVER_EARS_R", "COVER_EARS_S",
    "LIGHT_FUSE", "LOAD_CANNON", "STOKE", "STOKE_TO_STAND_FOR_STOKE",
    "LOAD_PUCKLE", "NAVAL_LIGHT_FUSE", "NAVAL_LOAD_CANNON", "NAVAL_STOKE",
    "NAVAL_STOKE_TO_STAND_FOR_STOKE", "NAVAL_STAND_FOR_STOKE_TO_STOKE", "NAVAL_PUSH", "MORTAR_STOKE",
    "MORTAR_STOKE_TO_STAND_FOR_STOKE", "MORTAR_STAND_FOR_STOKE_TO_STOKE", "FORT_LIGHT_FUSE", "FORT_PUSH",
    "LOAD_MORTAR", "LIGHT_MORTAR", "SET_ROCKET", "LOAD_ROCKET",
    "LIGHT_ROCKET", "CROSS_OBSTACLE_STAND_TO_STAND", "REFUSE_1", "REFUSE_2",
    "REFUSE_3", "REFUSE_4", "RIDER_REFUSE_1", "RIDER_REFUSE_2",
    "RIDER_REFUSE_3", "RIDER_REFUSE_4", "REFUSE_CAN_THROW_RIDER_1", "REFUSE_CAN_THROW_RIDER_2",
    "REFUSE_CAN_THROW_RIDER_3", "REFUSE_CAN_THROW_RIDER_4", "RIDER_REFUSE_CAN_THROW_RIDER_1", "RIDER_REFUSE_CAN_THROW_RIDER_2",
    "RIDER_REFUSE_CAN_THROW_RIDER_3", "RIDER_REFUSE_CAN_THROW_RIDER_4", "BE_MOUNTED_LEFT", "BE_MOUNTED_RIGHT",
    "BE_DISMOUNTED_LEFT", "BE_DISMOUNTED_RIGHT", "SMASH_WINDOW", "RIDER_STAND",
    "RIDER_STAND_TO_WALK", "RIDER_WALK", "RIDER_WALK_TO_STAND", "RIDER_WALK_TO_RUN",
    "RIDER_WALK_TO_TROT", "RIDER_WALK_JUMP", "RIDER_RUN", "RIDER_RUN_TO_STAND",
    "RIDER_RUN_TO_WALK", "RIDER_RUN_TO_GALLOP", "RIDER_RUN_JUMP", "RIDER_TROT",
    "RIDER_TROT_TO_STAND", "RIDER_TROT_TO_WALK", "RIDER_TROT_TO_CANTER", "RIDER_TROT_JUMP",
    "RIDER_CANTER", "RIDER_CANTER_TO_STAND", "RIDER_CANTER_TO_TROT", "RIDER_CANTER_TO_GALLOP",
    "RIDER_CANTER_JUMP", "RIDER_GALLOP", "RIDER_GALLOP_TO_STAND", "RIDER_GALLOP_TO_RUN",
    "RIDER_GALLOP_TO_CANTER", "RIDER_GALLOP_JUMP", "RIDER_CHARGE_SYNCHED", "RIDER_STEP_FORWARD",
    "RIDER_STEP_BACKWARD", "RIDER_STEP_LEFT", "RIDER_STEP_RIGHT", "RIDER_TURN_LEFT_45",
    "RIDER_TURN_RIGHT_45", "RIDER_TURN_LEFT_90", "RIDER_TURN_RIGHT_90", "RIDER_TURN_LEFT_180",
    "RIDER_TURN_RIGHT_180", "RIDER_TURN_LEFT_TO_WALK_45", "RIDER_TURN_RIGHT_TO_WALK_45", "RIDER_TURN_LEFT_TO_WALK_90",
    "RIDER_TURN_RIGHT_TO_WALK_90", "RIDER_TURN_LEFT_TO_WALK_180", "RIDER_TURN_RIGHT_TO_WALK_180", "RIDER_COMBAT_1",
    "RIDER_COMBAT_2", "RIDER_COMBAT_3", "RIDER_COMBAT_4", "RIDER_COMBAT_5",
    "RIDER_COMBAT_6", "RIDER_COMBAT_7", "RIDER_COMBAT_8", "RIDER_COMBAT_9",
    "RIDER_COMBAT_10", "RIDER_COMBAT_IDLE_1", "RIDER_COMBAT_IDLE_2", "RIDER_COMBAT_IDLE_3",
    "RIDER_COMBAT_IDLE_4", "RIDER_COMBAT_IDLE_5", "RIDER_COMBAT_IDLE_6", "RIDER_COMBAT_IDLE_7",
    "RIDER_COMBAT_IDLE_8", "RIDER_COMBAT_IDLE_9", "RIDER_COMBAT_IDLE_10", "RIDER_ATTACK_1",
    "RIDER_ATTACK_2", "RIDER_ATTACK_3", "RIDER_ATTACK_4", "RIDER_ATTACK_5",
    "RIDER_ATTACK_6", "RIDER_ATTACK_7", "RIDER_ATTACK_8", "RIDER_ATTACK_9",
    "RIDER_ATTACK_10", "RIDER_MOUNT_ATTACK_1", "RIDER_MOUNT_ATTACK_2", "RIDER_MOUNT_ATTACK_3",
    "RIDER_MOUNT_ATTACK_4", "RIDER_MOUNT_ATTACK_5", "RIDER_ATTACK_BLOCKED_1", "RIDER_ATTACK_BLOCKED_2",
    "RIDER_ATTACK_BLOCKED_3", "RIDER_ATTACK_BLOCKED_4", "RIDER_ATTACK_BLOCKED_5", "RIDER_CHARGE_ATTACK_1",
    "RIDER_CHARGE_ATTACK_2", "RIDER_CHARGE_ATTACK_3", "RIDER_CHARGE_ATTACK_4", "RIDER_CHARGE_ATTACK_5",
    "RIDER_CARRY_WEAPON", "RIDER_COMBAT_READY", "RIDER_CHARGE", "RIDER_SHOOT_READY",
    "RIDER_SHOOT_READY_TO_RIDER_RELOAD", "RIDER_RELOAD", "RIDER_RELOAD_TO_RIDER_SHOOT_READY", "RIDER_FIRE_FORWARD_LOW",
    "RIDER_FIRE_FORWARD_LEVEL", "RIDER_FIRE_FORWARD_HIGH", "RIDER_FIRE_LEFT_LOW", "RIDER_FIRE_LEFT_LEVEL",
    "RIDER_FIRE_LEFT_HIGH", "RIDER_FIRE_RIGHT_LOW", "RIDER_FIRE_RIGHT_LEVEL", "RIDER_FIRE_RIGHT_HIGH",
    "RIDER_STAND_IDLE_1", "RIDER_STAND_IDLE_2", "RIDER_STAND_IDLE_3", "RIDER_STAND_IDLE_4",
    "RIDER_STAND_IDLE_5", "RIDER_DEATH_STAND_1", "RIDER_DEATH_STAND_2", "RIDER_DEATH_STAND_3",
    "RIDER_DEATH_STAND_4", "RIDER_DEATH_STAND_5", "RIDER_DEATH_JUMP_1", "RIDER_DEATH_JUMP_2",
    "RIDER_DEATH_JUMP_3", "RIDER_DEATH_JUMP_4", "RIDER_DEATH_JUMP_5", "RIDER_DEATH_JUMP_6",
    "RIDER_DEATH_JUMP_7", "RIDER_DEATH_JUMP_8", "RIDER_STAND_CHEVAUX_DE_FRISE_DEATH_1", "RIDER_WALK_CHEVAUX_DE_FRISE_DEATH_1",
    "RIDER_TROT_CHEVAUX_DE_FRISE_DEATH_1", "RIDER_CANTER_CHEVAUX_DE_FRISE_DEATH_1", "RIDER_GALLOP_CHEVAUX_DE_FRISE_DEATH_1", "RIDER_DEATH_MOVING_1",
    "RIDER_DEATH_MOVING_2", "RIDER_DEATH_MOVING_3", "RIDER_DEATH_MOVING_4", "RIDER_DEATH_MOVING_5",
    "RIDER_DEATH_MOVING_6", "RIDER_DEATH_MOVING_7", "RIDER_DEATH_MOVING_8", "RIDER_DEATH_MOVING_9",
    "RIDER_DEATH_MOVING_10", "RIDER_DEATH_MOVING_11", "RIDER_DEATH_MOVING_12", "RIDER_DEATH_ATTACHED_TRANSITION_1",
    "RIDER_DEATH_ATTACHED_STATIONARY_1", "RIDER_DEATH_ATTACHED_MOVING_1", "RIDER_CROSS_OBSTACLE_STAND_TO_STAND", "RIDER_MOUNT_LEFT",
    "RIDER_MOUNT_RIGHT", "RIDER_DISMOUNT_LEFT", "RIDER_DISMOUNT_RIGHT", "RIDER_PLAY_INSTRUMENT",
    "RIDER_PLAY_INSTRUMENT_TO_STAND", "RIDER_STAND_TO_PLAY_INSTRUMENT", "ENGINE_LIMBER", "ENGINE_UNLIMBER",
    "ENGINE_LIMBERED", "ENGINE_LIMBERED_FORWARDS", "ENGINE_LIMBERED_BACKWARDS", "ENGINE_LIMBERED_TURN_LEFT",
    "ENGINE_LIMBERED_TURN_RIGHT", "GRENADIER", "FIX_BAYONET", "FIX_BAYONET_TRAINED",
    "DEPLOY_WOODEN_STAKES", "PLAY_INSTRUMENT", "PLAY_INSTRUMENT_TO_STAND", "STAND_TO_PLAY_INSTRUMENT",
    "REPAIR_1", "STAND_TO_REPAIR_1", "REPAIR_TO_STAND_1", "REPAIR_2",
    "STAND_TO_REPAIR_2", "REPAIR_TO_STAND_2", "REPAIR_3", "REPAIR_4",
    "REPAIR_5", "STAND_TO_REPAIR_5", "REPAIR_TO_STAND_5", "SNEAK",
    "CROUCH", "STEP_FORWARD_CROUCH", "STEP_BACKWARD_CROUCH", "STEP_LEFT_CROUCH",
    "STEP_RIGHT_CROUCH", "TURN_LEFT_CROUCH_45", "TURN_RIGHT_CROUCH_45", "TURN_LEFT_CROUCH_90",
    "TURN_RIGHT_CROUCH_90", "TURN_CROUCH_180", "TURN_LEFT_CROUCH_TO_WALK_45", "TURN_RIGHT_CROUCH_TO_WALK_45",
    "TURN_LEFT_CROUCH_TO_WALK_90", "TURN_RIGHT_CROUCH_TO_WALK_90", "TURN_LEFT_CROUCH_TO_WALK_180", "TURN_RIGHT_CROUCH_TO_WALK_180",
    "CROUCH_TO_STAND", "STAND_TO_CROUCH", "CROUCH_TO_KNEEL", "KNEEL_TO_CROUCH",
    "CROUCH_TO_SNEAK", "SNEAK_TO_CROUCH", "CROUCH_IDLE_1", "CROUCH_IDLE_2",
    "CROUCH_IDLE_3", "CROUCH_IDLE_4", "CROUCH_IDLE_5", "JUMP1",
    "JUMP2", "JUMP3", "JUMP_ACROSS", "FALL_CONTROLLED1",
    "FALL_CONTROLLED2", "FALL_CONTROLLED3", "FALL_FLAILING1", "FALL_FLAILING2",
    "FALL_FLAILING3", "FALL_TUMBLING", "ANTICIPATE_LANDING", "LAND_ON_SPOT",
    "LAND_WITH_MOVEMENT", "LAND_DEATH", "SEA_FLOATING_FLAIL_1", "SEA_FLOATING_FLAIL_2",
    "SEA_FLOATING_FLAIL_3", "SEA_FLOATING_FLAIL_4", "SEA_FLOATING_DEAD_1", "SEA_FLOATING_DEAD_2",
    "GRAPPLE_STAND_TO_SWING", "GRAPPLE_SWING", "GRAPPLE_SWING_TO_THROW", "GRAPPLE_PULL",
    "GRAPPLE_PULL_TO_STAND", "BOARDING_PLANK_PICKUP", "BOARDING_PLANK_LIFT", "BOARDING_PLANK_DROP",
    "DEATH_STAND_1", "DEATH_STAND_2", "DEATH_STAND_3", "DEATH_STAND_4",
    "DEATH_STAND_5", "DEATH_STAND_6", "DEATH_STAND_7", "DEATH_STAND_8",
    "DEATH_STAND_9", "DEATH_STAND_10", "DEATH_STAND_11", "DEATH_STAND_12",
    "DEATH_STAND_13", "DEATH_STAND_TRAINED_1", "DEATH_STAND_TRAINED_2", "DEATH_STAND_TRAINED_3",
    "DEATH_STAND_TRAINED_4", "DEATH_STAND_TRAINED_5", "DEATH_STAND_TRAINED_6", "DEATH_STAND_TRAINED_7",
    "DEATH_STAND_TRAINED_8", "DEATH_STAND_TRAINED_9", "DEATH_STAND_TRAINED_10", "DEATH_WALK_1",
    "DEATH_WALK_2", "DEATH_WALK_3", "DEATH_WALK_4", "DEATH_WALK_5",
    "DEATH_MARCH_1", "DEATH_MARCH_2", "DEATH_MARCH_3", "DEATH_MARCH_4",
    "DEATH_MARCH_5", "DEATH_RELOAD_1", "DEATH_RELOAD_2", "DEATH_RELOAD_3",
    "DEATH_RELOAD_4", "DEATH_RELOAD_5", "DEATH_POISED_1", "DEATH_POISED_2",
    "DEATH_POISED_3", "DEATH_POISED_4", "DEATH_POISED_5", "DEATH_POISED_6",
    "DEATH_POISED_7", "DEATH_POISED_8", "DEATH_POISED_9", "DEATH_POISED_10",
    "DEATH_RUN_1", "DEATH_RUN_2", "DEATH_RUN_3", "DEATH_RUN_4",
    "DEATH_RUN_5", "DEATH_RUN_6", "DEATH_RUN_7", "DEATH_RUN_TRAINED_1",
    "DEATH_RUN_TRAINED_2", "DEATH_RUN_TRAINED_3", "DEATH_RUN_TRAINED_4", "DEATH_RUN_TRAINED_5",
    "DEATH_CHARGE_1", "DEATH_CHARGE_2", "DEATH_CHARGE_3", "DEATH_CHARGE_4",
    "DEATH_CHARGE_5", "DEATH_CHARGE_6", "DEATH_CHARGE_7", "DEATH_ALT_STAND1_1",
    "DEATH_ALT_STAND1_2", "DEATH_ALT_STAND1_3", "DEATH_ALT_STAND1_4", "DEATH_ALT_STAND1_5",
    "DEATH_ALT_STAND2_1", "DEATH_ALT_STAND2_2", "DEATH_ALT_STAND2_3", "DEATH_ALT_STAND2_4",
    "DEATH_ALT_STAND2_5", "DEATH_ALT_STAND3_1", "DEATH_ALT_STAND3_2", "DEATH_ALT_STAND3_3",
    "DEATH_ALT_STAND3_4", "DEATH_ALT_STAND3_5", "DEATH_KNEEL_1", "DEATH_KNEEL_2",
    "DEATH_KNEEL_3", "DEATH_KNEEL_4", "DEATH_KNEEL_5", "DEATH_KNEEL_POISED_1",
    "DEATH_KNEEL_POISED_2", "DEATH_KNEEL_POISED_3", "DEATH_KNEEL_POISED_4", "DEATH_KNEEL_POISED_5",
    "DEATH_COMBAT_READY_1", "DEATH_COMBAT_READY_2", "DEATH_COMBAT_READY_3", "DEATH_COMBAT_READY_4",
    "DEATH_COMBAT_READY_5", "DEATH_COMBAT_READY_6", "DEATH_COMBAT_READY_7", "DEATH_COMBAT_READY_8",
    "DEATH_COMBAT_READY_9", "DEATH_COMBAT_READY_10", "DEATH_JUMP_1", "DEATH_JUMP_2",
    "DEATH_JUMP_3", "DEATH_JUMP_4", "DEATH_JUMP_5", "DEATH_JUMP_6",
    "DEATH_JUMP_7", "DEATH_JUMP_8", "STAND_CHEVAUX_DE_FRISE_DEATH_1", "WALK_CHEVAUX_DE_FRISE_DEATH_1",
    "TROT_CHEVAUX_DE_FRISE_DEATH_1", "CANTER_CHEVAUX_DE_FRISE_DEATH_1", "GALLOP_CHEVAUX_DE_FRISE_DEATH_1", "DEATH_MOVING_1",
    "DEATH_MOVING_2", "DEATH_MOVING_3", "DEATH_MOVING_4", "DEATH_MOVING_5",
    "DEATH_MOVING_6", "DEATH_MOVING_7", "DEATH_MOVING_8", "DEATH_MOVING_9",
    "DEATH_MOVING_10", "DEATH_MOVING_11", "DEATH_MOVING_12", "CLIMB_UP_LADDER",
    "CLIMB_DOWN_LADDER", "CLIMB_UP_STAIRS", "CLIMB_DOWN_STAIRS", "CLIMB_UP_ROPE",
    "CLIMB_DOWN_ROPE", "CLIMB_ON_ROPE_BOTTOM", "CLIMB_OFF_ROPE_TOP", "CLIMB_ON_ROPE_TOP",
    "CLIMB_OFF_ROPE_BOTTOM", "CLIMB_UP_RIGGING", "CLIMB_DOWN_RIGGING", "CLIMB_ON_RIGGING_BOTTOM",
    "CLIMB_OFF_RIGGING_TOP", "CLIMB_ON_RIGGING_TOP", "CLIMB_OFF_RIGGING_BOTTOM", "CLIMB_UP_GRAPPLE",
    "CLIMB_DOWN_GRAPPLE", "KNOCKBACK_1", "KNOCKBACK_2", "KNOCKBACK_3",
    "KNOCKBACK_4", "KNOCKBACK_5", "KNOCKDOWN_1", "KNOCKDOWN_2",
    "KNOCKDOWN_3", "KNOCKDOWN_4", "KNOCKDOWN_5", "KNOCKED_FLYING_1",
    "DEATH_KNOCKED_FLYING_LANDED_1", "WOUNDED_FRONT_IDLE", "WOUNDED_FRONT_INJURY1", "WOUNDED_FRONT_INJURY2",
    "WOUNDED_FRONT_INJURY3", "WOUNDED_FRONT_CRAWL", "WOUNDED_BACK_IDLE", "WOUNDED_BACK_INJURY1",
    "WOUNDED_BACK_INJURY2", "WOUNDED_BACK_CRAWL", "FALL_OVER_BACKWARDS", "FALL_OVER_FORWARDS",
    "FALL_OVER_LEFTSIDE", "FALL_OVER_RIGHTSIDE", "FACE_DOWN_GET_UP", "FACE_UP_GET_UP",
    "LYING_DOWN_FACE_DOWN", "LYING_DOWN_FACE_UP", "PIKEMEN_CHARGED_HIGH_IDLE", "PIKEMEN_CHARGED_HIGH_WALK",
    "PIKEMEN_CHARGED_HIGH_FORWARDS", "PIKEMEN_CHARGED_HIGH_BACKWARDS", "PIKEMEN_CHARGED_HIGH_LEFT", "PIKEMEN_CHARGED_HIGH_RIGHT",
    "PIKEMEN_CHARGED_HIGH_LEFT_45", "PIKEMEN_CHARGED_HIGH_RIGHT_45", "PIKEMEN_CHARGED_HIGH_THRUST_1", "PIKEMEN_CHARGED_HIGH_THRUST_2",
    "PIKEMEN_CHARGED_DEEP_IDLE", "PIKEMEN_CHARGED_DEEP_WALK", "PIKEMEN_CHARGED_DEEP_FORWARDS", "PIKEMEN_CHARGED_DEEP_BACKWARDS",
    "PIKEMEN_CHARGED_DEEP_LEFT", "PIKEMEN_CHARGED_DEEP_RIGHT", "PIKEMEN_CHARGED_DEEP_LEFT_45", "PIKEMEN_CHARGED_DEEP_RIGHT_45",
    "PIKEMEN_CHARGED_DEEP_THRUST_1", "PIKEMEN_CHARGED_DEEP_THRUST_2", "PIKEMEN_CHARGED_FRONT_IDLE", "PIKEMEN_CHARGED_FRONT_WALK",
    "PIKEMEN_CHARGED_FRONT_FORWARDS", "PIKEMEN_CHARGED_FRONT_BACKWARDS", "PIKEMEN_CHARGED_FRONT_LEFT", "PIKEMEN_CHARGED_FRONT_RIGHT",
    "PIKEMEN_CHARGED_FRONT_LEFT_45", "PIKEMEN_CHARGED_FRONT_RIGHT_45", "PIKEMEN_CHARGED_FRONT_THRUST_1", "PIKEMEN_CHARGED_FRONT_THRUST_2",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_geometry_matches_the_exe() {
        // 0x10 header + 864 * 0x90 is the 0x1E610-byte object the exe allocates.
        assert_eq!(0x10 + ACTION_SLOT_COUNT * ACTION_SLOT_BYTES, 0x1E610);
        // A slot block is five 0x1C fragments plus the u32 count in the last dword.
        assert_eq!(
            MAX_FRAGMENTS_PER_SLOT * FRAGMENT_BYTES + 4,
            ACTION_SLOT_BYTES
        );
        assert_eq!(EMPTY_FRAGMENT_TAG, ACTION_SLOT_COUNT as u32);
    }

    #[test]
    fn fragment_fields_fit_the_fragment() {
        // The fields the parser writes, in order. The gaps between them are the
        // widths it writes: u32, three-word UniString, f32, u32, u16.
        let offsets = [
            fragment_field::TAG,
            fragment_field::FILENAME,
            fragment_field::BLEND_IN_TIME,
            fragment_field::EQUIPMENT_USAGE,
            fragment_field::FLAGS,
        ];
        assert!(
            offsets.windows(2).all(|w| w[0] < w[1]),
            "fields must ascend"
        );
        assert_eq!(offsets[1] - offsets[0], 4);
        assert_eq!(offsets[2] - offsets[1], 12);
        assert_eq!(offsets[3] - offsets[2], 4);
        assert_eq!(offsets[4] - offsets[3], 4);
        // The flag word ends the record; the parser never writes the last two bytes.
        assert!(offsets[4] + 2 <= FRAGMENT_BYTES);
    }

    #[test]
    fn equipment_usage_is_a_closed_set_of_ten() {
        assert_eq!(EquipmentUsage::ALL.len(), 10);
        for u in EquipmentUsage::ALL {
            assert_eq!(EquipmentUsage::from_keyword(u.keyword()), Some(u));
            assert!(u.code() < EQUIPMENT_USAGE_UNRECOGNISED);
            // The exe's compare is case sensitive.
            assert_eq!(
                EquipmentUsage::from_keyword(&u.keyword().to_uppercase()),
                None
            );
        }
        let codes: Vec<u32> = EquipmentUsage::ALL.iter().map(|u| u.code()).collect();
        assert_eq!(codes, (0..10).collect::<Vec<u32>>());
        // Both sentinels sit outside the ten real codes.
        assert_eq!(EQUIPMENT_USAGE_UNRECOGNISED, 10);
        assert_eq!(EQUIPMENT_USAGE_CANCELLED, 11);
        assert!(EquipmentUsage::from_keyword("cancel").is_none());
        assert!(EquipmentUsage::from_keyword("musket").is_none());
    }

    #[test]
    fn special_usage_bits_are_disjoint_and_above_the_display_flags() {
        let bits: Vec<u16> = SpecialUsage::ALL.iter().map(|s| s.bit()).collect();
        let mut sorted = bits.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 10, "two usages share a bit");
        // Nothing in the special mask may collide with the five on/off keywords.
        let highest_display = DISPLAY_FLAGS.iter().map(|(b, _)| *b).max().unwrap();
        assert!(bits.iter().all(|b| *b > highest_display));
        assert!(bits.iter().all(|b| b.count_ones() == 1));
        // The invalid marker is the top bit, outside every real usage.
        assert_eq!(SPECIAL_USAGE_UNRECOGNISED, 0x8000);
        assert!(bits.iter().all(|b| *b < SPECIAL_USAGE_UNRECOGNISED));
    }

    #[test]
    fn display_flags_are_five_distinct_bits_with_the_exe_keywords() {
        assert_eq!(DISPLAY_FLAGS.len(), 5);
        let bits: Vec<u16> = DISPLAY_FLAGS.iter().map(|(b, _)| *b).collect();
        let mut sorted = bits.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, vec![1, 2, 4, 8, 16]);
        assert_eq!(
            DISPLAY_FLAGS.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
            [
                "primary_weapon",
                "secondary_weapon",
                "defensive",
                "ambient",
                "personal"
            ]
        );
        // Several may be set on one fragment, and the special mask ORs in above them.
        let word = FLAG_PRIMARY_WEAPON | FLAG_AMBIENT | SpecialUsage::ArrowMan.bit();
        assert_eq!(word & FLAG_PRIMARY_WEAPON, FLAG_PRIMARY_WEAPON);
        assert_eq!(word & FLAG_DEFENSIVE, 0);
        assert_eq!(word & SpecialUsage::CannonBall.bit(), 0);
        assert_eq!(ON_OFF, ["on", "off"]);
    }

    #[test]
    fn slot_names_are_unique_and_resolve_to_their_own_index() {
        assert_eq!(SLOT_NAMES.len(), ACTION_SLOT_COUNT);
        let mut seen = std::collections::HashSet::new();
        for (i, name) in SLOT_NAMES.iter().enumerate() {
            assert!(seen.insert(*name), "duplicate slot name {name}");
            assert_eq!(action_slot(name), Some(i));
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn known_slots_resolve_to_the_exe_indices() {
        // Read out of the exe's name table at 0x013AEBE0, stride 0x18.
        assert_eq!(action_slot("MISSING_ANIM"), Some(0));
        assert_eq!(action_slot("STAND"), Some(1));
        assert_eq!(action_slot("STAND_TO_WALK"), Some(3));
        assert_eq!(action_slot("WALK_1"), Some(10));
        assert_eq!(action_slot("WALK_TO_STAND"), Some(15));
        assert_eq!(action_slot("AIM"), Some(126));
        assert_eq!(action_slot("FIRE"), Some(130));
        assert_eq!(action_slot("COMBAT_1"), Some(229));
        assert_eq!(action_slot("COMBAT_83"), Some(311));
        assert_eq!(SLOT_NAMES[863], "PIKEMEN_CHARGED_FRONT_THRUST_2");
    }

    #[test]
    fn unknown_slot_names_do_not_resolve() {
        assert_eq!(
            action_slot("stand"),
            None,
            "the exe's compare is case sensitive"
        );
        assert_eq!(action_slot("NOT_AN_ACTION"), None);
        assert_eq!(action_slot(""), None);
    }

    #[test]
    fn doc_comment_slot_names_are_real_slots() {
        // The module docs abbreviate. `RIDER_MOUNT_LEFT` is a real slot; the bare
        // `IDLE_1` and `MOUNT` of the examples are not, the real ones carry a prefix.
        assert!(action_slot("RIDER_MOUNT_LEFT").is_some());
        assert!(action_slot("IDLE_1").is_none());
        assert!(action_slot("MOUNT").is_none());
        assert!(action_slot("STAND_IDLE_1").is_some());
        assert!(action_slot("RIDER_MOUNT_LEFT").is_some());
    }

    // -- round 10: the sentinels, the tag and the defaults, hardened -------------

    #[test]
    fn equipment_usage_sentinels_are_not_usages() {
        // 10 is what the exe's reader returns for a name it does not know *and* what a
        // fresh fragment is pre-filled with, so a line with no `equipment_usage` at all
        // reads as 10 rather than 0 (`none`). It is a sentinel, not a usage.
        assert_eq!(EQUIPMENT_USAGE_UNRECOGNISED, 10);
        assert_ne!(EQUIPMENT_USAGE_UNRECOGNISED, EquipmentUsage::None.code());
        assert!(
            EquipmentUsage::ALL
                .iter()
                .all(|u| u.code() != EQUIPMENT_USAGE_UNRECOGNISED)
        );
        assert!(EquipmentUsage::from_keyword("unrecognised").is_none());

        // 11 is the `cancel` marker, not an eleventh usage. The exe's `equipment_usage`
        // reader never returns it: `cancel` is tested before the name reader runs.
        assert_eq!(EQUIPMENT_USAGE_CANCELLED, 11);
        assert!(
            EquipmentUsage::ALL
                .iter()
                .all(|u| u.code() != EQUIPMENT_USAGE_CANCELLED)
        );
        assert!(EquipmentUsage::from_keyword("cancel").is_none());

        // The two sentinels are adjacent and both sit above the ten real codes, so a
        // caller can range-check with one comparison.
        assert_eq!(EQUIPMENT_USAGE_CANCELLED, EQUIPMENT_USAGE_UNRECOGNISED + 1);
        let highest = EquipmentUsage::ALL.iter().map(|u| u.code()).max().unwrap();
        assert_eq!(highest + 1, EQUIPMENT_USAGE_UNRECOGNISED);
    }

    #[test]
    fn special_usage_unrecognised_bit_marks_the_fragment() {
        assert_eq!(SPECIAL_USAGE_UNRECOGNISED, 0x8000);
        // The top bit, so it can never be confused with one of the ten real usages and
        // never disturbs the five on/off bits below it.
        assert_eq!(SPECIAL_USAGE_UNRECOGNISED.count_ones(), 1);
        assert!(
            SpecialUsage::ALL
                .iter()
                .all(|s| s.bit() & SPECIAL_USAGE_UNRECOGNISED == 0)
        );
        assert!(
            SpecialUsage::ALL
                .iter()
                .all(|s| s.bit() & FLAG_PERSONAL == 0)
        );
        // The reader ORs, so a bad name rides alongside the good ones instead of
        // replacing them: the fragment ends up marked, not silently accepted.
        let word = FLAG_DEFENSIVE | SpecialUsage::Bayonet.bit() | SPECIAL_USAGE_UNRECOGNISED;
        assert_eq!(word & FLAG_DEFENSIVE, FLAG_DEFENSIVE);
        assert_eq!(
            word & SpecialUsage::Bayonet.bit(),
            SpecialUsage::Bayonet.bit()
        );
        assert_eq!(
            word & SPECIAL_USAGE_UNRECOGNISED,
            SPECIAL_USAGE_UNRECOGNISED
        );
        // `special` is spelled like the on/off keywords but resolved by its own
        // case-sensitive reader, so it is not one of them.
        assert!(!ON_OFF.contains(&"special"));
        assert!(SpecialUsage::from_keyword("axe").is_some());
        assert!(EquipmentUsage::from_keyword("axe").is_some());
        // Every `special` name is lower case in a file and the reader compares with
        // strcmp, so an upper-case spelling is not a usage at all.
        for s in SpecialUsage::ALL {
            assert_eq!(SpecialUsage::from_keyword(s.keyword()), Some(s));
            assert!(
                SpecialUsage::from_keyword(&s.keyword().to_uppercase()).is_none(),
                "{} must be case sensitive",
                s.keyword()
            );
            // `axe` is the one name the two closed sets share, and they are separate
            // readers with separate meanings (a weapon kind vs a bit of the flag word).
            assert_eq!(
                EquipmentUsage::from_keyword(s.keyword()).is_some(),
                s.keyword() == "axe"
            );
        }
    }

    #[test]
    fn empty_fragment_tag_is_out_of_range_as_a_slot_index() {
        // 0x360 is both the count of action slots and the tag a fragment carries while
        // it is still empty, so the tag can never be mistaken for a real slot index:
        // every materialised fragment's tag is strictly below the sentinel.
        assert_eq!(EMPTY_FRAGMENT_TAG, 0x360);
        assert_eq!(EMPTY_FRAGMENT_TAG, ACTION_SLOT_COUNT as u32);
        assert!(EMPTY_FRAGMENT_TAG as usize > ACTION_SLOT_COUNT - 1);
        for slot in [0usize, 1, 431, 432, 862, 863] {
            assert!((slot as u32) < EMPTY_FRAGMENT_TAG, "slot {slot}");
            assert!(action_slot(SLOT_NAMES[slot]) == Some(slot));
        }
        // The sentinel is exactly one past the last slot, which is what lets the exe's
        // reader treat "tag == 0x360" as "no fragment here".
        assert_eq!(EMPTY_FRAGMENT_TAG as usize, ACTION_SLOT_COUNT);
        assert_eq!(EMPTY_FRAGMENT_TAG as usize - 1, SLOT_NAMES.len() - 1);
        // The tag lives in the fragment's first dword and the count in the slot block's
        // last dword, so the two never overlap.
        assert_eq!(fragment_field::TAG, 0);
        assert_eq!(
            ACTION_SLOT_BYTES - 4,
            MAX_FRAGMENTS_PER_SLOT * FRAGMENT_BYTES
        );
    }

    #[test]
    fn blend_in_time_defaults_to_one_second() {
        // The parser writes 1.0f (0x3F800000) into every fragment it materialises, so
        // a line that does not mention blend_in_time still blends for a second.
        assert_eq!(DEFAULT_BLEND_IN_TIME, 1.0);
        assert_eq!(DEFAULT_BLEND_IN_TIME.to_bits(), 0x3F80_0000);
        // It is stored as an f32 at fragment + 0x10, i.e. inside the record.
        assert_eq!(fragment_field::BLEND_IN_TIME, 0x10);
        const { assert!(fragment_field::BLEND_IN_TIME + 4 <= FRAGMENT_BYTES) };

        // Our reader is deliberately tolerant and leaves the field absent, so the
        // default has to be applied where a fragment becomes real. Both the absent and
        // the explicit case must survive that substitution.
        let parsed = Fragment::parse(BASE);
        let stand = parsed.get("STAND").unwrap();
        assert!(
            stand[0].blend_in_time.is_none(),
            "the line says nothing, so the field is absent"
        );
        let resolved = stand[0].blend_in_time.unwrap_or(DEFAULT_BLEND_IN_TIME);
        assert_eq!(resolved, 1.0);
        assert_eq!(stand[1].blend_in_time, Some(0.5));
        assert_ne!(stand[1].blend_in_time.unwrap_or(DEFAULT_BLEND_IN_TIME), 1.0);
    }

    #[test]
    fn action_slot_is_an_exact_case_sensitive_compare() {
        // The exe looks the name up with strcmp, so it is case sensitive and it is an
        // equality test: no normalisation, no prefixes, no substrings.
        assert_eq!(action_slot("STAND"), Some(1));
        for wrong in [
            "stand",
            "Stand",
            "sTAND",
            "STAND ",
            " STAND",
            "STAN",
            "STAND_",
            "STANDX",
            "STAND_TO_WALK_",
            "WALK_1X",
            "WALK_11",
        ] {
            assert_eq!(action_slot(wrong), None, "{wrong:?} must not resolve");
        }
        // Case sensitivity holds across the whole range, not just for the first slots.
        for i in [0usize, 1, 126, 229, 431, 700, 863] {
            let name = SLOT_NAMES[i];
            assert_eq!(action_slot(name), Some(i));
            assert_eq!(action_slot(&name.to_lowercase()), None, "slot {i}");
            if name.contains('_') {
                assert_eq!(
                    action_slot(&name.replace('_', " ")),
                    None,
                    "slot {i}: underscores are not normalised"
                );
            }
            // Every name is already upper case, so `to_uppercase` is the identity and
            // still resolves: case sensitivity bites downwards, not upwards.
            assert_eq!(name.to_uppercase(), *name);
        }
    }

    #[test]
    fn slot_names_spot_check_that_the_index_is_the_slot() {
        // The exe's descriptor table stores the index in +0x00 and the name in +0x04,
        // and for all 864 entries they agree, so the table order *is* the slot index.
        // Spot-check the ends and the middle rather than trusting the transcription.
        for (i, expected) in [
            (0usize, "MISSING_ANIM"),
            (1, "STAND"),
            (432, "RIDER_WALK_TO_TROT"),
            (863, "PIKEMEN_CHARGED_FRONT_THRUST_2"),
        ] {
            assert_eq!(SLOT_NAMES[i], expected, "slot {i}");
            assert_eq!(action_slot(SLOT_NAMES[i]), Some(i), "slot {i}");
            assert_eq!(SLOT_NAMES[i].len(), expected.len());
        }
        // The names are the exe's own: upper case, digits and underscores only, and
        // never empty, so a mistyped case cannot slip into the table.
        for (i, name) in SLOT_NAMES.iter().enumerate() {
            assert!(!name.is_empty(), "slot {i}");
            assert!(
                name.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'),
                "slot {i} {name:?} is not an exe's ACTION_ suffix"
            );
            assert_eq!(action_slot(name), Some(i), "slot {i}");
        }
    }

    // -- round 11: the fragment rule (`0x00E5F760`) and the +0x14 predicate ------------

    #[test]
    fn fragments_of_one_slot_are_alternatives_chosen_by_modulo() {
        // The CONFIRMED rule, and the point of the whole exercise: five fragments of a
        // slot are five *alternatives*, and the exe walks them with one unsigned DIV,
        // `index % count`. A sixth slot therefore plays the first fragment AGAIN -- which
        // is impossible if a later fragment replaced an earlier one.
        let count = MAX_FRAGMENTS_PER_SLOT; // 5
        assert_eq!(count, 5);
        for (index, want) in [
            (0usize, 0usize),
            (1, 1),
            (2, 2),
            (3, 3),
            (4, 4),
            (5, 0), // one past the end: wraps to the first, does not run off
            (6, 1),
            (7, 2),
        ] {
            assert_eq!(
                fragment_index(index, count),
                Some(want),
                "count 5, index {index} -> fragment {want}"
            );
            assert!(want < count, "the result must be a real fragment");
        }
        // A slot far past the count keeps the same short cycle, and the whole cycle
        // covers every fragment exactly once before repeating.
        for index in [10usize, 11, 12, 13, 14, 15, 100, 431, 862, 863, 864, 5000] {
            assert_eq!(
                fragment_index(index, count),
                Some(index % 5),
                "index {index}"
            );
        }
        for index in 0..1000usize {
            assert_eq!(fragment_index(index, count), Some(index % 5), "{index}");
            assert!(fragment_index(index, count).unwrap() < count);
        }
        // `0` and `1` counts cannot divide, and the exe would fault: we refuse instead.
        assert_eq!(fragment_index(0, 0), None);
        assert_eq!(fragment_index(7, 0), None);
        // A single fragment always plays, whatever the index: the common shipped case.
        for index in 0..50usize {
            assert_eq!(fragment_index(index, 1), Some(0), "index {index}");
        }
        // More than five would be a fatal "Max entries exceeded" in the exe, but the
        // arithmetic itself is just a modulo, so check the shape at 2 and at 6.
        assert_eq!(fragment_index(0, 2), Some(0));
        assert_eq!(fragment_index(1, 2), Some(1));
        assert_eq!(fragment_index(2, 2), Some(0));
        assert_eq!(fragment_index(5, 6), Some(5));
        assert_eq!(fragment_index(6, 6), Some(0));
        // The dividend is 32-bit in the exe (`MOV EAX,[ESP+0x8]` then `DIV dword ptr`),
        // so a 64-bit `usize::MAX` is truncated before the modulo. `u32::MAX % 5` is 0,
        // where the 64-bit answer would have been 3: the wrapper must follow the exe.
        assert_eq!(fragment_index(usize::MAX, 5), Some(u32::MAX as usize % 5));
        assert_eq!(fragment_index_u32(u32::MAX, 5), Some(u32::MAX % 5));
        assert_eq!(fragment_index_u32(u32::MAX, 5), Some(0));
        assert_eq!(
            fragment_index_u32(u32::MAX - 1, 5),
            Some((u32::MAX - 1) % 5)
        );
        // A 64-bit dividend that happens to fit in 32 bits is unaffected, so the
        // truncation is invisible for every value the exe can actually hold.
        for index in 0..1000usize {
            assert_eq!(
                fragment_index(index, 5),
                fragment_index_u32(index as u32, 5).map(|i| i as usize)
            );
        }
    }

    #[test]
    fn a_repeated_slot_line_adds_an_alternative_and_never_replaces() {
        // The parser side of the same rule. Five `STAND` lines in one file give five
        // alternatives in file order, and the modulo cycles through all of them. The
        // refuted "a later fragment replaces the slot" reading would leave one entry.
        let five = Fragment::parse(&format!(
            "version 1\n{}",
            (0..5)
                .map(|i| format!("STAND filename = \"a{i}.anim\",\n"))
                .collect::<String>()
        ));
        let clips = five.get("STAND").unwrap();
        assert_eq!(clips.len(), 5, "five lines must give five alternatives");
        for (i, c) in clips.iter().enumerate() {
            assert_eq!(c.filename, format!("a{i}.anim"), "file order at {i}");
        }
        // The whole cycle is a permutation, so every alternative really is reachable.
        for index in 0..5 {
            let c = five.fragment("STAND", index).expect("an alternative");
            assert_eq!(c.filename, format!("a{index}.anim"), "index {index}");
            assert!(fragment_index(index, 5).unwrap() < clips.len());
        }
        let reached: std::collections::BTreeSet<&str> = (0..25)
            .map(|i| five.fragment("STAND", i).unwrap().filename.as_str())
            .collect();
        assert_eq!(
            reached,
            ["a0.anim", "a1.anim", "a2.anim", "a3.anim", "a4.anim"]
                .into_iter()
                .collect(),
            "every alternative must be reachable and none may be shadowed"
        );
        // Wrapping returns to the first alternative, it does not fall back to "replace".
        assert_eq!(
            five.fragment("STAND", 5).unwrap().filename,
            "a0.anim",
            "index 5 of 5 wraps to the first alternative"
        );
        assert_eq!(five.fragment("STAND", 7).unwrap().filename, "a2.anim");
        // Different counts, same rule.
        assert_eq!(Fragment::parse(BASE).get("STAND").unwrap().len(), 2);
        let two = Fragment::parse(BASE);
        assert_eq!(
            two.fragment("STAND", 0).unwrap().filename,
            "Animations/A/Stand.anim"
        );
        assert_eq!(
            two.fragment("STAND", 1).unwrap().filename,
            "Animations/A/Stand_alt1.anim"
        );
        assert_eq!(
            two.fragment("STAND", 2).unwrap().filename,
            "Animations/A/Stand.anim"
        );
        // A cancelled slot has no fragments at all, so the modulo has no divisor: the
        // exe never reaches the DIV because its predicate is clear (see
        // `fragment_index_with_fallback`).
        let cancelled = Fragment::parse("version 1\nIDLE_1 cancel\n");
        assert!(cancelled.get("IDLE_1").unwrap().is_empty());
        assert_eq!(cancelled.fragment("IDLE_1", 0), None);
        assert_eq!(fragment_index(3, 0), None);
        // An absent slot, too.
        assert_eq!(two.fragment("NOPE", 0), None);
    }

    #[test]
    fn the_slot_resolver_falls_back_to_the_base_group() {
        // `0x00E5F760` tests the group's own count and, when it is 0, divides by group 0's
        // count and reads group 0's handles -- so a slot with no fragments of its own
        // still plays the base fragment instead of nothing.
        // The slot has fragments: its own count is the divisor.
        assert_eq!(fragment_index_with_fallback(3, 1, 7), Some(1));
        assert_eq!(fragment_index_with_fallback(1, 1, 9), Some(0));
        // The slot's count is clear -> group 0's count is used, and group 0's handles.
        assert_eq!(fragment_index_with_fallback(0, 5, 7), Some(2));
        assert_eq!(fragment_index_with_fallback(0, 1, 4), Some(0));
        // Both clear is the divide the exe does not survive.
        assert_eq!(fragment_index_with_fallback(0, 0, 3), None);
        // Group 0 is never the fallback for itself: its clear count is the fault case.
        assert_eq!(fragment_index(3, 0), None);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)] // the layout facts are all constants
    fn has_clip_predicate_matches_the_shipped_table() {
        // Read out of Napoleon.exe 1.3 at `0x013AEBE0`, stride 0x18, offset +0x14, over
        // all 864 entries: 708 are 1 and 156 are 0, and no other value occurs. The two
        // census numbers are a strong regression check on the whole name-table decode.
        let set = (0..ACTION_SLOT_COUNT).filter(|s| has_clip(*s)).count();
        let clear = (0..ACTION_SLOT_COUNT).filter(|s| !has_clip(*s)).count();
        assert_eq!(set, SLOTS_WITH_CLIP);
        assert_eq!(clear, SLOTS_WITHOUT_CLIP_COUNT);
        assert_eq!(set, 708, "the shipped set-flag census");
        assert_eq!(clear, 156, "the shipped clear-flag census");
        assert_eq!(set + clear, ACTION_SLOT_COUNT);
        assert_eq!(
            SLOTS_WITH_CLIP + SLOTS_WITHOUT_CLIP_COUNT,
            ACTION_SLOT_COUNT
        );

        // Spot-check individual slots against the measured table, and against what the
        // names should be: the transitions and the gait family are clear, the poses,
        // idles, walks, fires, deaths and knock-downs are set.
        for (slot, expected) in [
            (0usize, true), // MISSING_ANIM
            (1, true),      // STAND
            (3, false),     // STAND_TO_WALK
            (4, true),      // STAND_TO_COMBAT_READY
            (10, false),    // WALK_1
            (15, false),    // WALK_TO_STAND
            (45, false),    // STEP_LEFT
            (46, false),    // STEP_RIGHT
            (47, true),     // TURN_LEFT_45
            (52, true),     // TURN_RIGHT_180
            (53, false),    // TURN_LEFT_TO_WALK_45  (the six 0x006618D0 scans)
            (58, false),    // TURN_RIGHT_TO_WALK_180
            (59, true),     // STAND_TRAINED
            (126, true),    // AIM
            (130, true),    // FIRE
            (137, false),   // KNEEL_RELOAD
            (149, false),   // STAND_IDLE_11_TELESCOPE
            (229, true),    // COMBAT_1
            (311, true),    // COMBAT_83
            (787, false),   // CLIMB_UP_LADDER
            (804, false),   // CLIMB_DOWN_GRAPPLE
            (805, true),    // KNOCKBACK_1
            (815, false),   // KNOCKED_FLYING_1
            (816, true),    // DEATH_KNOCKED_FLYING_LANDED_1
            (862, true),    // PIKEMEN_CHARGED_FRONT_THRUST_1
            (863, true),    // PIKEMEN_CHARGED_FRONT_THRUST_2
        ] {
            assert_eq!(has_clip(slot), expected, "slot {slot} {}", SLOT_NAMES[slot]);
        }
        // The six slots `0x006618D0` scans, 53..58, are the `TURN_*_TO_WALK_*` turns and
        // all six are clear in the shipped table, which is why that facing resolver has to
        // walk past them.
        for (i, name) in SLOT_NAMES[53..=58].iter().enumerate() {
            let slot = 53 + i;
            assert!(!has_clip(slot), "facing slot {slot} {name}");
            assert!(name.contains("TO_WALK"), "slot {slot} {name}");
        }

        // The ranges are sorted, disjoint and inside the table, and they are exactly the
        // clear set -- otherwise the binary search in `has_clip` would lie.
        let mut prev_hi: Option<u32> = None;
        let mut counted = 0usize;
        for &(lo, hi) in SLOTS_WITHOUT_CLIP.iter() {
            assert!(lo <= hi, "range {lo}..{hi}");
            assert!(
                (hi as usize) < ACTION_SLOT_COUNT,
                "range {lo}..{hi} is off the table"
            );
            if let Some(p) = prev_hi {
                assert!(lo > p, "range {lo}..{hi} overlaps or is unsorted after {p}");
            }
            prev_hi = Some(hi);
            counted += (hi - lo + 1) as usize;
            for slot in lo..=hi {
                assert!(!has_clip(slot as usize), "slot {slot} in a clear range");
            }
        }
        assert_eq!(counted, SLOTS_WITHOUT_CLIP_COUNT);
        // Anything off the end of the table has no descriptor, so no clip.
        for slot in ACTION_SLOT_COUNT..ACTION_SLOT_COUNT + 8 {
            assert!(!has_clip(slot), "slot {slot} is past the table");
        }
        assert!(!has_clip(usize::MAX));

        // Geometry: the predicate is the last dword of a 6-dword group, and the group
        // size is what `0x00E5F760` strides by.
        assert_eq!(DESCRIPTOR_BYTES, 0x18);
        assert_eq!(DESCRIPTOR_BYTES, 6 * 4);
        assert_eq!(descriptor_field::HAS_CLIP, 0x14);
        assert_eq!(descriptor_field::INDEX, 0x00);
        assert_eq!(descriptor_field::NAME, 0x04);
        assert_eq!(descriptor_field::HAS_CLIP + 4, DESCRIPTOR_BYTES);
        // `0x00E5F760` reads the count as the DIV divisor and the five handles as the five
        // dwords before it, so the predicate and the fragments share the group.
        assert_eq!(descriptor_field::HAS_CLIP / 4, 5);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)] // the layout facts are all constants
    fn a_slot_block_holds_five_alternatives_and_their_count() {
        // The static layout: five 0x1C fragments plus the u32 count the modulo divides by.
        // The runtime layout: five clip handles plus the count, in six dwords. Same
        // "how many, then index by modulo" shape, and the count is what `0x00E5F760`
        // reads at `group + 0x14`.
        assert_eq!(
            MAX_FRAGMENTS_PER_SLOT * FRAGMENT_BYTES + 4,
            ACTION_SLOT_BYTES
        );
        // The count sits immediately after the fifth fragment, and the tag is the first
        // dword of a fragment, so the fifth fragment's tag is 0x70 into the block.
        assert_eq!(
            ACTION_SLOT_BYTES - 4,
            MAX_FRAGMENTS_PER_SLOT * FRAGMENT_BYTES
        );
        assert_eq!(
            (MAX_FRAGMENTS_PER_SLOT - 1) * FRAGMENT_BYTES + fragment_field::TAG,
            ACTION_SLOT_BYTES - 4 - FRAGMENT_BYTES
        );
        // The count cannot exceed the five fragments it counts.
        assert!(MAX_FRAGMENTS_PER_SLOT <= (ACTION_SLOT_BYTES - 4) / FRAGMENT_BYTES);
        // The descriptor table's stride is its own constant and is unrelated to the
        // fragment record size: do not let them be confused for each other.
        assert_eq!(FRAGMENT_BYTES, 0x1C);
        assert_ne!(DESCRIPTOR_BYTES, FRAGMENT_BYTES);
        assert_ne!(DESCRIPTOR_BYTES, ACTION_SLOT_BYTES);
    }

    const TABLES: &str = "version 1\r\nanimation_table rider_sabre\r\n{\r\n\tskeleton_type\tman\r\n\tfragment\tbase_fragment\tdefault_equipment_display = primary_weapon, ambient\r\n\tfragment\tover_fragment\r\n//\tfragment\tgone_fragment\r\n\tmount_table\tmount_horse\r\n}\r\nanimation_table mount_horse\r\n{\r\n\tskeleton_type\thorse\r\n\tfragment\thorse_fragment\r\n}\r\n";

    const BASE: &str = "version 1\n\nSTAND\t\tfilename = \"Animations/A/Stand.anim\",\nSTAND\t\tfilename = \"Animations/A/Stand_alt1.anim\", blend_in_time = 0.5\nIDLE_1  filename = \"Animations/A/Idle.anim\", blend_in_time = 0.25 equipment_usage = sword\n// IDLE_2 filename = \"x\"\nMOUNT filename = \"Animations/A/Mount.anim\" primary_weapon = off, ambient = on\n";

    const OVER: &str = "version 1\nIDLE_1 cancel\nWALK_1 filename = \"Animations/B/Walk.anim\"\n";

    fn tables() -> AnimationTables {
        AnimationTables::from_text(TABLES, |n| match n {
            "base_fragment" => Some(BASE.into()),
            "over_fragment" => Some(OVER.into()),
            _ => None,
        })
    }

    #[test]
    fn parses_tables() {
        let t = parse_tables(TABLES);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].name, "rider_sabre");
        assert_eq!(t[0].skeleton_type, "man");
        assert_eq!(t[0].fragments.len(), 2);
        assert_eq!(
            t[0].fragments[0].equipment_display,
            vec!["primary_weapon", "ambient"]
        );
        assert!(t[0].fragments[1].equipment_display.is_empty());
        assert_eq!(t[0].mount_table.as_deref(), Some("mount_horse"));
    }

    #[test]
    fn parses_fragment_lines() {
        let f = Fragment::parse(BASE);
        let stand = f.get("STAND").unwrap();
        assert_eq!(stand.len(), 2);
        assert_eq!(stand[1].filename, "Animations/A/Stand_alt1.anim");
        assert_eq!(stand[1].blend_in_time, Some(0.5));
        let idle = &f.get("IDLE_1").unwrap()[0];
        assert_eq!(idle.blend_in_time, Some(0.25));
        assert!(
            idle.attributes
                .iter()
                .any(|(k, v)| k == "equipment_usage" && v == "sword")
        );
        let mount = &f.get("MOUNT").unwrap()[0];
        assert!(
            mount
                .attributes
                .iter()
                .any(|(k, v)| k == "primary_weapon" && v == "off")
        );
        assert!(f.get("IDLE_2").is_none());
    }

    #[test]
    fn table_files_compose_by_last_file_and_cancel_drops_a_slot() {
        // NOTE the scope: this is the *table-level* composition of the several fragment
        // files one `animation_table` lists, which is our own PROVISIONAL convenience and
        // not an exe rule. It used to be called "later_fragment_overrides_and_cancels"
        // and shared its wording with the refuted "a later fragment replaces the slot"
        // claim, which was about a *repeated slot line inside one file* and is now
        // REFUTED -- see `fragments_of_one_slot_are_alternatives_chosen_by_modulo`.
        // Within one file nothing overrides anything; see the test above.
        let t = tables();
        // `over_fragment` cancels IDLE_1, which `base_fragment` had filled.
        assert!(t.resolve("rider_sabre", "IDLE_1").is_empty());
        // STAND is only in `base_fragment`, and it keeps BOTH of its alternatives.
        let s = t.resolve("RIDER_SABRE", "STAND");
        assert_eq!(s.len(), 2);
        assert_eq!(
            s.iter()
                .map(|c| c.clip.filename.clone())
                .collect::<Vec<_>>(),
            vec!["Animations/A/Stand.anim", "Animations/A/Stand_alt1.anim"]
        );
        assert_eq!(s[0].equipment_display, vec!["primary_weapon", "ambient"]);
        assert_eq!(
            t.resolve("rider_sabre", "WALK_1")[0].fragment,
            "over_fragment"
        );
        assert_eq!(t.resolve_first("rider_sabre", &["NOPE", "WALK_1"]).len(), 1);
        assert_eq!(t.missing_fragments(), vec!["horse_fragment".to_owned()]);
    }

    // -- round 12: the resolver as the exe runs it, over the runtime array --------------

    /// A fragment file in the shape `0x00E5F760`'s group-0 fallback needs: slot 0
    /// (`MISSING_ANIM`) is the *first* line, so it is group 0 with two alternatives, and
    /// `FIRE` is cancelled so it has to fall back. Every slot name is a real action slot,
    /// because `runtime_table` resolves names through `action_slot`.
    fn five_alternatives() -> Fragment {
        Fragment::parse(
            "version 1\n\
             MISSING_ANIM filename = \"m1.anim\",\n\
             MISSING_ANIM filename = \"m2.anim\",\n\
             STAND filename = \"a1.anim\",\n\
             STAND filename = \"a2.anim\",\n\
             STAND filename = \"a3.anim\",\n\
             STAND filename = \"a4.anim\",\n\
             STAND filename = \"a5.anim\",\n\
             AIM filename = \"i1.anim\",\n\
             AIM filename = \"i2.anim\",\n\
             FIRE cancel\n",
        )
    }

    #[test]
    fn the_runtime_resolver_is_the_modulo_over_the_groups_count() {
        // `0x00E5F760` in full, as data: five handles and a count per slot, and
        // `resolve` is its one unsigned `DIV`. Slot = group, selector = the dividend.
        let mut table = RuntimeClipTable::with_slots(4);
        table.set_group(1, SlotGroup::new(&[10, 11, 12, 13, 14]));
        let stand = action_slot("STAND").expect("STAND is a slot");

        // The modulo, over the whole cycle and past it.
        for selector in 0..100u32 {
            assert_eq!(
                table.resolve(stand, selector),
                Some(10 + selector % 5),
                "selector {selector}"
            );
        }
        // count = 1: the single clip always plays, whatever the selector.
        table.set_group(1, SlotGroup::new(&[77]));
        for selector in [0u32, 1, 2, 3, 4, 5, 999, u32::MAX] {
            assert_eq!(table.resolve(stand, selector), Some(77), "sel {selector}");
        }
        // count = 2: the cycle is exactly two.
        table.set_group(1, SlotGroup::new(&[20, 21]));
        for selector in 0..8u32 {
            assert_eq!(table.resolve(stand, selector), Some(20 + selector % 2));
        }
        // count larger than the selector: the dividend comes back unchanged, because
        // `index % count == index` whenever `index < count`. This is the shipped case --
        // most slots have one clip and every selector picks it.
        table.set_group(1, SlotGroup::new(&[30, 31, 32, 33, 34]));
        for selector in 0..5u32 {
            assert_eq!(table.resolve(stand, selector), Some(30 + selector));
        }
        // A group with 3 of 5 slots filled cycles over 3, not over 5.
        table.set_group(1, SlotGroup::new(&[40, 41, 42]));
        for selector in 0..6u32 {
            assert_eq!(table.resolve(stand, selector), Some(40 + selector % 3));
        }
    }

    #[test]
    fn the_runtime_resolver_falls_back_to_group_zero() {
        // The `TEST ECX,ECX` / `JNZ` at `0x00E5F776`: a slot with no fragments of its own
        // divides by **group 0's** count and returns one of group 0's handles. So the
        // fallback changes both the divisor and the handles, not just the divisor.
        let mut table = RuntimeClipTable::with_slots(4);
        let stand = action_slot("STAND").expect("STAND is a slot");
        table.set_group(0, SlotGroup::new(&[1, 2, 3])); // group 0: three alternatives
        table.set_group(1, SlotGroup::new(&[90, 91]));
        assert!(table.has_clip(stand));

        // The slot's own count wins when it is set.
        assert_eq!(table.resolve(stand, 0), Some(90));
        assert_eq!(table.resolve(stand, 1), Some(91));
        assert_eq!(table.resolve(stand, 2), Some(90));

        // Now clear the slot: the predicate goes false and group 0 answers instead.
        table.set_group(stand, SlotGroup::EMPTY);
        assert!(!table.has_clip(stand));
        assert_eq!(table.group(stand).unwrap().count, 0);
        for selector in 0..6u32 {
            assert_eq!(
                table.resolve(stand, selector),
                Some(1 + selector % 3),
                "group-0 fallback, selector {selector}"
            );
        }
        // ...and the fallback reads group 0's *handles*, not the slot's dead ones: the
        // slot's stale 90/91 must never come back.
        assert_ne!(table.resolve(stand, 0), Some(90));

        // Both clear is the divide the exe does not survive.
        table.set_group(0, SlotGroup::EMPTY);
        assert_eq!(table.resolve(stand, 3), None);
        assert_eq!(table.resolve(0, 3), None);
        // A table with no group 0 at all cannot answer either.
        assert_eq!(RuntimeClipTable::with_slots(0).resolve(0, 1), None);
    }

    #[test]
    fn the_runtime_resolver_refuses_what_the_exe_would_only_fault_or_overrun_on() {
        // The exe has no bounds check of any kind here: `0x00E5F760` indexes
        // `A + 0x14 + slot*0x18` and `A + rem*4` unguarded, which is why every caller
        // gates on `has_clip` first. We return `None` instead of inventing an answer.
        let mut table = RuntimeClipTable::with_slots(2);
        let stand = action_slot("STAND").unwrap();
        table.set_group(stand, SlotGroup::new(&[5]));
        assert_eq!(table.resolve(stand, 3), Some(5));
        for slot in [2usize, 3, 5, ACTION_SLOT_COUNT, usize::MAX] {
            assert_eq!(table.resolve(slot, 0), None, "slot {slot} is off the table");
            assert!(!table.has_clip(slot), "slot {slot}");
            assert_eq!(table.group(slot), None);
        }
        // A count larger than the five handles a group can hold cannot come from a
        // well-formed file, and the exe would read past the end of the group.
        let mut bad = SlotGroup::new(&[1, 2, 3, 4, 5]);
        bad.count = 6;
        // `5 % 6 == 5`, which is past the five handles the group holds.
        assert_eq!(bad.resolve(&SlotGroup::EMPTY, 5), None);
        assert_eq!(bad.resolve(&SlotGroup::EMPTY, 0), Some(1));
        // `6 % 6 == 0`, so the oversized count wraps back into the handles for every
        // other dividend -- which is exactly why the guard cannot be a `count <= 5` check
        // at the call site: only the residue tells you.
        assert_eq!(bad.resolve(&SlotGroup::EMPTY, 6), Some(1));
        // The guard is on the *residue*, not on the count, so an oversized count is only
        // refused for the dividends whose residue lands past the five handles. That is
        // every sixth dividend: 5, 11, 17, ...
        for selector in [
            0u32,
            1,
            2,
            3,
            4,
            6,
            7,
            8,
            9,
            10,
            0x7FFF_FFFF,
            u32::MAX - 1,
            u32::MAX,
        ] {
            let want = selector % 6;
            let got = bad.resolve(&SlotGroup::EMPTY, selector);
            if (want as usize) < MAX_FRAGMENTS_PER_SLOT {
                assert_eq!(got, Some(want + 1), "selector {selector} -> {want}");
            } else {
                assert_eq!(got, None, "selector {selector} -> {want} is past the group");
            }
        }
        assert_eq!(bad.resolve(&SlotGroup::EMPTY, 5), None);
        assert_eq!(bad.resolve(&SlotGroup::EMPTY, 17), None);
        // `SlotGroup::new` ignores a sixth handle rather than growing the group, so a file
        // asking for more than the exe allows loses the extra one rather than overflowing
        // a fixed-size array.
        let six = SlotGroup::new(&[1, 2, 3, 4, 5, 6]);
        assert_eq!(six.count, MAX_FRAGMENTS_PER_SLOT as u32);
        assert_eq!(six.handles, [1, 2, 3, 4, 5]);
        assert_eq!(SlotGroup::EMPTY.count, 0);
        assert!(!SlotGroup::EMPTY.has_clip());
    }

    #[test]
    fn a_fragment_file_becomes_a_runtime_clip_table() {
        // The end-to-end bridge: a parsed fragment file -> the 864-slot runtime array ->
        // the resolver -> a clip. This is the chain `0x006631A0` / `0x00E5F760` run, with
        // our 1-based alternative numbers standing in for the exe's clip pointers.
        let f = five_alternatives();
        let table = f.runtime_table();
        assert_eq!(table.len(), ACTION_SLOT_COUNT);
        let stand = action_slot("STAND").unwrap();
        let aim = action_slot("AIM").unwrap();
        let fire = action_slot("FIRE").unwrap();

        // Five alternatives -> a group of five, and the modulo walks all of them. Handles
        // are numbered across the whole file in the order it gave the clips, so group 0
        // (`MISSING_ANIM`, the first line) owns 1..2, `STAND` owns 3..7 and `AIM` 8..9.
        assert_eq!(table.group(0).unwrap().handles, [1, 2, 0, 0, 0]);
        assert_eq!(table.group(0).unwrap().count, 2);
        assert_eq!(table.group(stand).unwrap().count, 5);
        assert_eq!(table.group(stand).unwrap().handles, [3, 4, 5, 6, 7]);
        assert!(table.has_clip(stand));
        for selector in 0..5u32 {
            let handle = table.resolve(stand, selector).expect("a clip");
            let clip = f.handle_clip(handle).expect("handle -> clip");
            assert_eq!(clip.filename, format!("a{}.anim", selector + 1));
        }
        // Two alternatives cycle over two, and their handles are the later ones.
        assert_eq!(table.group(aim).unwrap().count, 2);
        assert_eq!(table.group(aim).unwrap().handles, [8, 9, 0, 0, 0]);
        assert_eq!(table.resolve(aim, 0), Some(8));
        assert_eq!(table.resolve(aim, 1), Some(9));
        assert_eq!(table.resolve(aim, 2), Some(8));
        assert_eq!(f.handle_clip(8).unwrap().filename, "i1.anim");
        assert_eq!(f.handle_clip(9).unwrap().filename, "i2.anim");
        // A slot the file cancels has no group at all and falls back to group 0 -- which
        // here is `MISSING_ANIM`'s two clips. This is the whole `0x00E5F760` rule in one
        // call: empty own group -> divide by group 0's count, read group 0's handles.
        assert!(!table.has_clip(fire));
        assert_eq!(table.group(fire).unwrap(), &SlotGroup::EMPTY);
        assert_eq!(table.resolve(fire, 0), Some(1));
        assert_eq!(table.resolve(fire, 1), Some(2));
        assert_eq!(table.resolve(fire, 2), Some(1));
        assert_eq!(
            f.handle_clip(table.resolve(fire, 1).unwrap())
                .unwrap()
                .filename,
            "m2.anim"
        );
        // An unknown slot name in the file is skipped, not misfiled.
        let odd = Fragment::parse("NO_SUCH_SLOT_ANYWHERE filename = \"x.anim\",\n");
        assert_eq!(odd.runtime_table().resolve(0, 0), None);
        // Handles are 1-based, so handle 0 never names a clip -- that is what makes 0 the
        // exe's "empty" marker.
        assert_eq!(f.handle_clip(0), None);
        assert_eq!(f.handle_clip(10), None);
        // `runtime_group` and `runtime_table` must agree, count for count.
        assert_eq!(f.runtime_group("STAND"), *table.group(stand).unwrap());
        assert_eq!(f.runtime_group("AIM"), *table.group(aim).unwrap());
        assert_eq!(f.runtime_group("MISSING_ANIM"), *table.group(0).unwrap());
        assert_eq!(f.runtime_group("FIRE"), SlotGroup::EMPTY);
        // And the runtime copy is a *different* predicate from the shipped name table:
        // `FIRE` (slot 130) has a shipped clip but no line in this file, and `MISSING_ANIM`
        // has both.
        assert!(has_clip(fire));
        assert!(!table.has_clip(fire));
        assert!(has_clip(0));
        assert!(table.has_clip(0));
    }

    #[test]
    fn the_transition_table_is_seventy_three_states_wide_and_picks_weighted() {
        // `0x00817BB0`: `idx = from * 0x49 + to`, one 0x1C block per pair, the count in
        // the first dword and three 8-byte `{weight, action_slot}` entries after it.
        assert_eq!(STATE_COUNT, 73);
        assert_eq!(STATE_COUNT, 0x49);
        assert_eq!(TRANSITION_BLOCK_BYTES, 0x1C);
        assert_eq!(TRANSITION_ENTRIES_PER_BLOCK, 3);
        assert_eq!(TRANSITION_BLOCK_BYTES, 4 + TRANSITION_ENTRIES_PER_BLOCK * 8);
        assert_eq!(transition_index(0, 0), Some(0));
        assert_eq!(transition_index(0, 1), Some(1));
        assert_eq!(transition_index(1, 0), Some(73));
        assert_eq!(transition_index(72, 72), Some(72 * 73 + 72));
        assert_eq!(transition_index(72, 72), Some(5328));
        // The exe indexes unguarded, so out-of-range states are ours to refuse.
        assert_eq!(transition_index(73, 0), None);
        assert_eq!(transition_index(0, 73), None);
        assert_eq!(transition_index(u32::MAX, u32::MAX), None);

        // The "no transition" answer: the static pair `{0, 0x360}` at `0x01454398`.
        assert_eq!(NO_TRANSITION_SLOT, EMPTY_FRAGMENT_TAG);
        assert_eq!(NO_TRANSITION_SLOT, ACTION_SLOT_COUNT as u32);

        // The weighted pick: `((seed >> 16) * count) / 0xFFFF` clamped to `count - 1`,
        // over the MSVC LCG. Deterministic, so assert the whole sequence.
        assert_eq!(pick_weighted(0, &mut 1), None);
        let mut seed = 0u32;
        let picks: Vec<usize> = (0..16)
            .map(|_| pick_weighted(3, &mut seed).unwrap())
            .collect();
        assert!(picks.iter().all(|p| *p < 3), "{picks:?}");
        // Reproduce the exe's arithmetic independently: same LCG, same multiply-shift.
        let mut want = 0u32;
        let expect: Vec<usize> = (0..16)
            .map(|_| {
                want = want.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
                let n = ((want >> 16) as u64 * 3) / 0xFFFF;
                n.min(2) as usize
            })
            .collect();
        assert_eq!(picks, expect);
        // The seed advances even when the count is 1, and a single entry always wins.
        let mut one = 7u32;
        for _ in 0..8 {
            assert_eq!(pick_weighted(1, &mut one), Some(0));
        }
        assert_eq!(one, expect_one_seed(7));
        // The clamp exists because `(seed >> 16) * count` can exceed 0xFFFF; a count of
        // 0xFFFF+ can never produce an out-of-range pick.
        let mut s = 1u32;
        for _ in 0..64 {
            let p = pick_weighted(0x7FFF_FFFF, &mut s).unwrap();
            assert!(p < 0x7FFF_FFFF);
        }
    }

    fn expect_one_seed(start: u32) -> u32 {
        let mut s = start;
        for _ in 0..8 {
            s = s.wrapping_mul(0x343FD).wrapping_add(0x269EC3);
        }
        s
    }

    #[test]
    fn the_ten_named_states_are_idle_and_wounded_ranges() {
        // Read out of the exe at `0x014520B0`: 46 eight-byte `{first_slot, count}` pairs,
        // of which the stance driver `0x006631A0`'s switch names exactly ten.
        assert_eq!(STANCE_CLIP_RANGES.len(), 10);
        assert_eq!(STANCE_TABLE_ENTRIES, 46);
        // Every range must be real slots and inside the table, and the state ids are the
        // exe's: 0..7 plus 60 and 61.
        let mut states: Vec<u32> = STANCE_CLIP_RANGES.iter().map(|(s, _, _)| *s).collect();
        states.sort_unstable();
        assert_eq!(states, vec![0, 1, 2, 3, 4, 5, 6, 7, 60, 61]);
        for &(state, first, count) in STANCE_CLIP_RANGES.iter() {
            assert!(count > 0, "state {state} has no clips");
            assert!(count <= MAX_FRAGMENTS_PER_SLOT as u32 * 4, "state {state}");
            let first = first as usize;
            let last = first + count as usize - 1;
            assert!(last < ACTION_SLOT_COUNT, "state {state} runs off the table");
            assert_eq!(stance_clip_range(state), Some((first, count as usize)));
            // The ranges are named, so the names must be real slots and the family must
            // read as one family (a shared `_IDLE_n` or a shared prefix).
            assert!(SLOT_NAMES[first].contains("IDLE"), "state {state}");
            assert!(
                SLOT_NAMES[last].starts_with(
                    SLOT_NAMES[first]
                        .rsplit_once('_')
                        .map_or(SLOT_NAMES[first], |(head, _)| head)
                ) || SLOT_NAMES[first].starts_with(SLOT_NAMES[last].split('_').next().unwrap()),
                "state {state}: {} .. {}",
                SLOT_NAMES[first],
                SLOT_NAMES[last]
            );
        }
        // The eight standing idles, by name -- this is the whole point of reading the
        // table: these are the names of the states the exe plays.
        let expect = [
            (0u32, "STAND_IDLE_1"),
            (1, "STAND_ALT_1_IDLE_1"),
            (2, "STAND_ALT_2_IDLE_1"),
            (3, "STAND_ALT_3_IDLE_1"),
            (4, "STAND_TRAINED_IDLE_1"),
            (5, "CROUCH_IDLE_1"),
            (6, "STAND_FOR_STOKE_IDLE_1"),
            (7, "STAND_NO_WEAPON_IDLE_1"),
            (60, "WOUNDED_FRONT_IDLE"),
            (61, "WOUNDED_BACK_IDLE"),
        ];
        for (state, first_name) in expect {
            let (first, count) = stance_clip_range(state).expect("a named state");
            assert_eq!(SLOT_NAMES[first], first_name, "state {state}");
            assert!(count >= 4, "state {state} has only {count} clips");
            // Every slot in the range is one of the cycle's clips, i.e. a real slot that
            // resolves back to itself.
            for (i, name) in SLOT_NAMES[first..first + count].iter().enumerate() {
                assert!(!name.is_empty());
                assert_eq!(action_slot(name), Some(first + i));
            }
        }
        // The other 63 states have no clip range. They still exist -- that is what makes
        // the transition table 73 wide -- so this must be `None`, not an error.
        for state in 0..STATE_COUNT {
            if !states.contains(&state) {
                assert_eq!(stance_clip_range(state), None, "state {state}");
                assert!(transition_index(state, state).is_some(), "state {state}");
            }
        }
    }

    #[test]
    fn the_runtime_array_is_populated_by_appending_in_file_order() {
        // The store `0x00E60260` performs, and the geometry it does it over. The outer
        // bound is `CMP EAX,0x1e600` over `0x90` strides; the group stride is `0x18`.
        assert_eq!(LOAD_IMAGE_BYTES, 0x1E600);
        assert_eq!(LOAD_IMAGE_BYTES, ACTION_SLOT_COUNT * ACTION_SLOT_BYTES);
        assert_eq!(ACTION_SLOT_COUNT * ACTION_SLOT_BYTES, 864 * 0x90);
        assert_eq!(SLOT_BLOCK_FRAGMENT_COUNT, 0x8C);
        assert_eq!(
            SLOT_BLOCK_FRAGMENT_COUNT + 4,
            ACTION_SLOT_BYTES,
            "the count is the last dword of the block"
        );
        assert_eq!(
            MAX_FRAGMENTS_PER_SLOT * FRAGMENT_BYTES,
            SLOT_BLOCK_FRAGMENT_COUNT,
            "five fragments, then the count"
        );
        // The whole runtime array is 864 groups of `DESCRIPTOR_BYTES`, the `0x5100` the
        // resolver `0x00E5F760` indexes and the post-pass `0x00E61160` walks (`ADD EBX,0x18`
        // / `CMP EBX,0x5100`).
        assert_eq!(ACTION_SLOT_COUNT * DESCRIPTOR_BYTES, 0x5100);
        assert_eq!(RuntimeClipTable::new().len(), ACTION_SLOT_COUNT);

        // Append, in order: three alternatives give `[h0, h1, h2, 0, 0]` and count 3, and
        // the modulo then walks all three in file order.
        let block = SlotBlock::new(&[7, 8, 9]);
        let group = populate_slot_group(&block);
        assert_eq!(group.count, 3);
        assert_eq!(group.handles, [7, 8, 9, 0, 0]);
        assert!(group.has_clip());
        assert_eq!(group.resolve(&SlotGroup::EMPTY, 0), Some(7));
        assert_eq!(group.resolve(&SlotGroup::EMPTY, 1), Some(8));
        assert_eq!(group.resolve(&SlotGroup::EMPTY, 2), Some(9));
        assert_eq!(group.resolve(&SlotGroup::EMPTY, 3), Some(7));

        // The loop's bound is the block's `count`, not `handles.len()`: handles past the
        // count are never stored. This is the one place `populate_slot_group` differs from
        // `SlotGroup::new`, and it is what a stale image would look like.
        let mut stale = SlotBlock::new(&[1, 2, 3]);
        stale.count = 2;
        assert_eq!(
            populate_slot_group(&stale),
            SlotGroup::new(&[1, 2]),
            "the third handle is past the count the loop iterates"
        );

        // A zeroed block is the "no clip" case: count 0, and the group is EMPTY, which is
        // what hands `0x00E5F760` to group 0.
        assert_eq!(populate_slot_group(&SlotBlock::EMPTY), SlotGroup::EMPTY);
        assert!(!populate_slot_group(&SlotBlock::EMPTY).has_clip());
        assert_eq!(SlotBlock::default(), SlotBlock::EMPTY);

        // A sixth alternative cannot come from a well-formed file (the exe aborts), and
        // the block refuses to hold it rather than overrunning five handles.
        let six = SlotBlock::new(&[1, 2, 3, 4, 5, 6]);
        assert_eq!(six.count, MAX_FRAGMENTS_PER_SLOT as u32);
        assert_eq!(populate_slot_group(&six).handles, [1, 2, 3, 4, 5]);
        // A count past five is the overrun the exe would take on; we clamp.
        let mut over = SlotBlock::new(&[1, 2, 3, 4, 5]);
        over.count = 9;
        assert_eq!(
            populate_slot_group(&over).count,
            MAX_FRAGMENTS_PER_SLOT as u32
        );

        // The whole-array driver walks every slot, filled or not.
        let mut image = vec![SlotBlock::EMPTY; ACTION_SLOT_COUNT];
        image[0] = SlotBlock::new(&[10, 11]);
        image[7] = SlotBlock::new(&[20]);
        let table = populate_runtime_table(&image);
        assert_eq!(table.len(), ACTION_SLOT_COUNT);
        assert!(table.has_clip(0));
        assert!(table.has_clip(7));
        assert_eq!(table.group(0).unwrap().handles, [10, 11, 0, 0, 0]);
        assert_eq!(table.resolve(0, 1), Some(11));
        assert_eq!(table.resolve(7, 0), Some(20));
        // Slot 8 is one of the 156 the shipped table marks clear, and it takes the
        // documented group-0 fallback rather than failing.
        assert!(!table.has_clip(8));
        assert_eq!(table.resolve(8, 0), Some(10));
        assert_eq!(table.resolve(8, 1), Some(11));
        // A short image leaves the tail empty, which is ours to allow: the exe has no
        // bound and would read past the end.
        let short = populate_runtime_table(&[SlotBlock::new(&[1])]);
        assert_eq!(short.len(), ACTION_SLOT_COUNT);
        assert!(!short.has_clip(ACTION_SLOT_COUNT - 1));
        // A long one is truncated at 864.
        let long = vec![SlotBlock::new(&[1]); ACTION_SLOT_COUNT + 8];
        assert_eq!(populate_runtime_table(&long).len(), ACTION_SLOT_COUNT);
    }

    #[test]
    fn the_population_loop_is_what_a_fragment_file_becomes() {
        // `Fragment::runtime_table` is now literally `populate_runtime_table` over
        // `Fragment::slot_image`, so the two must agree count for count, and the image
        // must be the exe's shape: 864 blocks, five handles each, the count in the block.
        let f = five_alternatives();
        let image = f.slot_image();
        assert_eq!(image.len(), ACTION_SLOT_COUNT);
        assert!(
            image
                .iter()
                .all(|b| b.count <= MAX_FRAGMENTS_PER_SLOT as u32)
        );

        let stand = action_slot("STAND").unwrap();
        let aim = action_slot("AIM").unwrap();
        let fire = action_slot("FIRE").unwrap();
        assert_eq!(image[stand].count, 5);
        assert_eq!(image[stand].handles, [3, 4, 5, 6, 7]);
        assert_eq!(image[aim].count, 2);
        assert_eq!(image[aim].handles, [8, 9, 0, 0, 0]);
        assert_eq!(image[fire].count, 0);
        assert_eq!(image[fire], SlotBlock::EMPTY);

        assert_eq!(f.runtime_table(), populate_runtime_table(&image));
        // `runtime_group` (one slot) is the same store applied to one block.
        for slot in [0usize, stand, aim, fire] {
            assert_eq!(
                f.runtime_group(SLOT_NAMES[slot]),
                populate_slot_group(&image[slot]),
                "slot {slot}"
            );
        }
        // An unknown slot name consumes a handle number but fills no group, so the
        // numbering `handle_clip` counts still lines up.
        let odd = Fragment::parse("NO_SUCH_SLOT_ANYWHERE filename = \"x.anim\",\n");
        assert!(odd.slot_image().iter().all(|b| *b == SlotBlock::EMPTY));
        assert_eq!(odd.handle_clip(1).unwrap().filename, "x.anim");
    }

    #[test]
    fn every_shipped_slot_resolves_through_the_populated_array() {
        // Task: tie the pure functions to the data actually decoded. Walk all 864
        // `SLOT_NAMES` entries, fill a group for every slot the shipped descriptor table
        // marks `HAS_CLIP` (708 of them) and leave the other 156 empty, then resolve each
        // slot the way the exe does and check the answer is the right one in both cases.
        assert_eq!(SLOT_NAMES.len(), ACTION_SLOT_COUNT);
        assert_eq!(
            SLOTS_WITH_CLIP + SLOTS_WITHOUT_CLIP_COUNT,
            ACTION_SLOT_COUNT
        );

        // Build the image the way a loaded record would: one alternative per slot the
        // shipped table says has a clip. The handle is the slot's own index + 1, so a
        // resolved handle names the slot it came from.
        let mut image = vec![SlotBlock::EMPTY; ACTION_SLOT_COUNT];
        for (slot, name) in SLOT_NAMES.iter().enumerate() {
            assert!(!name.is_empty(), "slot {slot} has no name");
            assert_eq!(action_slot(name), Some(slot), "{name}");
            if has_clip(slot) {
                image[slot] = SlotBlock::new(&[slot as u32 + 1]);
            }
        }
        let table = populate_runtime_table(&image);
        assert_eq!(table.len(), ACTION_SLOT_COUNT);

        let mut filled = 0usize;
        let mut empty = 0usize;
        for slot in 0..ACTION_SLOT_COUNT {
            // The runtime predicate agrees with the shipped one for this image, which is
            // what ties `has_clip` (transcribed from `0x013AEBE0`) to the store.
            assert_eq!(table.has_clip(slot), has_clip(slot), "slot {slot}");
            if has_clip(slot) {
                filled += 1;
                let group = table.group(slot).unwrap();
                assert_eq!(group.count, 1, "slot {slot}");
                assert_eq!(group.handles[0], slot as u32 + 1, "slot {slot}");
                // One alternative, so every selector picks it.
                for selector in [0u32, 1, 2, 7, 0xFFFF_FFFF] {
                    assert_eq!(
                        table.resolve(slot, selector),
                        Some(slot as u32 + 1),
                        "slot {slot} selector {selector}"
                    );
                }
            } else {
                empty += 1;
                assert_eq!(table.group(slot).unwrap(), &SlotGroup::EMPTY, "slot {slot}");
                // The documented fallback: an empty own group divides by group 0's count
                // and reads group 0's handles. Group 0 (`MISSING_ANIM`) has a clip, so
                // this is a real answer, not the divide the exe does not survive.
                let base = table.group(0).unwrap();
                assert!(base.has_clip(), "group 0 must have the base clip");
                for selector in [0u32, 1, 2, 0xFFFF_FFFF] {
                    let handle = table
                        .resolve(slot, selector)
                        .unwrap_or_else(|| panic!("slot {slot} selector {selector}"));
                    assert_eq!(
                        handle,
                        base.handles[selector as usize % base.count as usize]
                    );
                }
            }
        }
        assert_eq!(filled, SLOTS_WITH_CLIP);
        assert_eq!(empty, SLOTS_WITHOUT_CLIP_COUNT);

        // Group 0 is itself filled here, so the 156 all share its single handle; if the
        // name table ever marked slot 0 clear, the fallback would be the divide the exe
        // does not survive and every one of the 156 must read `None` instead. Assert that
        // edge explicitly so the fallback's precondition is locked, not assumed.
        let mut headless = vec![SlotBlock::EMPTY; ACTION_SLOT_COUNT];
        for (slot, block) in headless.iter_mut().enumerate() {
            if has_clip(slot) && slot != 0 {
                *block = SlotBlock::new(&[slot as u32 + 1]);
            }
        }
        let headless = populate_runtime_table(&headless);
        assert!(!headless.has_clip(0));
        assert!(headless.resolve(0, 0).is_none(), "group 0 empty: no divide");
        for slot in SLOTS_WITHOUT_CLIP.iter().flat_map(|&(lo, hi)| lo..=hi) {
            let slot = slot as usize;
            if slot == 0 {
                continue;
            }
            assert!(!headless.has_clip(slot));
            assert_eq!(headless.resolve(slot, 0), None, "slot {slot}");
        }
    }

    #[test]
    fn the_pose_code_reaches_sixty_three_states_and_names_only_ten() {
        // `0x00663730` is a pure `switch (entity + 0x1D8) { entity + 0x1B8 = imm; }`
        // over 73 poses, and it reaches 63 distinct states -- round 12's "63 unnamed
        // states" counted the other way.
        assert_eq!(POSE_COUNT, 73);
        assert_eq!(POSE_STATE.len(), POSE_COUNT as usize);
        let mut states: Vec<u32> = POSE_STATE.to_vec();
        states.sort_unstable();
        states.dedup();
        assert_eq!(states.len(), 63, "the map must reach 63 distinct states");
        assert_eq!(states[0], 0);
        assert_eq!(*states.last().unwrap(), 82);
        // Every pose answers, and every answer is a real state id. Ten of the 63 land
        // *above* the 73-wide transition table, which `0x006631A0` indexes with no range
        // check at all -- so this is a measured inconsistency, not something to assert
        // away. Lock the split instead: 62 of the 73 poses land inside the transition
        // table and 11 land outside it.
        let mut inside = 0usize;
        let mut outside = 0usize;
        for pose in 0..POSE_COUNT {
            let state = pose_state(pose).unwrap_or_else(|| panic!("pose {pose}"));
            assert!(state != POSE_LEAVES_STATE);
            if transition_index(state, state).is_some() {
                inside += 1;
            } else {
                assert!(state >= STATE_COUNT, "pose {pose} -> state {state}");
                outside += 1;
            }
        }
        assert_eq!(inside, 62);
        assert_eq!(outside, 11);
        // The out-of-range states are exactly 73..82, ten of them, reached by eleven poses:
        // state 75 is the only one two poses share, which is where 63 states come from 73
        // poses together with state 44 (poses 68 and 69).
        let mut tall: Vec<u32> = POSE_STATE
            .iter()
            .copied()
            .filter(|&s| s >= STATE_COUNT)
            .collect();
        tall.sort_unstable();
        assert_eq!(tall, vec![73, 74, 75, 75, 76, 77, 78, 79, 80, 81, 82]);
        assert_eq!(pose_state(POSE_COUNT), None);
        assert_eq!(pose_state(u32::MAX), None);
        // Poses are 0..72 and no state is left unset by the switch.
        assert!(POSE_STATE.iter().all(|&s| s != POSE_LEAVES_STATE));

        // The eight standing idle states are poses 0..7 in a permuted order; the wounded
        // pair are poses 37 and 38. Composing the two CONFIRMED tables is the only naming
        // the exe supports.
        assert_eq!(STANCE_STATE_POSES.len(), 8);
        for (i, &(state, first, _)) in STANCE_CLIP_RANGES.iter().take(8).enumerate() {
            let pose = STANCE_STATE_POSES[i];
            assert_eq!(pose_state(pose), Some(state), "pose {pose}");
            assert!(SLOT_NAMES[first as usize].contains("IDLE"));
        }
        // Poses 0..7 are exactly the eight idle states, once each -- that is what makes
        // the permutation meaningful rather than a coincidence.
        let mut idle: Vec<u32> = (0..8).map(|p| pose_state(p).unwrap()).collect();
        idle.sort_unstable();
        assert_eq!(idle, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        for (i, &(state, _, _)) in STANCE_CLIP_RANGES.iter().skip(8).enumerate() {
            assert_eq!(pose_state(WOUNDED_STATE_POSES[i]), Some(state));
        }

        // What the map does NOT give: a name. Nothing here ties a state to a string, so
        // the other 63 stay UNKNOWN and must not be invented. Lock that down: of the 63
        // states the map reaches, exactly ten are the ones §3 can name.
        let mut named: Vec<u32> = STANCE_CLIP_RANGES.iter().map(|(s, _, _)| *s).collect();
        named.sort_unstable();
        let reached_and_named = states.iter().filter(|s| named.contains(s)).count();
        assert_eq!(reached_and_named, 10);
        assert_eq!(states.len() - reached_and_named, 53);
    }
}

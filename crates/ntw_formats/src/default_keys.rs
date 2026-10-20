//! `text/default_keys.xml`: the original's key bindings ("shortcuts"), and the bindings one game
//! mode gives a UI component. CONFIRMED from the data and the exe's loader unless tagged; evidence
//! in `analysis/fidelity/UI_FIDELITY.md` §5.1 "Shortcut keys".
//!
//! The file (CONFIRMED, vanilla `data.pack`): `<shortcuts version>` holds the game modes
//! (`<mode>battle</mode>`, ...), then keysets `<keyset name>` → `<category name>` → `<group name
//! order>` → `<func name order repeating hidden ignore_lock>` → up to two `<key
//! modifier>KEYNAME</key>`, then `<mappings>` → `<mode name>` → `<func name component>`: in that
//! mode the func's shortcut goes to the UI component of that name (`root`, `button_end_turn`,
//! `camera`, ...).
//!
//! The exe (handler built by `0x00E8EBF0`, file read by `0x00ED5C20` from `0x00EDB080`):
//! - Key names are the exe's DirectInput key names (table `0x0139FEC8`, 143 entries from `ESCAPE`
//!   to `MEDIASELECT`), compared case-sensitively (`0x00E20290`, `CheckUniStringEqualsAnsi`); an
//!   unknown name gets code 0x90, which no key ever reports, so it never matches. A func reads at
//!   most two `<key>` children (`0x00E8BB00`).
//! - The modifier names are the [`Modifier`] values (table `0x013BC824`, `0x00EEC620`); a missing
//!   or unknown one is `NONE`.
//! - A mapping is kept only for a mode the file declares in its `<mode>` list (`0x00EA9CD0`
//!   writes only a mode entry the func already has) and is applied to the func of that name in
//!   every keyset (`0x00EA9D50`).
//! - The keysets go into a hash map keyed by name ([`keyset_order`]); without a player's
//!   `custom_keys` file the active keyset is a copy of the first keyset the map iterates, which
//!   in vanilla is `fps` (`0x00EDB080`).
//! - Setting a game mode (`0x00ED5640`: `campaign` from `0x008B7FE0`, `battle` from
//!   `0x00546C20`, `frontend` at `0x00463B1E`) binds every func whose component for the mode is
//!   a registered component; the `all` mode's component wins over the mode's own when both are
//!   set (binding constructor `0x00E8B760`, rebind `0x00EDB540`).

use crate::xml::{XmlElement, XmlError, parse_bytes};

/// The file's path in the data.
pub const PATH: &str = "text/default_keys.xml";

/// The mode whose mappings apply in every game mode (CONFIRMED string, `0x00ED5640`).
pub const ALL_MODES: &str = "all";

/// A key's modifier rule (`modifier` attribute; CONFIRMED names and order, table `0x013BC824`).
/// What each one asks of the CTRL / SHIFT / ALT keys held is [`Modifier::matches`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Modifier {
    #[default]
    None,
    Ctrl,
    Shift,
    Alt,
    Any,
    AllowShift,
    AllowAlt,
    AllowCtrl,
    ShiftOrCtrl,
    ShiftOrAlt,
    CtrlOrAlt,
}

/// The modifier names in the exe's order (index = the exe's value).
const MODIFIER_NAMES: [(&str, Modifier); 11] = [
    ("NONE", Modifier::None),
    ("CTRL", Modifier::Ctrl),
    ("SHIFT", Modifier::Shift),
    ("ALT", Modifier::Alt),
    ("ANY", Modifier::Any),
    ("ALLOW_SHIFT", Modifier::AllowShift),
    ("ALLOW_ALT", Modifier::AllowAlt),
    ("ALLOW_CTRL", Modifier::AllowCtrl),
    ("SHIFT_OR_CTRL", Modifier::ShiftOrCtrl),
    ("SHIFT_OR_ALT", Modifier::ShiftOrAlt),
    ("CTRL_OR_ALT", Modifier::CtrlOrAlt),
];

/// The modifier keys held (CONFIRMED in `0x00EC53A0`: CTRL = LCONTROL or RCONTROL, SHIFT = LSHIFT
/// or RSHIFT, ALT = LALT or RALT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Held {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Modifier {
    /// The `modifier` attribute's value; a missing or unknown name is `NONE` (CONFIRMED,
    /// `0x00EEC620` returns 0 when no name matches).
    pub fn parse(name: Option<&str>) -> Self {
        name.and_then(|n| MODIFIER_NAMES.iter().find(|(s, _)| *s == n)).map_or(Self::None, |(_, m)| *m)
    }

    /// True if the modifier keys held satisfy the rule (CONFIRMED, `0x00EC53A0`). A single
    /// modifier ignores the others: `CTRL` matches CTRL+ALT and CTRL+SHIFT too.
    pub fn matches(self, h: Held) -> bool {
        let none = !(h.ctrl || h.shift || h.alt);
        match self {
            Self::None => none,
            Self::Ctrl => h.ctrl,
            Self::Shift => h.shift,
            Self::Alt => h.alt,
            Self::Any => true,
            Self::AllowShift => h.shift || none,
            Self::AllowAlt => h.alt || none,
            Self::AllowCtrl => h.ctrl || none,
            Self::ShiftOrCtrl => h.ctrl || h.shift,
            Self::ShiftOrAlt => h.alt || h.shift,
            Self::CtrlOrAlt => h.alt || h.ctrl,
        }
    }
}

/// One `<key>` of a func.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySlot {
    /// The exe's key name (`S`, `ESCAPE`, ...), as written.
    pub key: String,
    pub modifier: Modifier,
}

impl KeySlot {
    /// True if a press of `key` with these modifiers held fires this slot.
    pub fn matches(&self, key: &str, held: Held) -> bool {
        self.key == key && self.modifier.matches(held)
    }
}

/// A `<func>`: a named action and its keys (at most two, the exe's two slots).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Func {
    pub name: String,
    pub keys: Vec<KeySlot>,
}

/// A `<keyset>`: its funcs in file order (categories and groups flattened).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyset {
    pub name: String,
    pub funcs: Vec<Func>,
}

/// A `<mappings>` entry: in `mode`, `func` goes to the component named `component`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    pub mode: String,
    pub func: String,
    pub component: String,
}

/// The parsed file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shortcuts {
    /// `version` (the exe accepts a player's `custom_keys` file only of the same version).
    pub version: i64,
    /// The declared game modes (`<mode>` list).
    pub modes: Vec<String>,
    /// The keysets in file order (a repeated name is dropped: the hash map keeps the first).
    pub keysets: Vec<Keyset>,
    /// The mappings, for declared modes only.
    pub mappings: Vec<Mapping>,
}

/// A shortcut one component gets in one mode: the func's name and keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub func: String,
    pub keys: Vec<KeySlot>,
}

/// The bindings of one component in one mode ([`Shortcuts::bindings`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bindings(pub Vec<Binding>);

impl Bindings {
    /// The funcs a press of `key` (the exe's key name) fires with these modifiers held, in file
    /// order. No allocation.
    pub fn fired<'a>(&'a self, key: &'a str, held: Held) -> impl Iterator<Item = &'a str> + 'a {
        self.0.iter().filter(move |b| b.keys.iter().any(|k| k.matches(key, held))).map(|b| b.func.as_str())
    }
}

/// The exe's string hash (`0x0045C1A0`, CONFIRMED): seed 0xF73A51C9, the UTF-16 units from the
/// last to the first, each XORed in shifted left by 0, 8, 16, 24, 0, ... bits.
pub fn name_hash(name: &str) -> u32 {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut h = 0xF73A_51C9u32;
    let mut shift = 0u32;
    for &u in units.iter().rev() {
        h ^= u32::from(u) << shift;
        shift = (shift + 8) & 31;
    }
    h
}

/// The order the exe's keyset map iterates the names in (CONFIRMED, insert in `0x00ED5C20`,
/// rehash `0x008E73D0`, iterator `0x00461120`): 4 buckets to start; a name goes to the tail of
/// bucket `hash % buckets` (a name already there is not added again); when the count passes the
/// bucket count (maximum load 1.0) the buckets double first, the nodes moving in bucket order, each
/// to the tail of its new bucket; iteration walks the buckets in index order, each front to back.
/// Returns indices into `names`.
pub fn keyset_order(names: &[&str]) -> Vec<usize> {
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); 4];
    let mut count = 0usize;
    for (i, name) in names.iter().enumerate() {
        let h = name_hash(name);
        let at = (h as usize) % buckets.len();
        if buckets[at].iter().any(|&j| names[j] == *name) {
            continue;
        }
        count += 1;
        if count > buckets.len() {
            let n = buckets.len() * 2;
            let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
            for j in buckets.into_iter().flatten() {
                next[(name_hash(names[j]) as usize) % n].push(j);
            }
            buckets = next;
        }
        let n = buckets.len();
        buckets[(h as usize) % n].push(i);
    }
    buckets.into_iter().flatten().collect()
}

impl Shortcuts {
    /// Parses the file's bytes (UTF-8 or UTF-16 with a byte-order mark).
    pub fn parse(bytes: &[u8]) -> Result<Self, XmlError> {
        let root = parse_bytes(bytes)?;
        if root.name != "shortcuts" {
            return Err(XmlError(format!("root element is <{}>, not <shortcuts>", root.name)));
        }
        let version = root.attr_i64("version").unwrap_or(0);
        let modes: Vec<String> = root.children_named("mode").map(|m| m.text.clone()).collect();
        let mut keysets: Vec<Keyset> = Vec::new();
        for ks in root.children_named("keyset") {
            let name = ks.attr("name").unwrap_or_default().to_owned();
            if keysets.iter().any(|k| k.name == name) {
                continue;
            }
            let mut funcs = Vec::new();
            for f in ks.children_named("category").flat_map(|c| c.children_named("group")).flat_map(|g| g.children_named("func")) {
                funcs.push(parse_func(f));
            }
            keysets.push(Keyset { name, funcs });
        }
        let mut mappings = Vec::new();
        if let Some(m) = root.child("mappings") {
            for mode in m.children_named("mode") {
                let mode_name = mode.attr("name").unwrap_or_default();
                if !modes.iter().any(|d| d == mode_name) {
                    continue;
                }
                for f in mode.children_named("func") {
                    mappings.push(Mapping {
                        mode: mode_name.to_owned(),
                        func: f.attr("name").unwrap_or_default().to_owned(),
                        component: f.attr("component").unwrap_or_default().to_owned(),
                    });
                }
            }
        }
        Ok(Self { version, modes, keysets, mappings })
    }

    /// The keyset the exe uses without a player's `custom_keys` file: the first its keyset map
    /// iterates ([`keyset_order`]); `fps` in vanilla.
    pub fn default_keyset(&self) -> Option<&Keyset> {
        let names: Vec<&str> = self.keysets.iter().map(|k| k.name.as_str()).collect();
        keyset_order(&names).first().map(|&i| &self.keysets[i])
    }

    /// The component a func goes to in `mode`: the `all` mode's if set, else the mode's own (the
    /// last mapping of a mode wins, as each one overwrites the func's entry). Empty if none.
    pub fn component(&self, mode: &str, func: &str) -> &str {
        let of = |m: &str| self.mappings.iter().rev().find(|x| x.mode == m && x.func == func).map_or("", |x| x.component.as_str());
        let all = of(ALL_MODES);
        if all.is_empty() { of(mode) } else { all }
    }

    /// The shortcuts of the default keyset that go to `component` in `mode`, in file order.
    pub fn bindings(&self, mode: &str, component: &str) -> Bindings {
        let Some(ks) = self.default_keyset() else { return Bindings::default() };
        Bindings(
            ks.funcs
                .iter()
                .filter(|f| self.component(mode, &f.name) == component)
                .map(|f| Binding { func: f.name.clone(), keys: f.keys.clone() })
                .collect(),
        )
    }
}

/// A `<func>`: its name and its first two `<key>` children (a third is ignored, `0x00E8BB00`).
fn parse_func(f: &XmlElement) -> Func {
    let keys = f
        .children_named("key")
        .take(2)
        .filter(|k| !k.text.is_empty())
        .map(|k| KeySlot { key: k.text.clone(), modifier: Modifier::parse(k.attr("modifier")) })
        .collect();
    Func { name: f.attr("name").unwrap_or_default().to_owned(), keys }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<shortcuts version = "32">
  <mode>battle</mode>
  <mode>campaign</mode>
  <mode>all</mode>
  <keyset name="fps">
    <category name="campaign">
      <group name ="game_play" order="2">
        <func name="quick_save" order="4" ignore_lock="1">
          <key modifier ="CTRL">S</key>
        </func>
        <func name="quick_load" order="5">
          <key modifier ="CTRL">F7</key>
          <key modifier ="SHIFT">L</key>
          <key>X</key>
        </func>
        <func name="show_lists" order="6">
          <key>L</key>
        </func>
        <func name="escape_menu" hidden="1">
          <key modifier ="ANY">ESCAPE</key>
        </func>
      </group>
    </category>
  </keyset>
  <keyset name="total_war">
    <category name="campaign">
      <group name="game_play" order="2">
        <func name="quick_save"><key modifier="ALT">Q</key></func>
      </group>
    </category>
  </keyset>
  <mappings>
    <mode name="campaign">
      <func name="quick_save" component="root"/>
      <func name="quick_load" component="root"/>
      <func name="show_lists" component="button_lists"/>
    </mode>
    <mode name="all">
      <func name="escape_menu" component="root"/>
    </mode>
    <mode name="naval">
      <func name="show_lists" component="root"/>
    </mode>
  </mappings>
</shortcuts>"#;

    #[test]
    fn modifier_rules_match_the_exe() {
        let h = |ctrl, shift, alt| Held { ctrl, shift, alt };
        assert!(Modifier::None.matches(h(false, false, false)));
        assert!(!Modifier::None.matches(h(false, true, false)));
        // A single modifier ignores the others (0x00EC53A0): CTRL+ALT still matches CTRL.
        assert!(Modifier::Ctrl.matches(h(true, false, true)));
        assert!(!Modifier::Ctrl.matches(h(false, false, true)));
        assert!(Modifier::Any.matches(h(true, true, true)));
        assert!(Modifier::AllowShift.matches(h(false, false, false)));
        assert!(Modifier::AllowShift.matches(h(false, true, false)));
        assert!(!Modifier::AllowShift.matches(h(true, false, false)));
        assert!(Modifier::ShiftOrCtrl.matches(h(true, false, false)));
        assert!(!Modifier::ShiftOrCtrl.matches(h(false, false, true)));
        assert!(Modifier::CtrlOrAlt.matches(h(false, false, true)));
        assert_eq!(Modifier::parse(Some("ALLOW_CTRL")), Modifier::AllowCtrl);
        assert_eq!(Modifier::parse(Some("ctrl")), Modifier::None, "names are case-sensitive");
        assert_eq!(Modifier::parse(None), Modifier::None);
    }

    #[test]
    fn vanilla_keyset_order_puts_fps_first() {
        // "fps" hashes to bucket 2 and "total_war" to bucket 3 of the first 4.
        assert_eq!(name_hash("fps") % 4, 2);
        assert_eq!(name_hash("total_war") % 4, 3);
        assert_eq!(keyset_order(&["total_war", "fps"]), vec![1, 0]);
        assert_eq!(keyset_order(&["fps", "fps"]), vec![0]);
    }

    #[test]
    fn keyset_order_rehash_keeps_bucket_order() {
        // Five names pass 4 buckets: the map doubles to 8 and the order follows the new buckets.
        let names = ["a", "b", "c", "d", "e"];
        let order = keyset_order(&names);
        let mut by_bucket: Vec<usize> = (0..names.len()).collect();
        by_bucket.sort_by_key(|&i| (name_hash(names[i]) % 8, i));
        assert_eq!(order, by_bucket);
    }

    #[test]
    fn bindings_from_the_file() {
        let s = Shortcuts::parse(DOC.as_bytes()).unwrap();
        assert_eq!(s.version, 32);
        assert_eq!(s.default_keyset().unwrap().name, "fps");
        // The naval mapping is dropped: "naval" is not a declared mode.
        assert_eq!(s.component("naval", "show_lists"), "");
        assert_eq!(s.component("campaign", "escape_menu"), "root");
        let root = s.bindings("campaign", "root");
        let funcs: Vec<&str> = root.0.iter().map(|b| b.func.as_str()).collect();
        assert_eq!(funcs, ["quick_save", "quick_load", "escape_menu"]);
        // Two key slots at most: the third <key> (X) is ignored.
        assert_eq!(root.0[1].keys.len(), 2);
        let none = Held::default();
        let ctrl = Held { ctrl: true, ..Held::default() };
        assert_eq!(root.fired("S", ctrl).collect::<Vec<_>>(), ["quick_save"]);
        assert_eq!(root.fired("S", none).count(), 0);
        // Rebound: quick_load is CTRL+F7 or SHIFT+L here, not CTRL+L.
        assert_eq!(root.fired("F7", ctrl).collect::<Vec<_>>(), ["quick_load"]);
        assert_eq!(root.fired("L", ctrl).count(), 0);
        assert_eq!(root.fired("X", none).count(), 0);
        assert_eq!(root.fired("ESCAPE", ctrl).collect::<Vec<_>>(), ["escape_menu"]);
        // show_lists (L) goes to its button, not the root.
        assert!(s.bindings("campaign", "button_lists").fired("L", none).eq(["show_lists"]));
    }

    #[test]
    fn rejects_other_files() {
        assert!(Shortcuts::parse(b"<credits/>").is_err());
    }
}

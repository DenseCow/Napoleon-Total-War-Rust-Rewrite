//! Mod activation: `user.script.txt`, mod packs, loose files and our `mods\` folder.
//!
//! The full write-up, with the exe addresses, is `analysis/mods/MOD_LOADING.md`. In short,
//! [`plan_layers`] mounts, in this order (CONFIRMED in `Napoleon.exe`, §2 of the notes):
//!
//! 1. the boot pack, then release, movie and patch packs found in the working directories
//!    (`data\` first), each group in the original's pack-scan hash-map order ([`scan_slot_order`]);
//! 2. the loose files of the working directories;
//! 3. the packs named by `mod` lines in script order (or, with `import_all_mods`, every mod-type
//!    pack of the scan);
//! 4. our own `mods\` folder (packs and loose folders), first entry of `load_order.txt` first.
//!
//! Mount order only breaks ties: which copy of a file wins is decided by [`Vfs::beats`] (the
//! precedence graph of `set_pack_file_precedence` / `set_pack_file_dependency` between two of its
//! packs, else header type first, then the first mounted). Nothing here is fatal for a mod: a missing or broken mod
//! pack is skipped with a warning.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use super::vfs::{LayerKind, LooseDir, effective_language, language_of, lower_file_name};
use super::{PackError, PackFile, PackGraph, PackType, PairResult, Vfs};

/// The name of the original's mod script inside its `scripts` folder.
pub const USER_SCRIPT_NAME: &str = "user.script.txt";
/// The optional load-order file inside our `mods\` folder.
pub const LOAD_ORDER_NAME: &str = "load_order.txt";

/// Every top-level `*.pack` in a folder, in the order `FindFirstFileW("*.pack")` returns them on
/// NTFS (upper-cased names ascending), which is the order the original's pack scan meets them
/// (`VFS_ScanPackFiles` 0x01095100).
pub fn list_packs(dir: &Path) -> Result<Vec<PathBuf>, PackError> {
    let mut out = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let path = item?.path();
        if path.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pack")) {
            out.push(path);
        }
    }
    out.sort_by_cached_key(|p| p.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default());
    Ok(out)
}

/// The original's pack-scan hash: djb2 over the UTF-16 units of the lowercase file name
/// (5381, ×33 + unit, wrapping), masked to 31 bits (`VFS_PackScanMapInsert` 0x010970D0, CONFIRMED).
fn pack_name_hash(lower_name: &str) -> u32 {
    lower_name.encode_utf16().fold(5381u32, |h, u| h.wrapping_mul(33).wrapping_add(u32::from(u))) & 0x7fff_ffff
}

/// The order in which the original walks its pack-scan map: names (lowercase file names, in scan
/// order) are inserted into an open-addressing table of 71 slots (`VFS_Construct` 0x01051340) with
/// linear probing and wrap; only when no slot is free the table grows to 2n+1 and re-inserts the old
/// slots in order (0x010970D0). Iteration is slot order (CONFIRMED). Returns indexes into `names`.
pub fn scan_slot_order(names: &[String]) -> Vec<usize> {
    fn place(slots: &mut [Option<usize>], hash: u32, item: usize) -> bool {
        let cap = slots.len();
        let start = hash as usize % cap;
        match (start..cap).chain(0..start).find(|&s| slots[s].is_none()) {
            Some(s) => {
                slots[s] = Some(item);
                true
            }
            None => false,
        }
    }
    let hashes: Vec<u32> = names.iter().map(|n| pack_name_hash(n)).collect();
    let mut slots: Vec<Option<usize>> = vec![None; 71];
    for item in 0..names.len() {
        while !place(&mut slots, hashes[item], item) {
            let old: Vec<usize> = slots.iter().flatten().copied().collect();
            slots = vec![None; slots.len() * 2 + 1];
            for o in old {
                place(&mut slots, hashes[o], o);
            }
        }
    }
    slots.into_iter().flatten().collect()
}

/// How a script file's bytes were decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    /// UTF-16 little-endian (with or without the `FF FE` BOM). The game itself writes
    /// `preferences.script.txt` this way (CONFIRMED on the user's machine).
    Utf16Le,
    /// UTF-16 big-endian with the `FE FF` BOM.
    Utf16Be,
    /// UTF-8 (with or without the `EF BB BF` BOM), or plain ASCII.
    Utf8,
}

/// Decodes a script file. UTF-16 is detected by its BOM, or (without a BOM) by a zero
/// second byte; everything else is read as UTF-8, replacing invalid bytes (INFERRED: the exe's
/// stream decoding is not traced, MOD_LOADING.md §5; this accepts every form seen in the wild).
pub fn decode_script_text(bytes: &[u8]) -> (String, TextEncoding) {
    let utf16 = |b: &[u8], le: bool| -> String {
        let units: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| if le { u16::from_le_bytes(c) } else { u16::from_be_bytes(c) })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => (utf16(rest, true), TextEncoding::Utf16Le),
        [0xFE, 0xFF, rest @ ..] => (utf16(rest, false), TextEncoding::Utf16Be),
        [0xEF, 0xBB, 0xBF, rest @ ..] => (String::from_utf8_lossy(rest).into_owned(), TextEncoding::Utf8),
        [a, 0, ..] if *a != 0 => (utf16(bytes, true), TextEncoding::Utf16Le),
        _ => (String::from_utf8_lossy(bytes).into_owned(), TextEncoding::Utf8),
    }
}

/// One command from `user.script.txt` that affects file loading. The command names and
/// help texts are CONFIRMED from strings in `Napoleon.exe`; how each one is applied is
/// described in `MOD_LOADING.md` §2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptCommand {
    /// `mod <file name>`: "Add a mod to the startup list".
    Mod(String),
    /// `import_all_mods`: "Import all mod packs".
    ImportAllMods,
    /// `exclude_pack_file <pack file name>`: "Exclude pack file from the virtual file system".
    ExcludePack(String),
    /// `set_pack_file_precedence <lhs> <rhs>`: "lhs will be searched before rhs for files".
    Precedence(String, String),
    /// `set_pack_file_dependency <lhs> <rhs>`: "lhs will be loaded first and used by rhs pack".
    Dependency(String, String),
    /// `add_working_directory <directory name>`: "Redirect the non-pack (and mod pack) file search".
    WorkingDirectory(String),
    /// Any other statement (graphics settings, `batch`, comments, a loading command without its
    /// arguments, ...): the command word. Not used for loading.
    Other(String),
}

/// The loading commands that take arguments, with what they need (for the warning when a line
/// lacks them; such a line parses as [`ScriptCommand::Other`]).
const LOADING_COMMANDS: [(&str, &str); 5] = [
    ("mod", "a pack name"),
    ("exclude_pack_file", "a pack name"),
    ("set_pack_file_precedence", "two pack names"),
    ("set_pack_file_dependency", "two pack names"),
    ("add_working_directory", "a folder"),
];

/// A parsed `user.script.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserScript {
    /// The commands, in file order.
    pub commands: Vec<ScriptCommand>,
    /// How the file was decoded.
    pub encoding: TextEncoding,
}

/// Whitespace of the exe's console tokenizer (table at 0x01466B54 used by 0x010F5D10, CONFIRMED):
/// space, tab, CR, LF, U+00A0, U+200B, U+FFFE, U+2028, U+2029.
fn is_script_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{a0}' | '\u{200b}' | '\u{fffe}' | '\u{2028}' | '\u{2029}')
}

/// Splits one statement into words like the exe's token reader (0x010F5D10, CONFIRMED): a `"`
/// anywhere toggles quoting and is dropped; unquoted whitespace ends a word.
fn tokenize(stmt: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = stmt.chars().peekable();
    loop {
        while chars.next_if(|&c| is_script_space(c)).is_some() {}
        if chars.peek().is_none() {
            return out;
        }
        let (mut word, mut quoted) = (String::new(), false);
        while let Some(c) = chars.next_if(|&c| quoted || !is_script_space(c)) {
            if c == '"' {
                quoted = !quoted;
            } else {
                word.push(c);
            }
        }
        out.push(word);
    }
}

/// Splits a line at `;` outside quotes. INFERRED: the exe's tokenizer has no `;` rule, yet mods
/// ship `mod x.pack;` lines (MOD_LOADING.md §5 names the breakpoint that settles it).
fn statements(line: &str) -> Vec<&str> {
    let (mut out, mut start, mut quoted) = (Vec::new(), 0, false);
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                out.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&line[start..]);
    out
}

impl UserScript {
    /// Parses the raw bytes of a script file (any of the [`TextEncoding`]s).
    pub fn parse(bytes: &[u8]) -> Self {
        let (text, encoding) = decode_script_text(bytes);
        Self { commands: Self::parse_str(&text), encoding }
    }

    /// Parses script text: statements end at a newline or `;`. Command words are case-sensitive,
    /// as in the exe's command map (0x00DB80F0, CONFIRMED). There is no comment syntax: a `#` or
    /// `//` line is just an unknown command, which [`plan_layers`] skips.
    pub fn parse_str(text: &str) -> Vec<ScriptCommand> {
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim_start_matches('\u{feff}');
            for stmt in statements(line) {
                let words = tokenize(stmt);
                let Some(cmd) = words.first() else { continue };
                let arg = |i: usize| words.get(i).filter(|w| !w.is_empty()).cloned();
                let c = match (cmd.as_str(), arg(1), arg(2)) {
                    ("mod", Some(a), _) => ScriptCommand::Mod(a),
                    ("import_all_mods", _, _) => ScriptCommand::ImportAllMods,
                    ("exclude_pack_file", Some(a), _) => ScriptCommand::ExcludePack(a),
                    ("set_pack_file_precedence", Some(a), Some(b)) => ScriptCommand::Precedence(a, b),
                    ("set_pack_file_dependency", Some(a), Some(b)) => ScriptCommand::Dependency(a, b),
                    ("add_working_directory", Some(a), _) => ScriptCommand::WorkingDirectory(a),
                    _ => ScriptCommand::Other(cmd.clone()),
                };
                out.push(c);
            }
        }
        out
    }
}

/// Where the original keeps `user.script.txt`:
/// `%APPDATA%\The Creative Assembly\Napoleon\scripts\user.script.txt` (CONFIRMED: the exe
/// builds it from `The Creative Assembly\`, `scripts\` and `.script.txt`, and the game writes
/// `preferences.script.txt` in the same folder).
pub fn default_user_script_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("The Creative Assembly").join("Napoleon").join("scripts").join(USER_SCRIPT_NAME))
}

/// Which `user.script.txt` to read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UserScriptSetting {
    /// The original's location ([`default_user_script_path`]).
    #[default]
    Default,
    /// A specific file.
    File(PathBuf),
    /// Script text given directly (tests).
    Text(String),
    /// Do not read any user script (no original-style mods).
    None,
}

/// What [`Vfs::open_with_mods`] should load on top of the install.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModOptions {
    /// The `user.script.txt` to obey.
    pub user_script: UserScriptSetting,
    /// Our easy-mod folder (packs and loose folders, plus `load_order.txt`). `None` = off.
    pub mods_dir: Option<PathBuf>,
}

impl ModOptions {
    /// No mods at all: the install alone (what [`Vfs::open_install`] loads when no mods are set).
    pub fn vanilla() -> Self {
        Self { user_script: UserScriptSetting::None, mods_dir: None }
    }
}

/// What was loaded and why. Shown by `napoleon --list-mods`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModReport {
    /// The install's `data` folder.
    pub data_dir: PathBuf,
    /// The `user.script.txt` that was read, if any.
    pub user_script: Option<PathBuf>,
    /// Its encoding.
    pub user_script_encoding: Option<TextEncoding>,
    /// The `mods` folder that was read, if any.
    pub mods_dir: Option<PathBuf>,
    /// Problems that made something get skipped (never fatal).
    pub warnings: Vec<String>,
    /// Informational notes (ignored commands, inactive packs, ...).
    pub notes: Vec<String>,
}

/// One planned layer before mounting.
enum Planned {
    Pack { pack: PackFile, kind: LayerKind },
    Dir { dir: Arc<LooseDir>, kind: LayerKind },
}

/// A resolved load order: the layers in mount order, the precedence graph and the report.
struct Plan {
    layers: Vec<Planned>,
    graph: Arc<PackGraph>,
    report: ModReport,
}

/// Reads `mods\load_order.txt` (or lists the folder) and returns the entries, highest priority first.
fn mods_folder_entries(dir: &Path, report: &mut ModReport) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() || p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pack")))
            .collect(),
        Err(e) => {
            report.notes.push(format!("no mods folder at {} ({e})", dir.display()));
            return Vec::new();
        }
    };
    entries.sort_by_cached_key(|p| lower_file_name(p));
    let order_file = dir.join(LOAD_ORDER_NAME);
    let Ok(bytes) = std::fs::read(&order_file) else {
        if !entries.is_empty() {
            report.notes.push(format!("{} not found: loading every mod in name order", order_file.display()));
        }
        return entries;
    };
    let (text, _) = decode_script_text(&bytes);
    let mut ordered = Vec::new();
    for line in text.lines() {
        let name = line.trim().trim_start_matches('\u{feff}').trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        let want = name.trim_end_matches(['/', '\\']).to_ascii_lowercase();
        match entries.iter().position(|p| lower_file_name(p) == want) {
            Some(i) => ordered.push(entries.remove(i)),
            None => report.warnings.push(format!("{LOAD_ORDER_NAME}: {name:?} is not in {}; skipped", dir.display())),
        }
    }
    for left in entries {
        report.notes.push(format!("{} is not listed in {LOAD_ORDER_NAME}; not loaded", left.display()));
    }
    ordered
}

/// Lowercase file name of a pack named in a script (`a/b/X.pack` -> `x.pack`).
fn script_pack_name(name: &str) -> String {
    name.rsplit(['/', '\\']).next().unwrap_or(name).to_ascii_lowercase()
}

/// The state of resolving `mod` lines (`VFS_MountPack` 0x01082690 per line).
struct ModResolver<'a> {
    work_dirs: &'a [PathBuf],
    /// Mod-type packs of the scan not mounted yet, by lowercase file name.
    inactive: HashMap<String, PackFile>,
    /// Lowercase file names of every pack mounted or planned so far.
    mounted: Vec<String>,
    excludes: &'a [String],
    /// The `set_pack_file_precedence` / `set_pack_file_dependency` graph.
    graph: &'a PackGraph,
    out: Vec<Planned>,
}

impl ModResolver<'_> {
    /// Plans one mod pack (and, first, what it depends on).
    fn add(&mut self, name: &str, report: &mut ModReport, depth: usize) {
        let file = script_pack_name(name);
        if self.mounted.contains(&file) || depth > 32 {
            return; // the exe's VFS_MountPack returns 1, "already mounted"
        }
        if self.excludes.contains(&file) {
            // ORIGINAL BUG: VFS_MountPack returns 2 for an excluded pack and VFS_LoadMods throws
            // "Could not load the mods." (0x01082240), stopping start-up. Fixed: skip it.
            report.warnings.push(format!("mod {name:?}: excluded by exclude_pack_file; skipped"));
            return;
        }
        // What the pack depends on (set_pack_file_dependency, transitively) is mounted first, in
        // the graph's node order (VFS_MountPack 0x01082690 -> 0x0109ECF0, CONFIRMED).
        match self.graph.mount_order(&file, self.excludes) {
            Ok(order) => {
                for d in order.iter().filter(|d| **d != file) {
                    self.add(d, report, depth + 1);
                }
            }
            // ORIGINAL BUG: VFS_MountPack returns 6 when a dependency is excluded and VFS_LoadMods
            // throws "Could not load the mods." (0x01082240). Fixed: skip just this pack.
            Err(dep) => {
                report.warnings.push(format!("mod {name:?}: needs {dep:?}, which exclude_pack_file excludes; skipped"));
                return;
            }
        }
        let pack = match self.inactive.remove(&file) {
            Some(p) => p,
            None => {
                let rel = name.replace('/', "\\");
                let found = self.work_dirs.iter().map(|d| d.join(&rel)).find(|p| p.is_file());
                match found.map(|p| (PackFile::open(&p), p)) {
                    Some((Ok(p), _)) => p,
                    // ORIGINAL BUG: a missing or unreadable mod pack throws "Could not load the
                    // mods." (0x01082240) and the original stops. Fixed: skip it with a warning.
                    Some((Err(e), path)) => {
                        report.warnings.push(format!("mod {name:?} ({}): {e}; skipped", path.display()));
                        return;
                    }
                    None => {
                        report.warnings.push(format!("mod {name:?}: pack not found in the working folders; skipped"));
                        return;
                    }
                }
            }
        };
        for dep in pack.header().dependencies.clone() {
            self.add(&dep, report, depth + 1);
        }
        self.mounted.push(file);
        self.out.push(Planned::Pack { pack, kind: LayerKind::ScriptMod });
    }
}

/// Reads the user script the options name, noting it in the report.
fn read_user_script(options: &ModOptions, report: &mut ModReport) -> Option<UserScript> {
    let path = match &options.user_script {
        UserScriptSetting::None => return None,
        UserScriptSetting::Text(t) => {
            return Some(UserScript { commands: UserScript::parse_str(t), encoding: TextEncoding::Utf8 });
        }
        UserScriptSetting::File(p) => p.clone(),
        UserScriptSetting::Default => default_user_script_path()?,
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            let s = UserScript::parse(&bytes);
            report.user_script = Some(path);
            report.user_script_encoding = Some(s.encoding);
            Some(s)
        }
        Err(e) => {
            report.notes.push(format!("no user script at {} ({e})", path.display()));
            None
        }
    }
}

/// Resolves the load order: the **one place** every loading rule lives (working directories, the
/// pack scan, pack types, language packs, `user.script.txt` commands, loose folders, our `mods\`
/// folder). Evidence for each rule: `analysis/mods/MOD_LOADING.md` §2.
///
/// Only a failure to read `data\` itself is an error. Missing or broken mod packs, a missing
/// `user.script.txt` and unknown commands are reported in the [`ModReport`].
fn plan_layers(data_dir: &Path, language: &str, options: &ModOptions) -> Result<Plan, PackError> {
    let mut report = ModReport { data_dir: data_dir.to_path_buf(), ..Default::default() };
    let script = read_user_script(options, &mut report);

    // 1. The script. Working directories: data\ first, then add_working_directory folders and the
    //    folders of absolute `mod` paths, in script order (VFS_AddWorkingDirectory 0x0105CA70,
    //    VFS_AddModToStartupList 0x0105C4C0). A relative folder is relative to the process folder,
    //    the install root (the folder above `data`). INFERRED: the script runs before the pack scan, so
    //    these folders are scanned too (MOD_LOADING.md §6).
    let root = data_dir.parent().unwrap_or(data_dir);
    let mut work_dirs: Vec<PathBuf> = vec![data_dir.to_path_buf()];
    let add_dir = |dirs: &mut Vec<PathBuf>, p: PathBuf| {
        if !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    };
    let (mut mod_names, mut import_all, mut excludes) = (Vec::new(), false, Vec::new());
    let mut graph = PackGraph::new();
    for c in script.iter().flat_map(|s| &s.commands) {
        match c {
            ScriptCommand::Mod(n) => {
                let p = Path::new(n);
                if p.is_absolute() {
                    if let Some(dir) = p.parent().filter(|d| d.is_dir()) {
                        add_dir(&mut work_dirs, dir.to_path_buf());
                    }
                    mod_names.push(script_pack_name(n));
                } else {
                    mod_names.push(n.clone());
                }
            }
            ScriptCommand::ImportAllMods => import_all = true,
            ScriptCommand::ExcludePack(n) => excludes.push(script_pack_name(n)),
            // Both commands add the ordered pair (lhs, rhs) to the graph, whose node order then
            // decides between its packs: rhs wins (0x0109AA10 / 0x0109A910 -> 0x0108E8F0, CONFIRMED).
            // INFERRED: names are matched like `mod` names (lowercase file name); the exe stores
            // the raw words (MOD_LOADING.md §2.1).
            ScriptCommand::Precedence(a, b) | ScriptCommand::Dependency(a, b) => {
                let (lhs, rhs) = (script_pack_name(a), script_pack_name(b));
                let (word, result) = match c {
                    ScriptCommand::Dependency(..) => ("set_pack_file_dependency", graph.add_dependency(&lhs, &rhs)),
                    _ => ("set_pack_file_precedence", graph.add_precedence(&lhs, &rhs)),
                };
                match result {
                    PairResult::Added => {}
                    PairResult::AlreadyThere => report.notes.push(if matches!(c, ScriptCommand::Dependency(..)) {
                        format!("{word} {a:?} {b:?}: precedence pair already there; the dependency is still recorded")
                    } else {
                        format!("{word} {a:?} {b:?}: repeated; no change")
                    }),
                    PairResult::WouldCloseCycle => report.warnings.push(format!(
                        "{word} {a:?} {b:?}: would put a pack before itself (a cycle); ignored, as in the original"
                    )),
                }
            }
            ScriptCommand::WorkingDirectory(d) => {
                let p = root.join(d.replace('/', "\\"));
                if p.is_dir() {
                    add_dir(&mut work_dirs, p);
                } else {
                    report.warnings.push(format!("add_working_directory {d:?}: no such folder; ignored"));
                }
            }
            // ORIGINAL BUG: the exe's script runner stops at the first command it does not know, so
            // one comment or typo silently drops every later line (0x00DB80F0). Fixed: only this
            // statement is skipped, with a warning (we run only the file-loading commands, so a
            // statement we skip may be one the original runs).
            ScriptCommand::Other(cmd) => {
                let warning = match LOADING_COMMANDS.iter().find(|(word, _)| word == cmd) {
                    Some((word, args)) => format!("user script: {word} needs {args}; skipped"),
                    None => format!("user script: {cmd:?} is not a file-loading command; skipped"),
                };
                report.warnings.push(warning);
            }
        }
    }

    // 2. The pack scan: *.pack of each working directory, first directory wins per lowercase name
    //    (VFS_ScanPackFiles 0x01095100), then the hash-map slot order.
    let mut scan: Vec<(String, PathBuf)> = Vec::new();
    for (i, dir) in work_dirs.iter().enumerate() {
        let packs = match list_packs(dir) {
            Ok(p) => p,
            Err(e) if i == 0 => return Err(e),
            Err(e) => {
                report.warnings.push(format!("{}: {e}; skipped", dir.display()));
                continue;
            }
        };
        for p in packs {
            let name = lower_file_name(&p);
            if !scan.iter().any(|(n, _)| *n == name) {
                scan.push((name, p));
            }
        }
    }
    let names: Vec<String> = scan.iter().map(|(n, _)| n.clone()).collect();
    let (mut boot, mut release, mut movie, mut patch) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut inactive: Vec<(String, PackFile)> = Vec::new();
    for i in scan_slot_order(&names) {
        let (name, path) = &scan[i];
        // Ours: one language's packs only (the original installs one language).
        if language_of(name).is_some_and(|l| l != language) {
            continue;
        }
        if excludes.contains(name) {
            report.notes.push(format!("{}: excluded by exclude_pack_file", path.display()));
            continue;
        }
        match PackFile::open(path) {
            Ok(p) => match p.pack_type() {
                PackType::Boot => boot.push(p),
                PackType::Release => release.push(p),
                PackType::Movie => movie.push(p),
                PackType::Patch => patch.push(p),
                PackType::Mod => inactive.push((name.clone(), p)),
                PackType::Other(t) => report.warnings.push(format!("{}: unknown pack type {t}; skipped", path.display())),
            },
            Err(e) => report.warnings.push(format!("{}: {e}; skipped", path.display())),
        }
    }
    if boot.len() > 1 {
        report.warnings.push(format!(
            "{} boot packs (the original refuses to start: \"Too many boot packs - only one is allowed.\")",
            boot.len()
        ));
    }

    // 3. Boot, release, movie, patch (VFS_InitMountBootPack 0x01062600,
    //    VFS_MountReleaseMoviePatchPacks 0x01084010), then the loose folders.
    let mut layers: Vec<Planned> = Vec::new();
    let mut mounted: Vec<String> = Vec::new();
    for pack in boot.into_iter().chain(release).chain(movie).chain(patch) {
        mounted.push(lower_file_name(pack.path()));
        let kind = LayerKind::from_pack_type(pack.pack_type());
        layers.push(Planned::Pack { pack, kind });
    }
    for (i, dir) in work_dirs.iter().enumerate() {
        match LooseDir::scan(dir) {
            Ok(d) => layers.push(Planned::Dir { dir: Arc::new(d), kind: LayerKind::Loose }),
            Err(e) if i == 0 => return Err(e.into()),
            Err(e) => report.warnings.push(format!("{}: {e}; skipped", dir.display())),
        }
    }

    // 4. The mods (VFS_LoadMods 0x01082240): the `mod` lines in order, or with import_all_mods
    //    every mod-type pack of the scan in slot order (the mod lines are then ignored).
    let mut resolver = ModResolver {
        work_dirs: &work_dirs,
        inactive: HashMap::new(),
        mounted,
        excludes: &excludes,
        graph: &graph,
        out: Vec::new(),
    };
    if import_all {
        if !mod_names.is_empty() {
            report.notes.push("import_all_mods is on: the `mod` lines are ignored (as in the original)".to_owned());
        }
        for (name, pack) in inactive {
            resolver.mounted.push(name);
            resolver.out.push(Planned::Pack { pack, kind: LayerKind::ScriptMod });
        }
    } else {
        resolver.inactive = inactive.into_iter().collect();
        for name in &mod_names {
            resolver.add(name, &mut report, 0);
        }
        let mut left: Vec<&PackFile> = resolver.inactive.values().collect();
        left.sort_by_key(|p| p.path().to_path_buf());
        for p in left {
            report.notes.push(format!("{}: a mod pack with no `mod` line; not loaded (as in the original)", p.path().display()));
        }
    }
    layers.append(&mut resolver.out);

    // 5. Our mods folder, first entry first (it wins ties: equal rank, first mounted).
    if let Some(dir) = &options.mods_dir {
        report.mods_dir = Some(dir.clone());
        for entry in mods_folder_entries(dir, &mut report) {
            if entry.is_dir() {
                match LooseDir::scan(&entry) {
                    Ok(d) => layers.push(Planned::Dir { dir: Arc::new(d), kind: LayerKind::ModsFolder }),
                    Err(e) => report.warnings.push(format!("{}: {e}; skipped", entry.display())),
                }
            } else {
                match PackFile::open(&entry) {
                    Ok(p) => layers.push(Planned::Pack { pack: p, kind: LayerKind::ModsFolder }),
                    Err(e) => report.warnings.push(format!("{}: {e}; skipped", entry.display())),
                }
            }
        }
    }

    Ok(Plan { layers, graph: Arc::new(graph), report })
}

/// One resolved layer of a cached plan: what to mount, without the open pack.
#[derive(Debug, Clone)]
enum CachedLayer {
    Pack { path: PathBuf, kind: LayerKind },
    Dir { dir: Arc<LooseDir>, kind: LayerKind },
}

/// A load order resolved once per (data folder, language, [`ModOptions`]) and reused by every
/// later [`Vfs::open_install`]: the user script is parsed, the packs' headers read and the
/// loose folders scanned once per process (the original also loads its mods once, at start-up).
#[derive(Debug)]
struct CachedPlan {
    layers: Vec<CachedLayer>,
    graph: Arc<PackGraph>,
    /// Mod packs that no longer opened when the plan was mounted again, each warned about once.
    gone: Mutex<Vec<PathBuf>>,
}

impl CachedPlan {
    /// Records that the mod pack at `path` failed to open; true the first time (warn then only).
    fn first_failure(&self, path: &Path) -> bool {
        let mut gone = self.gone.lock().unwrap_or_else(|e| e.into_inner());
        if gone.iter().any(|p| p == path) {
            return false;
        }
        gone.push(path.to_path_buf());
        true
    }
}

/// What a plan is cached under: data folder, language and mod setting.
type PlanKey = (PathBuf, String, ModOptions);

/// One cache entry: filled once under its own lock, so only callers of the same key wait for each
/// other while it is resolved (see [`Vfs::open_install_language`]).
type PlanSlot = Arc<Mutex<Option<Arc<CachedPlan>>>>;

/// The plans resolved (or being resolved) so far. Few entries (one per install and language the
/// process opens); the list's lock is held only to find or add a slot.
static PLANS: Mutex<Vec<(PlanKey, PlanSlot)>> = Mutex::new(Vec::new());

/// Test only: the data folder of every plan resolved, to count how often a miss plans.
#[cfg(test)]
static PLANNED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// The mod setting [`Vfs::open_install`] follows (NapoleonRust's `--mods` / `--no-mods` /
/// `--user-script`). Unset means vanilla.
static MOD_OPTIONS: RwLock<Option<ModOptions>> = RwLock::new(None);

/// Sets (or clears, = vanilla) the mods every later [`Vfs::open_install`] loads. Set it once at
/// start-up, before anything opens the install.
pub fn set_mod_options(options: Option<ModOptions>) {
    if let Ok(mut o) = MOD_OPTIONS.write() {
        *o = options;
    }
}

/// The mods [`Vfs::open_install`] loads ([`ModOptions::vanilla`] if none were set).
pub fn mod_options() -> ModOptions {
    MOD_OPTIONS.read().ok().and_then(|o| o.clone()).unwrap_or_else(ModOptions::vanilla)
}

impl Vfs {
    /// Opens the install **with mods**, like the original game plus our `mods\` folder, using
    /// the language of [`effective_language`]. The load order is resolved afresh (no cache);
    /// [`open_install`](Self::open_install) is the cached path the game uses.
    ///
    /// Only a failure to read `data\` itself is an error. Missing or broken mod packs, a
    /// missing `user.script.txt` and unknown commands are reported in the [`ModReport`].
    pub fn open_with_mods(data_dir: impl AsRef<Path>, options: &ModOptions) -> Result<(Self, ModReport), PackError> {
        let data_dir = data_dir.as_ref();
        let plan = plan_layers(data_dir, &effective_language(data_dir), options)?;
        Ok((mount_plan(plan.layers, plan.graph), plan.report))
    }

    /// Opens the install the way the game runs it: the packs and loose files of `data\` plus
    /// the mods set with [`set_mod_options`] (none by default), in the order of [`plan_layers`].
    ///
    /// Language packs (`local_XX*.pack`) are mounted only for one language: the override set
    /// with [`set_language_override`](super::set_language_override) if that language is
    /// installed, else the language named in `data\language.txt` (`EN`; what the original
    /// reads, CONFIRMED string in the exe), else English. See [`effective_language`].
    ///
    /// The load order is resolved once per data folder, language and mod setting and cached;
    /// its warnings (a broken or missing mod pack, ...) are logged once, when it is resolved.
    pub fn open_install(data_dir: impl AsRef<Path>) -> Result<Self, PackError> {
        let data_dir = data_dir.as_ref();
        Self::open_install_language(data_dir, &effective_language(data_dir))
    }

    /// Like [`open_install`](Self::open_install) with the language packs of `language` (a code
    /// such as `en`, `fr`; case-insensitive). Any installed language can be opened this way.
    pub fn open_install_language(data_dir: impl AsRef<Path>, language: &str) -> Result<Self, PackError> {
        let data_dir = data_dir.as_ref();
        let language = language.trim().to_ascii_lowercase();
        let options = mod_options();
        let key: PlanKey = (data_dir.to_path_buf(), language, options);
        let slot = {
            let mut plans = PLANS.lock().unwrap_or_else(|e| e.into_inner());
            match plans.iter().find(|(k, _)| *k == key) {
                Some((_, slot)) => Arc::clone(slot),
                None => {
                    let slot = PlanSlot::default();
                    plans.push((key.clone(), Arc::clone(&slot)));
                    slot
                }
            }
        };
        // A miss plans under its key's own lock, so two threads never plan (and warn) twice for one
        // key: the second finds the first one's plan once it gets the lock. Other keys don't wait.
        // Mounting (which re-opens every pack) always runs after the lock is released.
        let fresh = {
            let mut built = slot.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(plan) = built.clone() {
                drop(built);
                return mount_cached(&plan);
            }
            #[cfg(test)]
            PLANNED.lock().unwrap().push(data_dir.to_path_buf());
            let (_, language, options) = &key;
            let plan = plan_layers(data_dir, language, options)?;
            for w in &plan.report.warnings {
                warn_when_logging(&format!("ntw_formats: mods: {w}"));
            }
            let layers = plan
                .layers
                .iter()
                .map(|l| match l {
                    Planned::Pack { pack, kind } => CachedLayer::Pack { path: pack.path().to_path_buf(), kind: *kind },
                    Planned::Dir { dir, kind } => CachedLayer::Dir { dir: Arc::clone(dir), kind: *kind },
                })
                .collect();
            *built = Some(Arc::new(CachedPlan { layers, graph: Arc::clone(&plan.graph), gone: Mutex::new(Vec::new()) }));
            plan
        };
        Ok(mount_plan(fresh.layers, fresh.graph))
    }
}

/// Reports a mod warning. Plans are resolved by `--campaign` / `--battle-key` runs before Bevy's
/// `LogPlugin` installs a logger (the `log` max level is still `Off`), where `log::warn!` would be
/// dropped; those go to stderr, as the other pre-start messages in `main` do.
fn warn_when_logging(msg: &str) {
    if log::max_level() == log::LevelFilter::Off {
        eprintln!("{msg}");
    } else {
        log::warn!("{msg}");
    }
}

/// Mounts a freshly resolved plan in its mount order.
fn mount_plan(plan: Vec<Planned>, graph: Arc<PackGraph>) -> Vfs {
    let mut vfs = Vfs::new();
    vfs.set_pack_graph(graph);
    for l in plan {
        match l {
            Planned::Pack { pack, kind } => vfs.mount_as(pack, kind),
            Planned::Dir { dir, kind } => vfs.mount_loose_shared(dir, kind),
        }
    }
    vfs
}

/// Mounts a cached plan: re-opens its packs (each [`Vfs`] owns its own handles). A pack of the
/// install that no longer opens is an error; a mod pack that no longer opens is skipped with a
/// warning, once per plan (it opened when the plan was resolved, so this only happens if it
/// changed since).
fn mount_cached(plan: &CachedPlan) -> Result<Vfs, PackError> {
    let mut vfs = Vfs::new();
    vfs.set_pack_graph(Arc::clone(&plan.graph));
    for l in &plan.layers {
        match l {
            CachedLayer::Pack { path, kind } => match PackFile::open(path) {
                Ok(p) => vfs.mount_as(p, *kind),
                Err(e) if kind.is_mod() => {
                    if plan.first_failure(path) {
                        warn_when_logging(&format!("ntw_formats: mods: {}: {e}; skipped", path.display()));
                    }
                }
                Err(e) => return Err(e),
            },
            CachedLayer::Dir { dir, kind } => vfs.mount_loose_shared(Arc::clone(dir), *kind),
        }
    }
    Ok(vfs)
}

impl ModReport {
    /// A plain-text report: every layer (highest priority first), the files that mods add
    /// or override, then warnings and notes.
    pub fn render(&self, vfs: &Vfs) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "Install data folder: {}", self.data_dir.display());
        match &self.user_script {
            Some(p) => {
                let _ = writeln!(s, "User script: {} ({:?})", p.display(), self.user_script_encoding.unwrap_or(TextEncoding::Utf8));
            }
            None => {
                let _ = writeln!(s, "User script: none");
            }
        }
        let _ = writeln!(s, "Mods folder: {}", self.mods_dir.as_ref().map_or("off".to_owned(), |p| p.display().to_string()));
        let _ = writeln!(
            s,
            "\nActive layers, highest priority first (higher pack type wins; equal types: the first loaded):"
        );
        let layers = vfs.layers();
        let order = vfs.layers_by_priority();
        for &i in &order {
            let l = &layers[i];
            let _ = writeln!(s, "  {:>3}  {:<11} {:>6} files  {}", i, l.kind.name(), vfs.layer_len(l), vfs.layer_path(l).display());
        }
        // Files supplied by mod layers, and which other layer also has them.
        let mut holders: HashMap<String, Vec<usize>> = HashMap::new();
        for &i in &order {
            for p in vfs.layer_paths(&layers[i]) {
                holders.entry(p).or_default().push(i);
            }
        }
        let mut lines = Vec::new();
        for &i in order.iter().filter(|&&i| layers[i].kind.is_mod()) {
            for p in vfs.layer_paths(&layers[i]) {
                let state = match vfs.origin_index(&p) {
                    Some(w) if w != i => format!("hidden by layer {w}"),
                    _ => match holders.get(&p).and_then(|h| h.iter().find(|&&o| o != i)) {
                        Some(&o) => format!("overrides layer {o} ({})", vfs.layer_path(&layers[o]).display()),
                        None => "new file".to_owned(),
                    },
                };
                lines.push(format!("  [{i}] {p}  {state}"));
            }
        }
        let _ = writeln!(s, "\nFiles from mods ({}):", lines.len());
        for l in lines {
            let _ = writeln!(s, "{l}");
        }
        let _ = writeln!(s, "\nWarnings ({}):", self.warnings.len());
        for w in &self.warnings {
            let _ = writeln!(s, "  {w}");
        }
        let _ = writeln!(s, "\nNotes ({}):", self.notes.len());
        for n in &self.notes {
            let _ = writeln!(s, "  {n}");
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{build_pack, temp_dir};
    use super::*;

    fn utf16le(s: &str) -> Vec<u8> {
        let mut b = vec![0xFF, 0xFE];
        for u in s.encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        b
    }

    #[test]
    fn parses_all_encodings() {
        let text = "mod \"a.pack\";\r\nmod b.pack;\nimport_all_mods\n# mod \"c.pack\";\ngfx_quality 3;";
        let want = vec![
            ScriptCommand::Mod("a.pack".into()),
            ScriptCommand::Mod("b.pack".into()),
            ScriptCommand::ImportAllMods,
            ScriptCommand::Other("#".into()),
            ScriptCommand::Other("gfx_quality".into()),
        ];
        let le = UserScript::parse(&utf16le(text));
        assert_eq!((le.commands.clone(), le.encoding), (want.clone(), TextEncoding::Utf16Le));
        let no_bom = UserScript::parse(&utf16le(text)[2..]);
        assert_eq!(no_bom.commands, want);
        let mut be = vec![0xFE, 0xFF];
        for u in text.encode_utf16() {
            be.extend_from_slice(&u.to_be_bytes());
        }
        assert_eq!(UserScript::parse(&be).encoding, TextEncoding::Utf16Be);
        assert_eq!(UserScript::parse(&be).commands, want);
        let mut utf8 = vec![0xEF, 0xBB, 0xBF];
        utf8.extend_from_slice(text.as_bytes());
        assert_eq!(UserScript::parse(&utf8).commands, want);
        assert_eq!(UserScript::parse(text.as_bytes()).commands, want);
    }

    #[test]
    fn parses_loading_commands() {
        let c = UserScript::parse_str(
            "mod \"Sub Dir/x y.pack\"; mod z.pack\nexclude_pack_file media.pack\nset_pack_file_precedence a.pack b.pack\n\
             set_pack_file_dependency c.pack d.pack\nadd_working_directory \"extra\"\n\n   ;  ;\nMOD caps.pack\nmod\n\
             mod a\"b c\"d.pack",
        );
        assert_eq!(
            c,
            vec![
                ScriptCommand::Mod("Sub Dir/x y.pack".into()),
                ScriptCommand::Mod("z.pack".into()),
                ScriptCommand::ExcludePack("media.pack".into()),
                ScriptCommand::Precedence("a.pack".into(), "b.pack".into()),
                ScriptCommand::Dependency("c.pack".into(), "d.pack".into()),
                ScriptCommand::WorkingDirectory("extra".into()),
                // Command words are case-sensitive; a missing argument loads nothing.
                ScriptCommand::Other("MOD".into()),
                ScriptCommand::Other("mod".into()),
                // A quote toggles anywhere in a word (0x010F5D10).
                ScriptCommand::Mod("ab cd.pack".into()),
            ]
        );
    }

    #[test]
    fn slot_order_follows_the_original_hash_map() {
        // djb2 of "data.pack" etc. modulo 71 decides; equal buckets probe upward.
        let names: Vec<String> = ["a.pack", "b.pack", "c.pack"].iter().map(|s| s.to_string()).collect();
        let order = scan_slot_order(&names);
        let mut by_slot: Vec<(u32, usize)> = (0..3).map(|i| (pack_name_hash(&names[i]) % 71, i)).collect();
        by_slot.sort();
        assert_eq!(order, by_slot.iter().map(|&(_, i)| i).collect::<Vec<_>>());
        assert_eq!(pack_name_hash("a"), (5381u32 * 33 + 97) & 0x7fff_ffff);
        // More names than slots: the table grows (2n+1) and keeps every name once.
        let many: Vec<String> = (0..200).map(|i| format!("p{i}.pack")).collect();
        let mut order = scan_slot_order(&many);
        order.sort();
        assert_eq!(order, (0..200).collect::<Vec<_>>());
    }

    /// A small fake install: boot, release, patch, movie, two inactive mod packs and a loose file.
    fn fake_install(test: &str) -> PathBuf {
        let dir = temp_dir(test);
        let data = dir.join("data");
        std::fs::create_dir_all(data.join("sub")).unwrap();
        std::fs::write(data.join("boot.pack"), build_pack(0, &[("boot.txt", b"boot")])).unwrap();
        std::fs::write(
            data.join("data.pack"),
            build_pack(1, &[("f.txt", b"release"), ("g.txt", b"release"), ("h.txt", b"release"), ("loose.txt", b"release")]),
        )
        .unwrap();
        std::fs::write(data.join("patch.pack"), build_pack(2, &[("g.txt", b"patch")])).unwrap();
        std::fs::write(data.join("media.pack"), build_pack(4, &[("m.bik", b"movie")])).unwrap();
        std::fs::write(data.join("my_mod.pack"), build_pack(3, &[("f.txt", b"my_mod"), ("new.txt", b"my_mod")])).unwrap();
        std::fs::write(data.join("other_mod.pack"), build_pack(3, &[("f.txt", b"other_mod"), ("o.txt", b"other")]))
            .unwrap();
        std::fs::write(data.join("sub").join("nested.pack"), build_pack(3, &[("h.txt", b"nested")])).unwrap();
        std::fs::write(data.join("loose.txt"), b"loose").unwrap();
        std::fs::write(data.join("only_loose.txt"), b"only loose").unwrap();
        data
    }

    fn opts(script: &str) -> ModOptions {
        ModOptions { user_script: UserScriptSetting::Text(script.into()), mods_dir: None }
    }

    /// Two threads missing the plan cache together plan the install once.
    #[test]
    fn a_missed_plan_is_resolved_once_for_threads_missing_together() {
        let data = fake_install("plan_once");
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|s| {
            for _ in 0..2 {
                s.spawn(|| {
                    barrier.wait();
                    Vfs::open_install_language(&data, "en").unwrap();
                });
            }
        });
        let planned = PLANNED.lock().unwrap().iter().filter(|d| **d == data).count();
        assert_eq!(planned, 1);
    }

    #[test]
    fn mod_packs_need_a_script_line_and_loose_files_lose_to_packs() {
        let data = fake_install("mods_need_line");
        let (vfs, report) = Vfs::open_with_mods(&data, &opts("")).unwrap();
        assert_eq!(vfs.read("f.txt").unwrap(), b"release");
        assert!(!vfs.contains("new.txt"));
        assert_eq!(vfs.read("m.bik").unwrap(), b"movie", "movie packs load without a line");
        assert_eq!(vfs.read("loose.txt").unwrap(), b"release", "a pack beats a loose file");
        assert_eq!(vfs.read("only_loose.txt").unwrap(), b"only loose");
        assert_eq!(report.notes.iter().filter(|n| n.contains("no `mod` line")).count(), 2);
        let vanilla = Vfs::open_install(&data).unwrap();
        assert_eq!(vanilla.list(""), vfs.list(""));
    }

    /// Rule: among mod packs the first `mod` line wins; a mod beats release and patch packs.
    #[test]
    fn first_mod_line_wins_and_missing_packs_are_skipped() {
        let data = fake_install("mods_order");
        let script = "mod \"my_mod.pack\";\nmod \"missing.pack\";\nmod \"other_mod.pack\";\nmod \"sub/nested.pack\";";
        let (vfs, report) = Vfs::open_with_mods(&data, &opts(script)).unwrap();
        assert_eq!(vfs.read("f.txt").unwrap(), b"my_mod", "the first line wins");
        assert_eq!(vfs.read("o.txt").unwrap(), b"other");
        assert_eq!(vfs.read("h.txt").unwrap(), b"nested", "paths relative to data\\ work; a mod beats release");
        assert_eq!(vfs.read("g.txt").unwrap(), b"patch");
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("missing.pack"));
        let kinds: Vec<_> = vfs.layers().iter().map(|l| l.kind.name()).collect();
        assert_eq!(kinds, ["boot", "release", "movie", "patch", "loose", "mod", "mod", "mod"]);
        let rendered = report.render(&vfs);
        assert!(rendered.contains("new.txt  new file"), "{rendered}");
        assert!(rendered.contains("f.txt  overrides layer"), "{rendered}");
        assert!(rendered.contains("f.txt  hidden by layer"), "{rendered}");

        let (vfs, _) = Vfs::open_with_mods(&data, &opts("mod other_mod.pack;\nmod my_mod.pack;")).unwrap();
        assert_eq!(vfs.read("f.txt").unwrap(), b"other_mod");
    }

    #[test]
    fn import_exclude_and_precedence() {
        let data = fake_install("mods_cmds");
        // import_all_mods: every mod-type pack of data\ in slot order; the mod lines are ignored.
        let (vfs, report) = Vfs::open_with_mods(&data, &opts("mod other_mod.pack\nimport_all_mods")).unwrap();
        assert!(vfs.contains("o.txt") && vfs.contains("new.txt"));
        assert!(!vfs.contains("h.txt") || vfs.read("h.txt").unwrap() == b"release", "sub\\ is not scanned");
        assert!(report.notes.iter().any(|n| n.contains("ignored")));
        let names: Vec<String> = ["my_mod.pack", "other_mod.pack"].iter().map(|s| s.to_string()).collect();
        let first = &names[scan_slot_order(&names)[0]];
        let want: &[u8] = if first == "my_mod.pack" { b"my_mod" } else { b"other_mod" };
        assert_eq!(vfs.read("f.txt").unwrap(), want, "the earlier slot wins");

        let (vfs, _) = Vfs::open_with_mods(&data, &opts("import_all_mods\nexclude_pack_file my_mod.pack")).unwrap();
        assert_eq!(vfs.read("f.txt").unwrap(), b"other_mod");
        let (vfs, _) = Vfs::open_with_mods(&data, &opts("exclude_pack_file patch.pack")).unwrap();
        assert_eq!(vfs.read("g.txt").unwrap(), b"release");
        let (vfs, report) = Vfs::open_with_mods(&data, &opts("exclude_pack_file my_mod.pack\nmod my_mod.pack")).unwrap();
        assert!(!vfs.contains("new.txt"));
        assert!(report.warnings.iter().any(|w| w.contains("excluded")));
        // A precedence pair: rhs wins (VFS_ShouldPackOverride(old, new) is true when old comes
        // first in the node order), whatever the mount order or header types.
        let (vfs, _) =
            Vfs::open_with_mods(&data, &opts("mod other_mod.pack\nmod my_mod.pack\nset_pack_file_precedence other_mod.pack my_mod.pack"))
                .unwrap();
        assert_eq!(vfs.read("f.txt").unwrap(), b"my_mod");
        let (vfs, _) = Vfs::open_with_mods(&data, &opts("set_pack_file_precedence patch.pack data.pack")).unwrap();
        assert_eq!(vfs.read("g.txt").unwrap(), b"release", "a release pack over a patch pack");
        // A dependency is mounted first even without its own mod line, and the pack that needs it
        // wins over it (rhs).
        let (vfs, _) = Vfs::open_with_mods(&data, &opts("set_pack_file_dependency my_mod.pack other_mod.pack\nmod other_mod.pack")).unwrap();
        assert!(vfs.contains("new.txt"));
        assert_eq!(vfs.read("f.txt").unwrap(), b"other_mod");
        let kinds: Vec<_> = vfs.layers().iter().map(|l| vfs.layer_path(l).file_name().unwrap().to_owned()).collect();
        let pos = |n: &str| kinds.iter().position(|k| k == n).unwrap();
        assert!(pos("my_mod.pack") < pos("other_mod.pack"), "the dependency is mounted first");
    }

    /// The whole graph decides between two of its packs, not just a pair naming both
    /// (VFS_ComparePackPrecedenceGraph 0x0108EBB0 orders by the graph's node order).
    #[test]
    fn precedence_follows_the_whole_graph() {
        let data = fake_install("mods_graph");
        // Pairs (other_mod, data) and (my_mod, patch) give the node order [other_mod, my_mod,
        // data, patch]; f.txt is in data, my_mod and other_mod: data is the latest node holding
        // it, so the release pack beats both mod packs although no pair names data and my_mod.
        let script = "mod my_mod.pack\nmod other_mod.pack\nset_pack_file_precedence other_mod.pack data.pack\n\
                      set_pack_file_precedence my_mod.pack patch.pack";
        let (vfs, report) = Vfs::open_with_mods(&data, &opts(script)).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(vfs.read("f.txt").unwrap(), b"release");
        // g.txt: data (pos 2) and patch (pos 3): patch wins, as the types would also say.
        assert_eq!(vfs.read("g.txt").unwrap(), b"patch");
        // Chained pairs (other_mod, data), (data, my_mod): node order [other_mod, data, my_mod],
        // so my_mod beats other_mod with no pair naming both.
        let script = "mod my_mod.pack\nmod other_mod.pack\nset_pack_file_precedence other_mod.pack data.pack\n\
                      set_pack_file_precedence data.pack my_mod.pack";
        let (vfs, _) = Vfs::open_with_mods(&data, &opts(script)).unwrap();
        assert_eq!(vfs.read("f.txt").unwrap(), b"my_mod");
        // A pair closing a cycle is dropped with a warning; the earlier order stays.
        let (vfs, report) = Vfs::open_with_mods(&data, &opts(&format!("{script}\nset_pack_file_precedence my_mod.pack other_mod.pack")))
            .unwrap();
        assert_eq!(vfs.read("f.txt").unwrap(), b"my_mod");
        assert!(report.warnings.iter().any(|w| w.contains("cycle")), "{:?}", report.warnings);
    }

    /// A pack whose dependency is excluded is skipped (the original stops start-up).
    #[test]
    fn excluded_dependency_skips_the_pack() {
        let data = fake_install("mods_dep_excluded");
        let script = "exclude_pack_file my_mod.pack\nset_pack_file_dependency my_mod.pack other_mod.pack\nmod other_mod.pack";
        let (vfs, report) = Vfs::open_with_mods(&data, &opts(script)).unwrap();
        assert!(!vfs.contains("o.txt") && !vfs.contains("new.txt"));
        assert!(report.warnings.iter().any(|w| w.contains("needs \"my_mod.pack\"")), "{:?}", report.warnings);
    }

    /// Statements we do not run are warnings (they were notes nobody saw), and a loading command
    /// without its arguments says what it needs.
    #[test]
    fn skipped_script_statements_are_warnings() {
        let data = fake_install("mods_unknown_line");
        let (vfs, report) = Vfs::open_with_mods(&data, &opts("# mod my_mod.pack\nmod\nset_pack_file_precedence a.pack\nmod my_mod.pack")).unwrap();
        assert!(vfs.contains("new.txt"), "later lines still run");
        assert_eq!(
            report.warnings,
            [
                "user script: \"#\" is not a file-loading command; skipped",
                "user script: mod needs a pack name; skipped",
                "user script: set_pack_file_precedence needs two pack names; skipped",
            ]
        );
    }

    /// A cached plan whose mod pack disappeared warns once, not on every open.
    #[test]
    fn cached_plan_warns_once_for_a_gone_pack() {
        let data = fake_install("mods_gone_pack");
        let plan = plan_layers(&data, "en", &opts("mod my_mod.pack")).unwrap();
        let cached = CachedPlan {
            layers: plan
                .layers
                .iter()
                .map(|l| match l {
                    Planned::Pack { pack, kind } => CachedLayer::Pack { path: pack.path().to_path_buf(), kind: *kind },
                    Planned::Dir { dir, kind } => CachedLayer::Dir { dir: Arc::clone(dir), kind: *kind },
                })
                .collect(),
            graph: plan.graph,
            gone: Mutex::new(Vec::new()),
        };
        drop(plan.layers);
        std::fs::remove_file(data.join("my_mod.pack")).unwrap();
        for _ in 0..3 {
            let vfs = mount_cached(&cached).unwrap();
            assert!(!vfs.contains("new.txt"));
        }
        assert_eq!(cached.gone.lock().unwrap().len(), 1);
        assert!(!cached.first_failure(&data.join("my_mod.pack")));
    }

    /// add_working_directory: relative to the install root; its packs join the scan, its loose
    /// files come after data\'s.
    #[test]
    fn working_directories() {
        let data = fake_install("mods_workdir");
        let extra = data.parent().unwrap().join("extra");
        std::fs::create_dir_all(&extra).unwrap();
        std::fs::write(extra.join("extra_patch.pack"), build_pack(2, &[("x.txt", b"extra patch")])).unwrap();
        std::fs::write(extra.join("extra_mod.pack"), build_pack(3, &[("y.txt", b"extra mod")])).unwrap();
        std::fs::write(extra.join("only_loose.txt"), b"second").unwrap();
        std::fs::write(extra.join("extra_loose.txt"), b"extra loose").unwrap();
        let (vfs, _) = Vfs::open_with_mods(&data, &opts("add_working_directory extra\nmod extra_mod.pack")).unwrap();
        assert_eq!(vfs.read("x.txt").unwrap(), b"extra patch");
        assert_eq!(vfs.read("y.txt").unwrap(), b"extra mod");
        assert_eq!(vfs.read("only_loose.txt").unwrap(), b"only loose", "data\\ is searched first");
        assert_eq!(vfs.read("extra_loose.txt").unwrap(), b"extra loose");
    }

    #[test]
    fn mods_folder_with_load_order() {
        let data = fake_install("mods_folder");
        let mods = data.parent().unwrap().join("mods");
        std::fs::create_dir_all(mods.join("Loose Mod").join("db")).unwrap();
        std::fs::write(mods.join("Loose Mod").join("f.txt"), b"loose mod").unwrap();
        std::fs::write(mods.join("packed.pack"), build_pack(1, &[("f.txt", b"packed"), ("p.txt", b"p")])).unwrap();
        std::fs::write(mods.join("unlisted.pack"), build_pack(1, &[("u.txt", b"u")])).unwrap();
        let o = ModOptions { user_script: UserScriptSetting::Text("mod my_mod.pack;".into()), mods_dir: Some(mods.clone()) };

        // No load_order.txt: everything, name order, first = highest.
        let (vfs, _) = Vfs::open_with_mods(&data, &o).unwrap();
        assert_eq!(vfs.read("f.txt").unwrap(), b"loose mod");
        assert!(vfs.contains("u.txt"));

        std::fs::write(mods.join(LOAD_ORDER_NAME), "# my order\npacked.pack\nloose mod\nnot_there\n").unwrap();
        let (vfs, report) = Vfs::open_with_mods(&data, &o).unwrap();
        assert_eq!(vfs.read("f.txt").unwrap(), b"packed", "the first listed wins, over a script mod too");
        assert!(!vfs.contains("u.txt"));
        assert_eq!(vfs.origin("f.txt").unwrap().kind, LayerKind::ModsFolder);
        assert!(report.warnings.iter().any(|w| w.contains("not_there")));
        assert!(report.notes.iter().any(|n| n.contains("unlisted.pack")));
    }

    #[test]
    fn db_table_file_order() {
        let root = temp_dir("db_merge_order");
        let dir = root.join("data");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("data.pack"),
            build_pack(1, &[("db\\units_tables\\units", b"v"), ("db\\units_tables\\zz_mod", b"z"), ("db\\units_tables\\deep\\x", b"x")]),
        )
        .unwrap();
        std::fs::write(dir.join("m.pack"), build_pack(3, &[("db\\units_tables\\aa_mod", b"a")])).unwrap();
        let mods = root.join("mods");
        std::fs::create_dir_all(mods.join("easy").join("db").join("units_tables")).unwrap();
        std::fs::write(mods.join("easy").join("db").join("units_tables").join("zzz_easy"), b"e").unwrap();
        let o = ModOptions { user_script: UserScriptSetting::Text("mod m.pack".into()), mods_dir: Some(mods) };
        let (vfs, _) = Vfs::open_with_mods(&dir, &o).unwrap();
        let files: Vec<String> = vfs.table_files("db/units_tables").into_iter().map(|f| f.path).collect();
        assert_eq!(
            files,
            ["db\\units_tables\\units", "db\\units_tables\\zz_mod", "db\\units_tables\\aa_mod", "db\\units_tables\\zzz_easy"]
        );
    }
}

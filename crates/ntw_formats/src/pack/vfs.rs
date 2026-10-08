//! The virtual file system: many packs layered into one read-only view.

use std::collections::HashMap;
use std::path::Path;

use super::{PackEntry, PackError, PackFile, PackType, normalize_path};

/// A read-only view over several packs. When packs contain the same path,
/// the one **mounted later** wins.
///
/// # Load order
/// Worker 1 found the game's VFS init (`0x01051340`, CONFIRMED). It mounts the boot
/// pack, then release packs, then patch packs, then bink (movie) packs, then mods.
/// [`Vfs::open_install`] follows that order. Within one group, packs are sorted by
/// file name. That matches the alphabetical order Windows' `FindFirstFileW` returns
/// on NTFS (INFERRED; no shipped file depends on it). So `local_en_patch.pack`
/// (patch) overrides `local_en.pack` (release).
///
/// There are no write methods. The packs are opened read-only.
///
/// ```no_run
/// use ntw_formats::pack::Vfs;
/// let vfs = Vfs::open_install(r"C:\Games\Napoleon Total War\data").unwrap();
/// let units = vfs.read("db/units_tables/units").unwrap();
/// ```
#[derive(Debug, Default)]
pub struct Vfs {
    packs: Vec<PackFile>,
    /// Normalized path -> (pack index, entry index). Later mounts overwrite earlier ones.
    index: HashMap<String, (usize, usize)>,
}

/// Position of a pack type in the mount order (lower mounts first).
fn load_rank(t: PackType) -> u8 {
    match t {
        PackType::Boot => 0,
        PackType::Release => 1,
        PackType::Patch => 2,
        PackType::Movie => 3,
        PackType::Mod => 4,
        PackType::Other(_) => 5,
    }
}

/// For `local_en.pack` / `local_en_patch.pack` returns `Some("en")`; otherwise `None`.
fn language_of(file_name: &str) -> Option<String> {
    let rest = file_name.to_ascii_lowercase().strip_prefix("local_")?.to_owned();
    let lang: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    (!lang.is_empty()).then_some(lang)
}

impl Vfs {
    /// An empty VFS with nothing mounted.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens every `*.pack` in an install's `data` folder, in game load order.
    ///
    /// Language packs (`local_XX*.pack`) are mounted only for one language: the override set
    /// with [`set_language_override`] if that language is installed, else the language named in
    /// `data\language.txt` (`EN`; what the original reads, CONFIRMED string in the exe), else
    /// English. See [`effective_language`].
    pub fn open_install(data_dir: impl AsRef<Path>) -> Result<Self, PackError> {
        let data_dir = data_dir.as_ref();
        Self::open_install_language(data_dir, &effective_language(data_dir))
    }

    /// Like [`open_install`](Self::open_install) with the language packs of `language` (a code
    /// such as `en`, `fr`; case-insensitive). Any installed language can be opened this way.
    pub fn open_install_language(data_dir: impl AsRef<Path>, language: &str) -> Result<Self, PackError> {
        let data_dir = data_dir.as_ref();
        let language = language.trim().to_ascii_lowercase();
        let mut packs = Vec::new();
        for item in std::fs::read_dir(data_dir)? {
            let path = item?.path();
            let is_pack = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pack"));
            if !is_pack || !path.is_file() {
                continue;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if language_of(&name).is_some_and(|l| l != language) {
                continue;
            }
            packs.push(PackFile::open(&path)?);
        }
        Ok(Self::from_packs(packs))
    }

    /// Builds a VFS from already-open packs, sorting them into game load order first.
    pub fn from_packs(mut packs: Vec<PackFile>) -> Self {
        packs.sort_by_cached_key(|p| {
            let name = p.path().file_name().map(|n| n.to_string_lossy().to_ascii_lowercase());
            (load_rank(p.pack_type()), name)
        });
        let mut vfs = Self::new();
        for p in packs {
            vfs.mount(p);
        }
        vfs
    }

    /// Mounts one more pack **on top**: its files override any already mounted.
    /// Use this, for example, to add a mod pack after [`open_install`](Self::open_install).
    pub fn mount(&mut self, pack: PackFile) {
        let pack_index = self.packs.len();
        for (i, e) in pack.entries().iter().enumerate() {
            self.index.insert(normalize_path(&e.path), (pack_index, i));
        }
        self.packs.push(pack);
    }

    /// The mounted packs, in mount order (the last one has the highest priority).
    pub fn packs(&self) -> &[PackFile] {
        &self.packs
    }

    /// Number of distinct paths visible through the VFS.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// True if no files are visible.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The winning pack and entry for a path (case-insensitive, `/` or `\`).
    pub fn find(&self, path: &str) -> Option<(&PackFile, &PackEntry)> {
        let &(p, e) = self.index.get(&normalize_path(path))?;
        let pack = &self.packs[p];
        Some((pack, &pack.entries()[e]))
    }

    /// True if some mounted pack contains `path`.
    pub fn contains(&self, path: &str) -> bool {
        self.index.contains_key(&normalize_path(path))
    }

    /// Reads a file's contents from the highest-priority pack that has it.
    pub fn read(&self, path: &str) -> Result<Vec<u8>, PackError> {
        let (pack, entry) = self.find(path).ok_or_else(|| PackError::NotFound(path.to_owned()))?;
        pack.read_entry(entry)
    }

    /// Reads `len` bytes of a file starting `start` bytes in (clamped to the file), from the
    /// highest-priority pack that has it. Used to stream large files such as movies.
    pub fn read_range(&self, path: &str, start: u64, len: usize) -> Result<Vec<u8>, PackError> {
        let (pack, entry) = self.find(path).ok_or_else(|| PackError::NotFound(path.to_owned()))?;
        pack.read_entry_range(entry, start, len)
    }

    /// All visible paths that start with `prefix` (normalized), sorted.
    /// The paths are returned normalized (lowercase, `\`). Example: `list("db/units_tables/")`.
    pub fn list(&self, prefix: &str) -> Vec<&str> {
        let prefix = normalize_path(prefix);
        let mut out: Vec<&str> =
            self.index.keys().filter(|k| k.starts_with(&prefix)).map(String::as_str).collect();
        out.sort_unstable();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::{build_pack, temp_dir};
    use super::*;

    #[test]
    fn later_packs_override_in_load_order() {
        let _lock = super::LANGUAGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = temp_dir("vfs_order");
        // Written in "wrong" alphabetical order on purpose: type decides first.
        std::fs::write(dir.join("a_patch.pack"), build_pack(2, &[("text\\ui.loc", b"patch")])).unwrap();
        std::fs::write(dir.join("b_boot.pack"), build_pack(0, &[("text\\ui.loc", b"boot"), ("boot.txt", b"b")]))
            .unwrap();
        std::fs::write(
            dir.join("c_data.pack"),
            build_pack(1, &[("text\\ui.loc", b"release"), ("db\\x_tables\\x", b"x")]),
        )
        .unwrap();
        std::fs::write(dir.join("d_mod.pack"), build_pack(3, &[("db\\x_tables\\x", b"modded")])).unwrap();
        std::fs::write(dir.join("local_fr.pack"), build_pack(1, &[("text\\ui.loc", b"french")])).unwrap();
        std::fs::write(dir.join("language.txt"), "EN").unwrap();
        std::fs::write(dir.join("not_a_pack.txt"), "ignored").unwrap();

        let vfs = Vfs::open_install(&dir).unwrap();
        let order: Vec<_> =
            vfs.packs().iter().map(|p| p.path().file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(order, ["b_boot.pack", "c_data.pack", "a_patch.pack", "d_mod.pack"]);
        assert_eq!(vfs.read("TEXT/ui.loc").unwrap(), b"patch");
        assert_eq!(vfs.read("db/x_tables/x").unwrap(), b"modded");
        assert_eq!(vfs.read("boot.txt").unwrap(), b"b");
        assert!(matches!(vfs.read("nope"), Err(PackError::NotFound(_))));
        assert_eq!(vfs.list("db/"), ["db\\x_tables\\x"]);
        assert_eq!(vfs.len(), 3);
    }

    #[test]
    fn mount_puts_pack_on_top() {
        let dir = temp_dir("vfs_mount");
        std::fs::write(dir.join("m.pack"), build_pack(1, &[("f", b"1")])).unwrap();
        std::fs::write(dir.join("n.pack"), build_pack(1, &[("F", b"2")])).unwrap();
        let mut vfs = Vfs::new();
        vfs.mount(PackFile::open(dir.join("m.pack")).unwrap());
        vfs.mount(PackFile::open(dir.join("n.pack")).unwrap());
        assert_eq!(vfs.read("f").unwrap(), b"2");
        assert!(vfs.contains("F"));
    }

    #[test]
    fn language_detection() {
        assert_eq!(language_of("local_en_patch.pack").as_deref(), Some("en"));
        assert_eq!(language_of("LOCAL_FR.pack").as_deref(), Some("fr"));
        assert_eq!(language_of("data.pack"), None);
    }
}

/// The language override (our own setting; the install's `language.txt` is never written).
static LANGUAGE_OVERRIDE: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// Serialises the tests that depend on [`LANGUAGE_OVERRIDE`]: it is process-wide, and tests run
/// in parallel threads.
#[cfg(test)]
pub(crate) static LANGUAGE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Sets (or clears) the language [`Vfs::open_install`] uses, e.g. from NapoleonRust's own
/// settings or `--language fr`. A language that is not installed is ignored (with no packs to
/// read, the text would be missing), so the install's language is used instead.
pub fn set_language_override(language: Option<&str>) {
    if let Ok(mut l) = LANGUAGE_OVERRIDE.write() {
        *l = language.map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty());
    }
}

/// The languages installed in `data_dir`: every `XX` with a `local_XX*.pack`, sorted, lower case.
pub fn installed_languages(data_dir: impl AsRef<Path>) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(data_dir.as_ref())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.to_ascii_lowercase().ends_with(".pack").then(|| language_of(&name)).flatten()
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The install's own language: `data\language.txt` (e.g. `EN`), lower case; `en` if missing.
pub fn install_language(data_dir: impl AsRef<Path>) -> String {
    std::fs::read_to_string(data_dir.as_ref().join("language.txt"))
        .map(|s| s.trim().to_ascii_lowercase())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "en".to_owned())
}

/// The language [`Vfs::open_install`] mounts: the override if it is installed, else
/// [`install_language`].
pub fn effective_language(data_dir: impl AsRef<Path>) -> String {
    let data_dir = data_dir.as_ref();
    let over = LANGUAGE_OVERRIDE.read().ok().and_then(|l| l.clone());
    match over {
        Some(l) if installed_languages(data_dir).contains(&l) => l,
        _ => install_language(data_dir),
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn languages_and_override() {
        let _lock = LANGUAGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join("napoleonrust_tests").join("languages");
        std::fs::create_dir_all(&dir).unwrap();
        for f in ["local_en.pack", "local_en_patch.pack", "local_FR.pack", "data.pack"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        std::fs::write(dir.join("language.txt"), "EN").unwrap();
        assert_eq!(installed_languages(&dir), ["en", "fr"]);
        assert_eq!(install_language(&dir), "en");
        set_language_override(Some("FR"));
        assert_eq!(effective_language(&dir), "fr");
        set_language_override(Some("de"));
        assert_eq!(effective_language(&dir), "en");
        set_language_override(None);
        assert_eq!(effective_language(&dir), "en");
    }
}

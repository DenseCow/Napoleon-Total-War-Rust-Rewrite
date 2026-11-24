//! Where the original game is installed, and where it keeps its user files, on each platform.
//!
//! One place for these lookups: the game, the examples and the install tests all call it, so a
//! platform or Steam-library rule is written once.
//!
//! * **Windows:** the default Steam folder `C:\Program Files (x86)\Steam`, plus every library listed
//!   in its `steamapps\libraryfolders.vdf`. The original's user folder is `%APPDATA%\The Creative
//!   Assembly\Napoleon`.
//! * **Linux:** the game is the Windows build run through Proton. The Steam roots searched are
//!   `$XDG_DATA_HOME/Steam` (`~/.local/share/Steam`), `~/.steam/steam`, `~/.steam/root`, the
//!   Flatpak and Snap Steam folders, plus every library their `libraryfolders.vdf` lists. The
//!   original's user folder is inside the game's Proton prefix
//!   (`<library>/steamapps/compatdata/<app id>/pfx/drive_c/users/steamuser/AppData/Roaming`),
//!   unless `APPDATA` is set (running under plain Wine).
//!
//! Everything here only reads.

use std::path::{Path, PathBuf};

/// The environment variable naming the original's `data` folder (used by the tests and examples).
pub const DATA_DIR_ENV: &str = "NTW_DATA_DIR";
/// The environment variable naming the original's install folder (the one holding `data`).
pub const INSTALL_DIR_ENV: &str = "NAPOLEON_INSTALL_DIR";
/// The environment variable naming the original's user folder (`...\The Creative Assembly\Napoleon`).
pub const ORIGINAL_USER_DIR_ENV: &str = "NAPOLEON_ORIGINAL_USER_DIR";

/// The install's folder name under `steamapps/common`.
pub const STEAM_FOLDER: &str = "Napoleon Total War";
/// Napoleon: Total War's Steam app id (the Proton prefix is `steamapps/compatdata/<id>`).
pub const STEAM_APP_ID: u32 = 34030;
/// The default Windows install, used when no Steam library holds the game.
pub const WINDOWS_DEFAULT_INSTALL_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War";


/// The environment overrides, read on each call (cheap); only the Steam search is cached.
struct Overrides {
    install: Option<PathBuf>,
    data: Option<PathBuf>,
}

impl Overrides {
    fn from_env() -> Self {
        Self { install: env_path(INSTALL_DIR_ENV), data: env_path(DATA_DIR_ENV) }
    }

    /// One precedence for every lookup: `$NAPOLEON_INSTALL_DIR`, then `$NTW_DATA_DIR` (its
    /// parent is the install), then the Steam search.
    fn install_dir(&self, found: impl FnOnce() -> PathBuf) -> PathBuf {
        if let Some(dir) = &self.install {
            return dir.clone();
        }
        match self.data.as_deref().and_then(Path::parent) {
            Some(parent) => parent.to_path_buf(),
            None => found(),
        }
    }

    fn data_dir(&self, found: impl FnOnce() -> PathBuf) -> PathBuf {
        match (&self.install, &self.data) {
            (Some(install), _) => install.join("data"),
            (None, Some(data)) => data.clone(),
            (None, None) => found().join("data"),
        }
    }
}

/// The original's install folder: `$NAPOLEON_INSTALL_DIR` if set, else the parent of
/// `$NTW_DATA_DIR` if set, else the first Steam library holding the game, else the platform's
/// default Steam location (which may not exist; readers report the missing files).
pub fn install_dir() -> PathBuf {
    Overrides::from_env().install_dir(found_install_dir)
}

/// The original's `data` folder: `$NAPOLEON_INSTALL_DIR/data` if set, else `$NTW_DATA_DIR` if
/// set, else `data` in the Steam search's install (the same precedence as [`install_dir`]).
pub fn data_dir() -> PathBuf {
    Overrides::from_env().data_dir(found_install_dir)
}

/// The original's user folder (saves, `scripts/preferences.script.txt`, `user.script.txt`):
/// `$NAPOLEON_ORIGINAL_USER_DIR` if set, else `%APPDATA%\The Creative Assembly\Napoleon`, else
/// (Linux) the same folder inside the game's Proton prefix. `None` when none can be named.
pub fn original_user_dir() -> Option<PathBuf> {
    if let Some(dir) = env_path(ORIGINAL_USER_DIR_ENV) {
        return Some(dir);
    }
    let roaming = env_path("APPDATA")
        .or_else(|| proton_prefix().map(|p| p.join("drive_c/users/steamuser/AppData/Roaming")))?;
    Some(roaming.join("The Creative Assembly").join("Napoleon"))
}

/// The game's Proton (Wine) prefix, `<library>/steamapps/compatdata/<app id>/pfx`, in the library
/// that holds the install (or any library if the install is elsewhere). `None` when there is none
/// (always on Windows).
pub fn proton_prefix() -> Option<PathBuf> {
    if cfg!(windows) {
        return None;
    }
    let compat = |lib: &Path| lib.join("steamapps/compatdata").join(STEAM_APP_ID.to_string()).join("pfx");
    // `<library>/steamapps/common/<folder>` → `<library>`.
    let install = install_dir();
    let own_library = install.parent().and_then(Path::parent).and_then(Path::parent).map(compat);
    own_library
        .into_iter()
        .chain(steam_search().libraries.iter().map(|l| compat(l)))
        .find(|p| p.is_dir())
}

/// The Steam search's result, taken once per process (the Steam libraries do not move while the
/// game runs): every library, and the install found in one of them.
struct SteamSearch {
    libraries: Vec<PathBuf>,
    install: Option<PathBuf>,
}

fn steam_search() -> &'static SteamSearch {
    static SEARCH: std::sync::OnceLock<SteamSearch> = std::sync::OnceLock::new();
    SEARCH.get_or_init(|| {
        let libraries = steam_libraries();
        let install = libraries
            .iter()
            .map(|lib| lib.join("steamapps").join("common").join(STEAM_FOLDER))
            .find(|dir| dir.join("data").is_dir());
        SteamSearch { libraries, install }
    })
}

fn found_install_dir() -> PathBuf {
    steam_search().install.clone().unwrap_or_else(default_install_dir)
}

fn default_install_dir() -> PathBuf {
    if cfg!(windows) {
        return PathBuf::from(WINDOWS_DEFAULT_INSTALL_DIR);
    }
    steam_roots()
        .into_iter()
        .next()
        .map(|root| root.join("steamapps").join("common").join(STEAM_FOLDER))
        .unwrap_or_else(|| PathBuf::from(WINDOWS_DEFAULT_INSTALL_DIR))
}

/// The Steam client folders to look in, most likely first.
fn steam_roots() -> Vec<PathBuf> {
    if cfg!(windows) {
        return vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"C:\Program Files\Steam")];
    }
    let Some(home) = env_path("HOME") else { return Vec::new() };
    let data_home = env_path("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
    vec![
        data_home.join("Steam"),
        home.join(".steam/steam"),
        home.join(".steam/root"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        home.join("snap/steam/common/.local/share/Steam"),
        home.join("Library/Application Support/Steam"),
    ]
}

/// Every Steam library: each existing root and the libraries its `libraryfolders.vdf` lists,
/// without duplicates (the `~/.steam` links usually point at the same folder). Off Windows the
/// links are resolved to compare them; on Windows the paths stay as written (`canonicalize`
/// would turn them into `\\?\` verbatim paths).
fn steam_libraries() -> Vec<PathBuf> {
    let mut libs: Vec<PathBuf> = Vec::new();
    for root in steam_roots().into_iter().filter(|r| r.is_dir()) {
        // A missing or unreadable list just means no extra libraries.
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        let listed = std::fs::read_to_string(&vdf).map(|t| library_paths(&t)).unwrap_or_default();
        for lib in std::iter::once(root).chain(listed) {
            let lib = if cfg!(windows) { lib } else { lib.canonicalize().unwrap_or(lib) };
            if !libs.contains(&lib) {
                libs.push(lib);
            }
        }
    }
    libs
}

/// `rel` (a relative path written with `\` or `/`, as in the original's scripts) under `base`,
/// matched the way Windows does: separators either way, letter case ignored. The exact path is
/// tried first; off Windows each component that misses is looked up in its folder's listing
/// without case. `None` if nothing matches.
pub fn find_case_insensitive(base: &Path, rel: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = rel.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".").collect();
    let exact = parts.iter().fold(base.to_path_buf(), |p, c| p.join(c));
    if exact.exists() {
        return Some(exact);
    }
    if cfg!(windows) {
        return None;
    }
    let mut path = base.to_path_buf();
    for part in parts {
        let direct = path.join(part);
        path = if direct.exists() {
            direct
        } else {
            std::fs::read_dir(&path)
                .ok()?
                .filter_map(Result::ok)
                .find(|e| e.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case(part)))?
                .path()
        };
    }
    Some(path)
}

/// The `"path"` values of a Steam `libraryfolders.vdf` (KeyValues text; backslashes escaped).
fn library_paths(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('"').skip(1).step_by(2);
            (parts.next()? == "path").then(|| parts.next()).flatten()
        })
        .map(|p| PathBuf::from(p.replace(r"\\", r"\")))
        .collect()
}

/// A DWORD value from a Wine registry file (`user.reg`), e.g. key
/// `Software\The Creative Assembly\Napoleon`, value `nap_unlock`. `None` if absent.
pub fn wine_reg_dword(reg_text: &str, key: &str, value: &str) -> Option<u32> {
    let escaped_key = key.replace('\\', r"\\");
    let mut in_key = false;
    for line in reg_text.lines().map(str::trim_start) {
        if let Some(section) = line.strip_prefix('[') {
            // Section lines are `[Key\\Path] <timestamp>`; Wine writes names case-preserved but
            // the registry compares key and value names without case.
            in_key = section.split(']').next().is_some_and(|k| k.eq_ignore_ascii_case(&escaped_key));
        } else if in_key {
            // Value lines are `"name"=dword:0000000a`.
            let Some((name, data)) = line.strip_prefix('"').and_then(|l| l.split_once("\"=")) else { continue };
            if name.eq_ignore_ascii_case(value) {
                return data.strip_prefix("dword:").and_then(|hex| u32::from_str_radix(hex.trim(), 16).ok());
            }
        }
    }
    None
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_library_paths() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/u/.local/share/Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
        assert_eq!(library_paths(vdf), vec![PathBuf::from("/home/u/.local/share/Steam"), PathBuf::from(r"D:\SteamLibrary")]);
    }

    #[test]
    fn reads_wine_dword() {
        let reg = "WINE REGISTRY Version 2\n\n[Software\\\\Other] 1\n\"nap_unlock\"=dword:00000002\n\n[Software\\\\The Creative Assembly\\\\Napoleon] 1700000000\n#time=1\n\"nap_unlock\"=dword:00000004\n";
        assert_eq!(wine_reg_dword(reg, r"Software\The Creative Assembly\Napoleon", "nap_unlock"), Some(4));
        assert_eq!(wine_reg_dword(reg, r"Software\The Creative Assembly\Napoleon", "missing"), None);
        assert_eq!(wine_reg_dword(reg, r"Software\Nothing", "nap_unlock"), None);
        assert_eq!(wine_reg_dword(reg, r"software\the creative assembly\napoleon", "NAP_UNLOCK"), Some(4), "names without case");
    }

    /// One precedence for install and data folder: `NAPOLEON_INSTALL_DIR`, `NTW_DATA_DIR`, search.
    #[test]
    fn install_and_data_folders_follow_one_precedence() {
        let found = || PathBuf::from("/steam/common/ntw");
        let p = |s: &str| Some(PathBuf::from(s));
        let both = Overrides { install: p("/a"), data: p("/b/data") };
        assert_eq!(both.install_dir(found), PathBuf::from("/a"));
        assert_eq!(both.data_dir(found), PathBuf::from("/a/data"), "the install override wins for data too");
        let data_only = Overrides { install: None, data: p("/b/data") };
        assert_eq!(data_only.install_dir(found), PathBuf::from("/b"));
        assert_eq!(data_only.data_dir(found), PathBuf::from("/b/data"));
        let none = Overrides { install: None, data: None };
        assert_eq!(none.install_dir(found), found());
        assert_eq!(none.data_dir(found), found().join("data"));
    }

    #[test]
    fn finds_paths_with_either_separator_and_any_case() {
        let base = std::env::temp_dir().join(format!("ntw_install_case_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("Sub")).unwrap();
        std::fs::write(base.join("Sub").join("MyMod.pack"), b"").unwrap();
        assert_eq!(find_case_insensitive(&base, "Sub/MyMod.pack"), Some(base.join("Sub").join("MyMod.pack")));
        assert!(find_case_insensitive(&base, r"Sub\MyMod.pack").is_some(), "backslash separators");
        if !cfg!(windows) {
            assert_eq!(find_case_insensitive(&base, r"sub\mymod.PACK"), Some(base.join("Sub").join("MyMod.pack")));
        }
        assert_eq!(find_case_insensitive(&base, "sub/other.pack"), None);
        std::fs::remove_dir_all(&base).unwrap();
    }
}

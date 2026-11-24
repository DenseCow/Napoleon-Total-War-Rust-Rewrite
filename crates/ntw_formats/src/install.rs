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

/// The original's install folder: `$NAPOLEON_INSTALL_DIR` if set, else the parent of
/// `$NTW_DATA_DIR` if set, else the first Steam library holding the game, else the platform's
/// default Steam location (which may not exist; readers report the missing files).
pub fn install_dir() -> PathBuf {
    if let Some(dir) = env_path(INSTALL_DIR_ENV) {
        return dir;
    }
    if let Some(parent) = env_path(DATA_DIR_ENV).and_then(|d| d.parent().map(Path::to_path_buf)) {
        return parent;
    }
    find_install().unwrap_or_else(default_install_dir)
}

/// The original's `data` folder: `$NTW_DATA_DIR` if set, else `<install_dir()>/data`.
pub fn data_dir() -> PathBuf {
    env_path(DATA_DIR_ENV).unwrap_or_else(|| install_dir().join("data"))
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
    let install = install_dir();
    // `<library>/steamapps/common/<folder>` → `<library>`.
    if let Some(lib) = install.parent().and_then(Path::parent).and_then(Path::parent) {
        let pfx = compat(lib);
        if pfx.is_dir() {
            return Some(pfx);
        }
    }
    steam_libraries().iter().map(|l| compat(l)).find(|p| p.is_dir())
}

/// The first Steam library whose `steamapps/common/<folder>` holds a `data` folder.
fn find_install() -> Option<PathBuf> {
    steam_libraries()
        .into_iter()
        .map(|lib| lib.join("steamapps").join("common").join(STEAM_FOLDER))
        .find(|dir| dir.join("data").is_dir())
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
/// without duplicates (the `~/.steam` links usually point at the same folder).
fn steam_libraries() -> Vec<PathBuf> {
    let mut libs: Vec<PathBuf> = Vec::new();
    for root in steam_roots().into_iter().filter(|r| r.is_dir()) {
        // A missing or unreadable list just means no extra libraries.
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        let listed = std::fs::read_to_string(&vdf).map(|t| library_paths(&t)).unwrap_or_default();
        for lib in std::iter::once(root).chain(listed) {
            let lib = lib.canonicalize().unwrap_or(lib);
            if !libs.contains(&lib) {
                libs.push(lib);
            }
        }
    }
    libs
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
    let wanted = format!("\"{value}\"=dword:");
    let mut in_key = false;
    for line in reg_text.lines() {
        if let Some(section) = line.strip_prefix('[') {
            // Section lines are `[Key\\Path] <timestamp>`; Wine writes keys case-preserved but
            // the registry is case-insensitive.
            in_key = section.split(']').next().is_some_and(|k| k.eq_ignore_ascii_case(&escaped_key));
        } else if let Some(hex) = line.strip_prefix(&wanted).filter(|_| in_key) {
            return u32::from_str_radix(hex.trim(), 16).ok();
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
    }
}

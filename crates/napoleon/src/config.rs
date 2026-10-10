//! Program configuration.
//!
//! The only setting so far is where the original Napoleon: Total War is installed. The game
//! READS the original files from there (read-only) and never copies or changes them.

use std::path::PathBuf;

/// The default Steam install location.
pub const DEFAULT_INSTALL_DIR: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War";

/// Name of the environment variable that overrides the install location.
pub const INSTALL_DIR_ENV: &str = "NAPOLEON_INSTALL_DIR";

/// The original game's `data` folder: `$NAPOLEON_INSTALL_DIR\data` if that variable is set,
/// otherwise the default Steam location.
pub fn game_data_dir() -> PathBuf {
    let install = std::env::var_os(INSTALL_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_INSTALL_DIR));
    install.join("data")
}

/// The original game's own user folder, `%APPDATA%\The Creative Assembly\Napoleon` (saves,
/// preferences). NapoleonRust only READS it.
pub fn original_user_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("The Creative Assembly").join("Napoleon"))
}

/// Name of the environment variable that overrides NapoleonRust's own user folder.
pub const USER_DIR_ENV: &str = "NAPOLEONRUST_USER_DIR";

/// NapoleonRust's own user folder (our preferences copy lives in its `scripts\`):
/// `$NAPOLEONRUST_USER_DIR` if set, otherwise `%APPDATA%\NapoleonRust`. It is never the original
/// game's folder, so the original's settings stay untouched.
pub fn user_dir() -> Option<PathBuf> {
    std::env::var_os(USER_DIR_ENV)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("NapoleonRust")))
}

/// Napoleon's campaign progress `nap_unlock` (1..=4). The original reads it from the registry value
/// `HKCU\Software\The Creative Assembly\Napoleon\nap_unlock` (default 1, clamped to 1..4;
/// CONFIRMED `0x0047B750`). We read the same value (read only, with `reg query`), default 1.
pub fn nap_unlock() -> u32 {
    let out = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\The Creative Assembly\Napoleon", "/v", "nap_unlock"])
        .output();
    let Ok(out) = out else { return 1 };
    let text = String::from_utf8_lossy(&out.stdout);
    let value = text
        .lines()
        .find(|l| l.contains("nap_unlock"))
        .and_then(|l| l.split_whitespace().last())
        .and_then(|v| v.strip_prefix("0x").map_or_else(|| v.parse().ok(), |h| u32::from_str_radix(h, 16).ok()));
    value.unwrap_or(1).clamp(1, 4)
}

/// Our easy-mod folder next to the program: `<exe folder>\mods` (packs and loose folders, with an
/// optional `load_order.txt`; see `ntw_formats::pack::ModOptions`).
pub fn default_mods_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("mods"))
}

/// The mods the game loads, from the command line:
/// - `--no-mods`: none (the install alone);
/// - otherwise the original's `user.script.txt` `mod` lines (`--user-script <file>`, else the
///   original's own file under `%APPDATA%`, read-only) and our mods folder (`--mods <dir>`, else
///   [`default_mods_dir`]).
///
/// Also returns a warning for each of these flags given without its value (the default is then used).
pub fn mod_options_from_args(args: &[String]) -> (ntw_formats::pack::ModOptions, Vec<String>) {
    use ntw_formats::pack::{ModOptions, UserScriptSetting};
    if args.iter().any(|a| a == "--no-mods") {
        return (ModOptions::vanilla(), Vec::new());
    }
    let mut warnings = Vec::new();
    let mut value = |flag: &str, what: &str, default: &str| {
        let i = args.iter().position(|a| a == flag)?;
        let v = args.get(i + 1).filter(|v| !v.starts_with("--"));
        if v.is_none() {
            warnings.push(format!("{flag} needs {what}; using {default}"));
        }
        v
    };
    let user_script = value("--user-script", "a file", "the original's user.script.txt")
        .map_or(UserScriptSetting::Default, |p| UserScriptSetting::File(PathBuf::from(p)));
    let mods_dir = value("--mods", "a folder", "the mods folder next to the program").map(PathBuf::from).or_else(default_mods_dir);
    (ModOptions { user_script, mods_dir }, warnings)
}

/// Applies the mod setting ([`mod_options_from_args`]) for every later `Vfs::open_install`. Call
/// it once at start-up, before anything opens the install. With `--list-mods` it prints what
/// would load (layers, files from mods, warnings) and returns `true`: the caller then exits.
pub fn apply_mod_setting(args: &[String]) -> bool {
    let (options, warnings) = mod_options_from_args(args);
    for w in &warnings {
        eprintln!("WARN napoleon: {w}");
    }
    if args.iter().any(|a| a == "--list-mods") {
        let dir = game_data_dir();
        match ntw_formats::pack::Vfs::open_with_mods(&dir, &options) {
            Ok((vfs, report)) => print!("{}", report.render(&vfs)),
            Err(e) => eprintln!("cannot open the install at {}: {e}", dir.display()),
        }
        return true;
    }
    ntw_formats::pack::set_mod_options(Some(options));
    false
}

/// Applies the text language setting: `--language <code>` on the command line, else the code in
/// NapoleonRust's own `language.txt` (in [`user_dir`]; same one-line form as the install's
/// `data\language.txt`, e.g. `FR`), else nothing (the install's `language.txt` is used). A code
/// whose `local_<code>*.pack` is not installed is ignored by the VFS (see
/// `ntw_formats::pack::set_language_override`). The install's own file is never written.
pub fn apply_language_setting(args: &[String]) {
    let from_args = args.iter().position(|a| a == "--language").and_then(|i| args.get(i + 1)).cloned();
    let from_file = || user_dir().and_then(|d| std::fs::read_to_string(d.join("language.txt")).ok());
    let lang = from_args.or_else(from_file).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    ntw_formats::pack::set_language_override(lang.as_deref());
    let dir = game_data_dir();
    let effective = ntw_formats::pack::effective_language(&dir);
    if let Some(l) = &lang
        && !l.eq_ignore_ascii_case(&effective)
    {
        eprintln!(
            "language {l} is not installed (installed: {:?}); using {effective}",
            ntw_formats::pack::installed_languages(&dir)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntw_formats::pack::UserScriptSetting;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// `--mods` / `--user-script` without a value warn and fall back to the defaults.
    #[test]
    fn mod_flags_without_a_value_warn() {
        let (o, w) = mod_options_from_args(&args(&["napoleon", "--mods"]));
        assert_eq!(o.mods_dir, default_mods_dir());
        assert_eq!(w, ["--mods needs a folder; using the mods folder next to the program"]);
        let (o, w) = mod_options_from_args(&args(&["napoleon", "--user-script", "--mods", "m"]));
        assert_eq!(o.user_script, UserScriptSetting::Default);
        assert_eq!(o.mods_dir, Some(PathBuf::from("m")));
        assert_eq!(w.len(), 1);
        let (_, w) = mod_options_from_args(&args(&["napoleon", "--mods", "m", "--user-script", "u.txt"]));
        assert!(w.is_empty());
    }
}

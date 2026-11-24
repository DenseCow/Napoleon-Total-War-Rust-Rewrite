//! Shared test helpers (also included by `ntw_script`'s `campaign_ui.rs` through `#[path]`).

use std::path::PathBuf;

/// The vanilla saves (read-only evidence copies): `NTW_EVIDENCE_DIR`, else
/// `Documents/ntw-evidence/saves` in `%USERPROFILE%` (or `$HOME`). The folder may not exist; tests skip then.
pub fn evidence_dir() -> PathBuf {
    std::env::var_os("NTW_EVIDENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default().join("Documents").join("ntw-evidence").join("saves"))
}

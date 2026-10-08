//! Repo check: no source or note file is saved with a UTF-8 byte-order mark or carries mojibake
//! (UTF-8 read as Windows-1252 and saved again, as Windows PowerShell 5.1's `Get-Content` /
//! `Set-Content` do). A worker's round trip through them corrupted `battle/view.rs` on 2026-10-07.
//!
//! Checks every tracked `.rs`, `.md`, `.lua` and `.toml` file under `crates/`, `docs/`, `analysis/`
//! and the repository root.

use std::path::Path;
use std::process::Command;

/// The sequences a UTF-8 → Windows-1252 → UTF-8 round trip leaves: `Ã` (from `×`, `é`, `ü`...),
/// `Â§` / `Â±` (`§`, `±`), `â€` (`—`, `’`, `“`...), `â†` (`→`, `←`...).
const MOJIBAKE: [&str; 5] = ["Ã", "Â§", "Â±", "â€", "â†"];

/// Files that legitimately contain one of [`MOJIBAKE`], with the reason.
const ALLOWED: [(&str, &str); 2] = [
    ("crates/napoleon/tests/encoding.rs", "this check names the sequences it looks for"),
    ("docs/archive/HANDOFF_2026-10-05_to_06.md", "an old session log quotes the `Â§` it repaired in BACKLOG.md"),
];

fn checked(path: &str) -> bool {
    let ext_ok = [".rs", ".md", ".lua", ".toml"].iter().any(|e| path.ends_with(e));
    let dir_ok = !path.contains('/') || ["crates/", "docs/", "analysis/"].iter().any(|d| path.starts_with(d));
    ext_ok && dir_ok
}

#[test]
fn no_bom_or_mojibake_in_tracked_files() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = match Command::new("git").arg("-C").arg(&root).args(["ls-files", "-z"]).output() {
        Ok(o) if o.status.success() => o.stdout,
        Ok(o) => panic!("git ls-files failed: {}", String::from_utf8_lossy(&o.stderr)),
        Err(e) => {
            eprintln!("skipped: git is not available ({e})");
            return;
        }
    };
    let mut bad = Vec::new();
    let mut files = 0;
    for path in out.split(|&b| b == 0).filter(|p| !p.is_empty()) {
        let path = String::from_utf8_lossy(path);
        if !checked(&path) {
            continue;
        }
        // A tracked file deleted in the working tree is not this check's business.
        let Ok(bytes) = std::fs::read(root.join(path.as_ref())) else { continue };
        files += 1;
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            bad.push(format!("{path}: starts with a UTF-8 BOM"));
        }
        if ALLOWED.iter().any(|(p, _)| *p == path) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (n, line) in text.lines().enumerate() {
            if let Some(m) = MOJIBAKE.iter().find(|m| line.contains(**m)) {
                bad.push(format!("{path}:{}: mojibake {m:?}", n + 1));
            }
        }
    }
    assert!(files > 100, "only {files} files checked; is the repository root right?");
    assert!(bad.is_empty(), "{} encoding problems (restore the bytes; edit files with Edit/Write or bash, not PowerShell 5.1 Set-Content):\n{}", bad.len(), bad.join("\n"));
}

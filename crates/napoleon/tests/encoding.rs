//! Repo check: no source or note file is saved with a UTF-8 byte-order mark or carries mojibake
//! (UTF-8 read as Windows-1252 and saved again, as Windows PowerShell 5.1's `Get-Content` /
//! `Set-Content` do). A worker's round trip through them corrupted `battle/view.rs` on 2026-10-07.
//!
//! A file that is not valid UTF-8 (saved in the ANSI codepage) fails too.
//!
//! Checks every tracked text file (see [`EXTENSIONS`]) under `crates/`, `docs/`, `analysis/`,
//! `tools/`, `.opencode/` and the repository root. Skips cleanly when there is no `.git`.

use std::path::Path;
use std::process::Command;

/// The sequences a UTF-8 → Windows-1252 → UTF-8 round trip leaves: `Ã` (from `×`, `é`, `ü`...),
/// `Â§` / `Â±` / `Â°` / `Â·` (`§`, `±`, `°`, `·`), `Â` + NBSP (from a non-breaking space), `â€`
/// (`—`, `’`, `“`...), `â†` (`→`, `←`...).
const MOJIBAKE: [&str; 8] = ["Ã", "Â§", "Â±", "Â°", "Â·", "Â\u{a0}", "â€", "â†"];

/// Extensions of the checked text files.
const EXTENSIONS: [&str; 8] = [".rs", ".md", ".lua", ".toml", ".sh", ".ps1", ".json", ".yml"];

/// Files that legitimately contain one of [`MOJIBAKE`], with the reason.
const ALLOWED: [(&str, &str); 2] = [
    ("crates/napoleon/tests/encoding.rs", "this check names the sequences it looks for"),
    ("docs/archive/HANDOFF_2026-10-05_to_06.md", "an old session log quotes the `Â§` it repaired in BACKLOG.md"),
];

fn checked(path: &str) -> bool {
    let ext_ok = EXTENSIONS.iter().any(|e| path.ends_with(e));
    let dir_ok = !path.contains('/') || ["crates/", "docs/", "analysis/", "tools/", ".opencode/"].iter().any(|d| path.starts_with(d));
    ext_ok && dir_ok
}

/// The encoding problems in one file's bytes, one message per problem.
fn scan(path: &str, bytes: &[u8]) -> Vec<String> {
    let mut bad = Vec::new();
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bad.push(format!("{path}: starts with a UTF-8 BOM"));
    }
    if ALLOWED.iter().any(|(p, _)| *p == path) {
        return bad;
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => {
            bad.push(format!("{path}: not valid UTF-8 (saved in the ANSI codepage?), first bad byte at {}", e.valid_up_to()));
            return bad;
        }
    };
    for (n, line) in text.lines().enumerate() {
        if let Some(m) = MOJIBAKE.iter().find(|m| line.contains(**m)) {
            bad.push(format!("{path}:{}: mojibake {m:?}", n + 1));
        }
    }
    bad
}

#[test]
fn scan_flags_each_kind_of_damage() {
    assert!(scan("a.rs", "plain \u{b0} text \u{a0}\n".as_bytes()).is_empty());
    assert_eq!(scan("a.rs", b"\xEF\xBB\xBFfn main() {}").len(), 1, "BOM");
    assert_eq!(scan("a.md", b"caf\xE9 au lait").len(), 1, "Windows-1252 byte");
    for bad in ["30\u{c2}\u{b0}", "a \u{c2}\u{b7} b", "x\u{c2}\u{a0}y", "\u{c3}\u{97}", "\u{e2}\u{20ac}\u{201d}"] {
        assert_eq!(scan("a.md", bad.as_bytes()).len(), 1, "{bad:?}");
    }
    assert!(scan(ALLOWED[0].0, "\u{c3}\u{97}".as_bytes()).is_empty(), "allow-listed file");
}

#[test]
fn no_bom_or_mojibake_in_tracked_files() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = match Command::new("git").arg("-C").arg(&root).args(["ls-files", "-z"]).output() {
        Ok(o) if o.status.success() => o.stdout,
        Ok(o) => {
            eprintln!("skipped: not a git checkout ({})", String::from_utf8_lossy(&o.stderr).trim());
            return;
        }
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
        bad.extend(scan(&path, &bytes));
    }
    assert!(files > 100, "only {files} files checked; is the repository root right?");
    assert!(bad.is_empty(), "{} encoding problems (restore the bytes; edit files with Edit/Write or bash, not PowerShell 5.1 Set-Content):\n{}", bad.len(), bad.join("\n"));
}

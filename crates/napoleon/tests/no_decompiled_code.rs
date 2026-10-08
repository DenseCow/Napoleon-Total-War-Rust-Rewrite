//! Repo check: the notes describe the exe's behaviour in words; they never quote Ghidra's decompiled
//! C (CLAUDE.md, "Hard rules": "Never paste decompiled code into source or docs"). Addresses, field
//! offsets, constants, symbol names (`FUN_00ED49A0`, `DAT_013305F8`), short disassembly and prose
//! naming one parameter ("`param_3` is 1") all stay; agents re-decompile live in Ghidra.
//!
//! Checks every tracked `.md` file under `docs/`, `analysis/` and the repository root. A line fails
//! when it holds either shape that only decompiler output has:
//! - a decompiler temporary: `iVar7`, `uVar2`, `piVar4`, `bVar10` (1-3 lowercase letters, `Var`,
//!   digits), `iStack_14`, `local_14` / `local_f0` (hex suffix), `in_ECX` / `in_stack_..`,
//!   `extraout_..`, `unaff_..`, or `CONCAT44` / `SUB41` / `ZEXT14` / `SEXT24`;
//! - a C pointer-cast dereference: `*(int *)`, `**(int **)`, `*(unsigned short *)`.
//!
//! Rewrite a flagged line as behaviour: "reads the dword at `this + 0x2C` when the third argument is 0".
//! There is no allow-list: no note needs one.

use std::path::Path;
use std::process::Command;

fn checked(path: &str) -> bool {
    let dir_ok = !path.contains('/') || ["docs/", "analysis/"].iter().any(|d| path.starts_with(d));
    path.ends_with(".md") && dir_ok
}

/// `iVar7`, `piVar4`, `iStack_14`: 1-3 lowercase letters, then `marker`, then a suffix accepted by `ok`.
fn prefixed_temp(word: &str, marker: &str, ok: impl Fn(&str) -> bool) -> bool {
    match word.find(marker) {
        Some(i) => {
            let (pre, suffix) = (&word[..i], &word[i + marker.len()..]);
            (1..=3).contains(&pre.len()) && pre.bytes().all(|b| b.is_ascii_lowercase()) && ok(suffix)
        }
        None => false,
    }
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether one identifier is a name only Ghidra's decompiler invents.
fn is_temporary(word: &str) -> bool {
    if prefixed_temp(word, "Var", digits) || prefixed_temp(word, "Stack_", hex) {
        return true;
    }
    if word.strip_prefix("local_").is_some_and(hex) {
        return true;
    }
    // `in_ECX`, `in_FS_OFFSET`, `in_stack_00000008`: a register or stack slot read without a parameter.
    if let Some(rest) = word.strip_prefix("in_") {
        let register = rest.starts_with(|c: char| c.is_ascii_uppercase())
            && rest.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
        if register || rest.starts_with("stack_") {
            return true;
        }
    }
    if word.starts_with("extraout_") || word.starts_with("unaff_") {
        return true;
    }
    ["CONCAT", "SUB", "ZEXT", "SEXT"].iter().any(|p| word.strip_prefix(p).is_some_and(|n| n.len() == 2 && digits(n)))
}

/// The first `*(T *)` dereference cast in `line`: `*(` then one or more type words, then only `*`s.
fn pointer_cast(line: &str) -> Option<&str> {
    let mut from = 0;
    while let Some(i) = line[from..].find("*(") {
        let start = from + i;
        let inner_start = start + 2;
        let len = line[inner_start..].find(')')?;
        let inner = &line[inner_start..inner_start + len];
        let (ty, stars) = inner.split_at(inner.trim_end_matches(['*', ' ']).len());
        let is_type = !ty.is_empty() && ty.split(' ').all(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
        if is_type && stars.contains('*') {
            return Some(&line[start..inner_start + len + 1]);
        }
        from = inner_start;
    }
    None
}

/// What makes `line` look like decompiler output, or `None`.
fn decompiler_shape(line: &str) -> Option<String> {
    if let Some(w) = line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).find(|w| is_temporary(w)) {
        return Some(format!("decompiler temporary `{w}`"));
    }
    pointer_cast(line).map(|c| format!("pointer cast `{c}`"))
}

#[test]
fn matcher_flags_decompiler_lines_and_not_prose() {
    let flagged = [
        "`if (param_3 == '\\0') iVar7 = *(int *)(in_ECX + 0x2c);`",
        "uVar2 = 1; piVar4 += 6; bVar10 ^ 1",
        "`*local_14 = param_5;`",
        "UIFileInPtr(local_f0, x)",
        "iStack_14 = first_slot * 0x18",
        "CONCAT44(a, b) and SUB41(x, 0)",
        "extraout_EAX, unaff_ESI, in_stack_00000008, in_FS_OFFSET",
        "reads `*(short *)(handle + 0x50)`",
        "`**(int **)(this + 0x218)`",
        "`*(unsigned short *)(p + 2)`",
    ];
    for line in flagged {
        assert!(decompiler_shape(line).is_some(), "not flagged: {line}");
    }
    let prose = [
        "`param_1` is the unit, and the panel's `param_3` is `1`",
        "`param_2`",
        "the 13 `param_1` constants are not the enum",
        "reads the dword at `this + 0x2C`; `*(this + 0x14)` is the clock",
        "`a * b` and `(x * 2)` and *(emphasis)*",
        "`FUN_00ed49a0` reads `DAT_013305F8`; `PTR_DAT_0146ce34`",
        "`MOV [ESI + 0x14],ECX` / `DIV dword ptr [ESI+0x14]`",
        "local_variable, invalid_ECX, VarName, iVar, SUBSYSTEM, in_game, insta_ECX",
    ];
    for line in prose {
        assert_eq!(decompiler_shape(line), None, "flagged prose: {line}");
    }
}

#[test]
fn no_decompiled_code_in_tracked_notes() {
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
        let text = String::from_utf8_lossy(&bytes);
        for (n, line) in text.lines().enumerate() {
            if let Some(why) = decompiler_shape(line) {
                bad.push(format!("{path}:{}: {why}", n + 1));
            }
        }
    }
    assert!(files > 20, "only {files} files checked; is the repository root right?");
    assert!(bad.is_empty(), "{} lines quote decompiled code (describe the behaviour in words, keeping addresses and offsets):\n{}", bad.len(), bad.join("\n"));
}

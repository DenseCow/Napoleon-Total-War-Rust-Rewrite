//! Repo check (CLAUDE.md "Modding seams"): game tables are read only through the one merged table
//! reader, `ntw_formats::db_folder`, never by a `db\<table>_tables\...` path. A reader that opens
//! `db/units_tables/units` itself sees only the one file of that name and misses a mod's additive
//! and `bob_` files (BACKLOG §11 "One table path").
//!
//! Scans the game code: every `.rs` file under `crates/*/src`, without its `#[cfg(test)]` items
//! (unit tests build small packs with table paths on purpose). Examples and integration tests are
//! development probes of the shipped files and are not scanned.

use std::path::{Path, PathBuf};

/// The loader itself: the only game code that may name a table folder.
const ALLOWED: [&str; 1] = ["crates/ntw_formats/src/db_folder.rs"];

/// How many lines a path split over string literals (`concat!`, `+`, `format!` pieces) may span.
const WINDOW: usize = 4;

/// True if `text` (string literal contents) names a table folder: `db`, one or more `/` or `\`,
/// then a `_tables` segment later on, in any case (`/DB/units_tables/`, `data\\db\\x_tables`).
fn names_table(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    lower.match_indices("db").any(|(i, _)| {
        let after = &bytes[i + 2..];
        let seps = after.iter().take_while(|&&b| b == b'/' || b == b'\\').count();
        seps > 0 && lower[i + 2 + seps..].contains("_tables")
    })
}

/// The contents of the string literals on one line, joined (normal, byte and raw strings; a `"`
/// char literal is not a string).
fn literals(line: &str) -> String {
    let line = line.replace("'\"'", "");
    let chars: Vec<char> = line.chars().collect();
    let (mut out, mut i) = (String::new(), 0);
    while i < chars.len() {
        if chars[i] != '"' {
            i += 1;
            continue;
        }
        // A raw string: `r"`, `r#"`, `br##"`...; it ends at `"` and as many `#`.
        let mut hashes = 0;
        let mut k = i;
        while k > 0 && chars[k - 1] == '#' {
            hashes += 1;
            k -= 1;
        }
        let raw = k > 0 && chars[k - 1] == 'r';
        i += 1;
        while i < chars.len() {
            if !raw && chars[i] == '\\' {
                out.push(chars[i]);
                if let Some(&c) = chars.get(i + 1) {
                    out.push(c);
                }
                i += 2;
                continue;
            }
            if chars[i] == '"' && (!raw || chars[i + 1..].iter().take(hashes).filter(|&&c| c == '#').count() == hashes) {
                i += 1 + if raw { hashes } else { 0 };
                break;
            }
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// The lines of `text` that name a `db\..._tables` path in string literals (one line, or literals
/// split over up to [`WINDOW`] lines), with their numbers. Comments and `#[cfg(test)]` items are
/// skipped.
fn table_paths(text: &str) -> Vec<(usize, String)> {
    let mut scanned: Vec<(usize, &str, String)> = Vec::new();
    let mut skip_depth: Option<i32> = None;
    let mut pending_test_item = false;
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(depth) = skip_depth.as_mut() {
            *depth += brace_balance(line);
            if *depth <= 0 {
                skip_depth = None;
            }
            continue;
        }
        if trimmed.starts_with("#[cfg(test)]") {
            pending_test_item = true;
            continue;
        }
        if pending_test_item {
            if trimmed.starts_with("#[") || trimmed.starts_with("//") {
                continue;
            }
            pending_test_item = false;
            let balance = brace_balance(line);
            if balance > 0 {
                skip_depth = Some(balance);
            }
            // A one-line item (`#[cfg(test)] use ...;` or `fn f() {}`) ends here.
            continue;
        }
        if trimmed.starts_with("//") {
            continue;
        }
        scanned.push((n + 1, line, literals(line)));
    }
    let mut found = Vec::new();
    let join = |from: usize, to: usize| -> String { scanned[from..=to].iter().map(|(_, _, lit)| lit.as_str()).collect() };
    let mut i = 0;
    while i < scanned.len() {
        let mut next = i + 1;
        if !scanned[i].2.is_empty() {
            // The shortest run of lines in a row, starting here, whose literals name a table and
            // need this line to do so (a split path is reported at its first line).
            let last = (i..scanned.len().min(i + WINDOW)).take_while(|&j| scanned[j].0 == scanned[i].0 + (j - i)).last().unwrap_or(i);
            if let Some(j) = (i..=last).find(|&j| names_table(&join(i, j)) && (j == i || !names_table(&join(i + 1, j)))) {
                found.push((scanned[i].0, scanned[i].1.trim().to_owned()));
                next = j.max(i) + 1;
            }
        }
        i = next;
    }
    found
}

/// `{` minus `}` on a line, outside string and char literals (good enough for this check).
fn brace_balance(line: &str) -> i32 {
    let (mut balance, mut in_str, mut prev) = (0, false, ' ');
    let line = line.replace("'{'", "").replace("'}'", "").replace("'\"'", "");
    for c in line.chars() {
        match c {
            '"' if prev != '\\' => in_str = !in_str,
            '{' if !in_str => balance += 1,
            '}' if !in_str => balance -= 1,
            _ => {}
        }
        prev = c;
    }
    balance
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The shapes that slipped past a plain `"db/` search: a leading separator, capitals, a `data/`
/// prefix, doubled or mixed separators, raw strings with hashes, and a path split over literals
/// (on one line or over lines).
#[test]
fn the_check_is_not_bypassed() {
    let caught = [
        r#"vfs.read("/db/units_tables/units")"#,
        r#"vfs.read("DB/Units_Tables/units")"#,
        r#"source.find("data/db/battles_tables/battles")"#,
        r#"let p = "db\\units_tables\\units";"#,
        r##"let p = r#"db\units_tables\units"#;"##,
        r#"let p = "db//x_tables/x";"#,
        r#"let p = concat!("db/", "units", "_tables/units");"#,
        r#"let p = format!("{}{}", "db\\", "units_tables");"#,
        "let p = concat!(\n    \"db/\",\n    \"units_tables/units\",\n);",
        "let base = \"DB\\\\\";\nlet p = base.to_owned() + \"units\" + \"_tables\";",
    ];
    for code in caught {
        assert_eq!(table_paths(code).len(), 1, "missed: {code}");
    }
    let fine = [
        r#"let p = "animations/animation_tables/animation_tables.txt";"#,
        r#"let p = "db_tables";"#,
        r#"let p = "dbx/units_tables";"#,
        "// \"db/units_tables/units\" in a comment",
        "let c = '\"'; let d = \"db\"; let e = \"x\";",
    ];
    for code in fine {
        assert!(table_paths(code).is_empty(), "false hit: {code}");
    }
}

#[test]
fn the_check_sees_each_way_of_naming_a_table() {
    let code = "let a = vfs.read(\"db/units_tables/units\");\n\
                let b = format!(\"db/{t}_tables/{t}\");\n\
                const C: &str = r\"db\\units_tables\\units\";\n\
                // db/units_tables/units in a comment is fine\n\
                let d = \"animations/animation_tables/x.txt\";\n\
                #[cfg(test)]\n\
                mod tests {\n\
                    fn f() { let e = \"db/x_tables/x\"; }\n\
                }\n\
                let g = vfs.list(\"db/models_building_tables\");\n";
    let lines: Vec<usize> = table_paths(code).into_iter().map(|(n, _)| n).collect();
    assert_eq!(lines, [1, 2, 3, 10]);
}

#[test]
fn no_game_code_reads_a_table_path_directly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut files = Vec::new();
    for krate in std::fs::read_dir(root.join("crates")).expect("crates folder").flatten() {
        rust_files(&krate.path().join("src"), &mut files);
    }
    assert!(files.len() > 100, "found only {} source files; wrong root?", files.len());
    let mut bad = Vec::new();
    for f in files {
        let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        if ALLOWED.contains(&rel.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        for (n, line) in table_paths(&text) {
            bad.push(format!("{rel}:{n}: {line}"));
        }
    }
    assert!(
        bad.is_empty(),
        "read these tables through ntw_formats::db_folder (RawTable / merged_rows) or ntw_data, not by path:\n{}",
        bad.join("\n")
    );
}

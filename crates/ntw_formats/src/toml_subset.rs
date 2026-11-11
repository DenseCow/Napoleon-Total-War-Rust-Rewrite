//! A reader for the part of TOML our own open text formats use (the material descriptions of
//! [`crate::material`], DESIGN.md §3.7.2), so a modder's file opens in any TOML editor and this
//! crate needs no parser dependency.
//!
//! What it reads:
//!
//! ```text
//! # A comment runs from '#' to the end of the line.
//! shader = "shaders/wet_stone.wgsl"     # keys before the first [table] are the top-level table
//!
//! [roughness]                           # a table: a bare name in square brackets
//! value = 0.8                           # a number
//! texture = "gloss.a"                   # a string: "basic" (with \" \\ \n \t) or 'literal'
//! enabled = true                        # a boolean
//! tint = [1.0, 0.95, 0.9]               # a list on one line (lists may nest)
//! ```
//!
//! What it refuses, with the line number: everything else TOML has (dotted names, `[[arrays of
//! tables]]`, inline tables, multi-line strings and lists, dates, `inf` and `nan`), a table or a
//! key given twice, and anything that is not TOML at all. A file this reader accepts is valid TOML
//! with the same meaning.

/// One `[table]` of a file: its name (empty for the keys before the first table), the line of its
/// header (0 for the top-level table) and its keys in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// The name in the square brackets; empty for the top-level table.
    pub name: String,
    /// The 1-based line of the `[name]` header; 0 for the top-level table.
    pub line: usize,
    /// The table's `key = value` lines in file order.
    pub entries: Vec<Entry>,
}

/// One `key = value` line.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The key (letters, digits, `_`, `-`).
    pub key: String,
    /// Its value.
    pub value: Value,
    /// The 1-based line it is on.
    pub line: usize,
}

/// A value of the subset.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A string.
    Text(String),
    /// A finite number (integers and floats are not told apart).
    Number(f64),
    /// `true` or `false`.
    Bool(bool),
    /// A one-line list.
    List(Vec<Value>),
}

impl Value {
    /// What the value is, for an error message ("a string", "a number", ...).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Text(_) => "a string",
            Self::Number(_) => "a number",
            Self::Bool(_) => "true/false",
            Self::List(_) => "a list",
        }
    }
}

/// Parses a file's text into its tables, the top-level table first (always there, possibly
/// empty), then each `[table]` in file order. An error names the first bad line (1-based).
pub fn parse(text: &str) -> Result<Vec<Table>, String> {
    let mut tables = vec![Table { name: String::new(), line: 0, entries: Vec::new() }];
    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let at = |message: String| format!("line {line_no}: {message}");
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            if rest.starts_with('[') {
                return Err(at("arrays of tables ([[name]]) are not in the subset this reader takes".to_owned()));
            }
            let Some((name, after)) = rest.split_once(']') else {
                return Err(at("a table header needs its closing ']'".to_owned()));
            };
            let name = name.trim();
            if !is_bare(name) {
                return Err(at(format!("table name {name:?} must be letters, digits, '_' or '-' (dotted names are not in the subset)")));
            }
            end_of_line(after).map_err(&at)?;
            if tables.iter().any(|t| t.name == name) {
                return Err(at(format!("table [{name}] is given twice")));
            }
            tables.push(Table { name: name.to_owned(), line: line_no, entries: Vec::new() });
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(at("expected `key = value` or a [table] header".to_owned()));
        };
        let key = key.trim();
        if !is_bare(key) {
            return Err(at(format!("key {key:?} must be letters, digits, '_' or '-' (quoted and dotted keys are not in the subset)")));
        }
        let (value, after) = parse_value(value).map_err(&at)?;
        end_of_line(after).map_err(&at)?;
        let table = tables.last_mut().expect("the top-level table is always there");
        if table.entries.iter().any(|e| e.key == key) {
            return Err(at(format!("key {key:?} is given twice")));
        }
        table.entries.push(Entry { key: key.to_owned(), value, line: line_no });
    }
    Ok(tables)
}

/// A bare TOML name: one or more of letters, digits, `_` and `-`.
fn is_bare(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// What follows a value or a header on its line must be blank or a comment.
fn end_of_line(rest: &str) -> Result<(), String> {
    let rest = rest.trim_start();
    if rest.is_empty() || rest.starts_with('#') { Ok(()) } else { Err(format!("unexpected text {rest:?} after the value")) }
}

/// Parses one value at the start of `text` (leading blanks skipped); returns it with the text
/// that follows it.
fn parse_value(text: &str) -> Result<(Value, &str), String> {
    let text = text.trim_start();
    let Some(first) = text.chars().next() else { return Err("a value is missing after '='".to_owned()) };
    match first {
        '"' => {
            if text.starts_with("\"\"\"") {
                return Err("multi-line strings are not in the subset this reader takes".to_owned());
            }
            let mut out = String::new();
            let mut chars = text[1..].char_indices();
            while let Some((i, c)) = chars.next() {
                match c {
                    '"' => return Ok((Value::Text(out), &text[1 + i + 1..])),
                    '\\' => match chars.next() {
                        Some((_, '"')) => out.push('"'),
                        Some((_, '\\')) => out.push('\\'),
                        Some((_, 'n')) => out.push('\n'),
                        Some((_, 't')) => out.push('\t'),
                        Some((_, other)) => return Err(format!("string escape \\{other} is not in the subset (use \\\" \\\\ \\n \\t)")),
                        None => break,
                    },
                    c => out.push(c),
                }
            }
            Err("a string needs its closing '\"'".to_owned())
        }
        '\'' => {
            if text.starts_with("'''") {
                return Err("multi-line strings are not in the subset this reader takes".to_owned());
            }
            match text[1..].split_once('\'') {
                Some((body, rest)) => Ok((Value::Text(body.to_owned()), rest)),
                None => Err("a string needs its closing \"'\"".to_owned()),
            }
        }
        '[' => {
            let mut items = Vec::new();
            let mut rest = text[1..].trim_start();
            loop {
                if let Some(after) = rest.strip_prefix(']') {
                    return Ok((Value::List(items), after));
                }
                if rest.is_empty() || rest.starts_with('#') {
                    return Err("a list must close with ']' on the line it opens on".to_owned());
                }
                let (item, after) = parse_value(rest)?;
                items.push(item);
                rest = after.trim_start();
                if let Some(after) = rest.strip_prefix(',') {
                    rest = after.trim_start();
                } else if !rest.starts_with(']') {
                    return Err("list items are separated by ','".to_owned());
                }
            }
        }
        '{' => Err("inline tables are not in the subset this reader takes".to_owned()),
        _ => {
            let end = text.find(|c: char| c == ',' || c == ']' || c == '#' || c.is_whitespace()).unwrap_or(text.len());
            let (word, rest) = text.split_at(end);
            let value = match word {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => Value::Number(parse_number(word).ok_or_else(|| format!("{word:?} is not a string (in quotes), a finite number, true or false"))?),
            };
            Ok((value, rest))
        }
    }
}

/// A TOML decimal number: an optional sign, digits (with `_` between them), an optional fraction
/// and exponent. `inf`, `nan` and the hex, octal and binary forms are not in the subset.
fn parse_number(word: &str) -> Option<f64> {
    let digits = word.strip_prefix(['+', '-']).unwrap_or(word);
    if !digits.starts_with(|c: char| c.is_ascii_digit()) || !digits.ends_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '_' | 'e' | 'E' | '+' | '-')) {
        return None;
    }
    // An underscore only between two digits.
    let bytes = digits.as_bytes();
    if (1..bytes.len() - 1).any(|i| bytes[i] == b'_' && !(bytes[i - 1].is_ascii_digit() && bytes[i + 1].is_ascii_digit())) {
        return None;
    }
    word.replace('_', "").parse::<f64>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::Text(s.to_owned())
    }

    /// Top-level keys, tables, every value kind, comments and blank lines parse in file order
    /// with their line numbers.
    #[test]
    fn parses_tables_and_values() {
        let src = "# header\nshader = \"a/b.wgsl\" # trailing\n\n[roughness]\nvalue = 0.8\ntexture = 'gloss.a'\non = true\noff=false\n  [base_colour]  # indented\ntint = [1, 0.5, -2e-1,]\nnames = [\"a # b\", 'c', [1, 2]]\nbig = 1_000\nquote = \"say \\\"hi\\\"\\n\"\n";
        let tables = parse(src).expect("parses");
        assert_eq!(tables.len(), 3);
        assert_eq!((tables[0].name.as_str(), tables[0].line), ("", 0));
        assert_eq!(tables[0].entries, [Entry { key: "shader".into(), value: text("a/b.wgsl"), line: 2 }]);
        assert_eq!((tables[1].name.as_str(), tables[1].line), ("roughness", 4));
        let values: Vec<(&str, &Value)> = tables[1].entries.iter().map(|e| (e.key.as_str(), &e.value)).collect();
        assert_eq!(values, [("value", &Value::Number(0.8)), ("texture", &text("gloss.a")), ("on", &Value::Bool(true)), ("off", &Value::Bool(false))]);
        assert_eq!((tables[2].name.as_str(), tables[2].line), ("base_colour", 9));
        assert_eq!(tables[2].entries[0].value, Value::List(vec![Value::Number(1.0), Value::Number(0.5), Value::Number(-0.2)]));
        assert_eq!(
            tables[2].entries[1].value,
            Value::List(vec![text("a # b"), text("c"), Value::List(vec![Value::Number(1.0), Value::Number(2.0)])])
        );
        assert_eq!(tables[2].entries[2].value, Value::Number(1000.0));
        assert_eq!(tables[2].entries[3], Entry { key: "quote".into(), value: text("say \"hi\"\n"), line: 13 });
        assert_eq!(parse("").expect("empty"), [Table { name: String::new(), line: 0, entries: Vec::new() }]);
    }

    /// Everything outside the subset, and everything malformed, is refused with its line number.
    #[test]
    fn refuses_what_is_outside_the_subset() {
        for (src, line) in [
            ("a = 1\na = 2\n", "line 2"),
            ("[t]\n[u]\n[t]\n", "line 3"),
            ("[[t]]\n", "line 1"),
            ("[a.b]\n", "line 1"),
            ("[t\n", "line 1"),
            ("[t] x\n", "line 1"),
            ("a.b = 1\n", "line 1"),
            ("\"a\" = 1\n", "line 1"),
            ("just words\n", "line 1"),
            ("a =\n", "line 1"),
            ("a = 1 2\n", "line 1"),
            ("a = \"open\n", "line 1"),
            ("a = 'open\n", "line 1"),
            ("a = \"\"\"multi\n", "line 1"),
            ("a = \"bad \\q\"\n", "line 1"),
            ("\n\na = [1, 2\n", "line 3"),
            ("a = [1 2]\n", "line 1"),
            ("a = { b = 1 }\n", "line 1"),
            ("a = inf\n", "line 1"),
            ("a = nan\n", "line 1"),
            ("a = 0x10\n", "line 1"),
            ("a = .5\n", "line 1"),
            ("a = 1e\n", "line 1"),
            ("a = 1__0\n", "line 1"),
            ("a = 1_.5\n", "line 1"),
            ("a = 1979-05-27\n", "line 1"),
            ("a = yes\n", "line 1"),
        ] {
            let e = parse(src).expect_err(src);
            assert!(e.starts_with(line), "{src:?}: {e}");
        }
    }
}

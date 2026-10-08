//! Names for every sound event, from the shipped CSV sources.
//!
//! The packed `sound_events` file stores most events without a name (only the six
//! [named categories](super::events::NAMED_CATEGORIES) keep one). The sources it was
//! built from ship in the same pack: `sounds\events\sound_events_*.csv`, one row per
//! event. Their rows, read file by file in name order, line up one to one with the
//! packed events (INFERRED; checked row by row: same category and same sound files, see
//! AUDIO_FORMAT.md §3.4). [`EventNames::from_csvs`] only accepts a row whose category and
//! files match the packed event at that position, so a changed or missing CSV can never
//! give an event a wrong name: it just leaves it unnamed.
//!
//! Names are compared ignoring case: the exe's own built-in names (see [`super::slots`])
//! are upper-case versions of the same CSV names.

use std::collections::HashMap;

use super::events::SoundEvents;

/// The folder of the CSV sources.
pub const CSV_DIR: &str = r"sounds\events";

/// Parses one CSV line: comma-separated, fields optionally in double quotes with `""`
/// for a literal quote. Fields are trimmed.
pub fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur).trim().to_owned()),
            _ => cur.push(ch),
        }
    }
    out.push(cur.trim().to_owned());
    out
}

/// A sound file path in the form used for comparisons: lower case, `/` -> `\`, runs of `\`
/// collapsed to one ([`normalized_sound_path_bytes`] collected).
pub fn normalize_sound_path(p: &str) -> String {
    // Only ASCII bytes are changed or dropped, so the result is still UTF-8.
    String::from_utf8(normalized_sound_path_bytes(p).collect()).expect("ASCII edits keep UTF-8")
}

/// The bytes of [`normalize_sound_path`]`(p)`, one at a time without allocating (it collects them).
pub fn normalized_sound_path_bytes(p: &str) -> impl Iterator<Item = u8> + '_ {
    let mut after_sep = false;
    p.trim().bytes().map(|b| if b == b'/' { b'\\' } else { b.to_ascii_lowercase() }).filter(move |&b| {
        let sep = b == b'\\';
        let keep = !(sep && after_sep);
        after_sep = sep;
        keep
    })
}

/// One CSV row: name, category and sound files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvEventRow {
    pub name: String,
    pub category: String,
    pub files: Vec<String>,
}

/// Index of the first `wavefile_*` column (after name, category and the 35 parameters).
pub const CSV_FIRST_FILE_COLUMN: usize = 37;

/// Parses a `sound_events_*.csv` file (header line, then one row per event).
pub fn parse_events_csv(text: &str) -> Vec<CsvEventRow> {
    text.lines()
        .skip(1)
        .map(split_csv_line)
        .filter(|f| f.first().is_some_and(|n| !n.is_empty()))
        .map(|f| CsvEventRow {
            name: f[0].clone(),
            category: f.get(1).cloned().unwrap_or_default(),
            files: f.iter().skip(CSV_FIRST_FILE_COLUMN).filter(|s| !s.is_empty()).cloned().collect(),
        })
        .collect()
}

/// Event names, by event index, plus a name -> events lookup.
#[derive(Debug, Clone, Default)]
pub struct EventNames {
    /// `names[i]` = the name of event `i`, if known.
    pub names: Vec<Option<String>>,
    by_name: HashMap<String, Vec<usize>>,
    /// Number of CSV rows (unnamed categories) that matched no packed event.
    pub unmatched_rows: usize,
}

impl EventNames {
    /// Names from the packed file alone (named categories only).
    pub fn from_packed(events: &SoundEvents) -> Self {
        let names = events.events.iter().map(|e| e.name.clone()).collect();
        Self::finish(names, 0)
    }

    /// Names from the CSV sources `csvs` (`(file name, text)`), matched to `events` by content.
    ///
    /// The developer build that packed the CSVs reordered the rows (by an order that is
    /// UNKNOWN), so rows are matched by content instead of position: a row names the packed
    /// event with the same category and the same sound files. When several events share
    /// both (e.g. silent placeholders), the rows and events of that group are paired in
    /// file order (INFERRED). Events in named categories keep their packed name.
    pub fn from_csvs(events: &SoundEvents, csvs: &[(String, String)]) -> Self {
        let mut files: Vec<&(String, String)> = csvs.iter().collect();
        files.sort_by_key(|(n, _)| n.to_ascii_lowercase());
        let key = |cat: &str, f: &mut dyn Iterator<Item = &String>| {
            let mut k = cat.to_ascii_lowercase();
            for p in f {
                k.push('|');
                k.push_str(&normalize_sound_path(p));
            }
            k
        };
        // Unnamed packed events grouped by content, in file order.
        let mut groups: HashMap<String, std::collections::VecDeque<usize>> = HashMap::new();
        for (i, e) in events.events.iter().enumerate() {
            if e.name.is_none() {
                groups.entry(key(events.category_name(e), &mut e.files.iter())).or_default().push_back(i);
            }
        }
        let mut names: Vec<Option<String>> = events.events.iter().map(|e| e.name.clone()).collect();
        let mut unmatched = 0usize;
        for (_, text) in files {
            for row in parse_events_csv(text) {
                if super::events::is_named_category(&row.category) {
                    continue;
                }
                match groups.get_mut(&key(&row.category, &mut row.files.iter())).and_then(|g| g.pop_front()) {
                    Some(i) => names[i] = Some(row.name),
                    None => unmatched += 1,
                }
            }
        }
        Self::finish(names, unmatched)
    }
    fn finish(names: Vec<Option<String>>, unmatched_rows: usize) -> Self {
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, n) in names.iter().enumerate() {
            if let Some(n) = n {
                by_name.entry(n.to_ascii_lowercase()).or_default().push(i);
            }
        }
        Self { names, by_name, unmatched_rows }
    }

    /// Events with this name (case-insensitive), in file order.
    pub fn find(&self, name: &str) -> &[usize] {
        self.by_name.get(&name.to_ascii_lowercase()).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The name of an event.
    pub fn name(&self, event: usize) -> Option<&str> {
        self.names.get(event)?.as_deref()
    }

    /// Number of named events.
    pub fn named_count(&self) -> usize {
        self.names.iter().filter(|n| n.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes() {
        assert_eq!(split_csv_line(r#""a",b, "c,d" ,"e""f""#), vec!["a", "b", "c,d", "e\"f"]);
        assert_eq!(split_csv_line("x,,y"), vec!["x", "", "y"]);
    }

    #[test]
    fn rows() {
        let mut header = String::from("name,category");
        for _ in 0..35 {
            header.push_str(",p");
        }
        let mut row = String::from("Music_frontend,music");
        for _ in 0..35 {
            row.push_str(",0");
        }
        row.push_str(r",Front_End_Music\NTW_MUS01.mp3,,,");
        let rows = parse_events_csv(&format!("{header}\n{row}\n,,,\n"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].files, vec![r"Front_End_Music\NTW_MUS01.mp3".to_string()]);
        assert_eq!(normalize_sound_path("A/B\\C.WAV"), r"a\b\c.wav");
        assert_eq!(normalize_sound_path(" Mus//X\\/\\\\Ü.MP3 \t"), "mus\\x\\Ü.mp3");
        assert_eq!(normalize_sound_path(r"\\a"), r"\a");
    }
}

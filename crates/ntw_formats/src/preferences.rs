//! `preferences.script.txt`: the original's settings file.
//!
//! The original keeps its settings in `%APPDATA%\The Creative Assembly\Napoleon\scripts\
//! preferences.script.txt` (write class `.script.txt` + folder `scripts\`, CONFIRMED strings in the
//! exe's write-class tables). Layout (CONFIRMED from the player's own file):
//! - UTF-16LE with a byte-order mark (`FF FE`), lines ending in `\r\n`;
//! - one setting per line: `<key> <value>; # <help text> #`, e.g.
//!   `gfx_vsync false; # gfx_vsync <bool>, vertical synchronization #`;
//! - values are bools (`true`/`false`), integers, floats, or free text (`gfx_screenshot_folder
//!   ./screenshots;`, possibly empty: `local_player_name ;`).
//!
//! [`Preferences`] keeps every line (unknown keys and the help text too) so a file written back
//! differs only in the values that were changed. NapoleonRust never writes the player's original
//! file: the game keeps its own copy (see `napoleon::config::user_dir`).

/// One `key value; # help #` line (or a line we do not understand, kept as is).
#[derive(Debug, Clone, PartialEq)]
pub enum PrefLine {
    /// A setting.
    Setting {
        /// The key, e.g. `gfx_vsync`.
        key: String,
        /// The value text exactly as stored (without the `;`).
        value: String,
        /// Everything after the `;` (the help comment), kept verbatim.
        tail: String,
    },
    /// Any other line (blank, comment-only, ...), kept verbatim.
    Other(String),
}

/// A whole preferences file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preferences {
    /// The lines in file order.
    pub lines: Vec<PrefLine>,
}

impl Preferences {
    /// Parses the file bytes (UTF-16LE with BOM; UTF-8 is accepted too).
    pub fn read(bytes: &[u8]) -> Self {
        Self::parse(&decode_text(bytes))
    }

    /// Parses the text of the file.
    pub fn parse(text: &str) -> Self {
        let lines = text
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l))
            .filter(|l| !l.is_empty())
            .map(parse_line)
            .collect();
        Self { lines }
    }

    /// The file text (lines joined with `\r\n`, final newline included).
    pub fn to_text(&self) -> String {
        let mut s = String::new();
        for l in &self.lines {
            match l {
                PrefLine::Setting { key, value, tail } => {
                    s.push_str(key);
                    s.push(' ');
                    s.push_str(value);
                    s.push(';');
                    s.push_str(tail);
                }
                PrefLine::Other(t) => s.push_str(t),
            }
            s.push_str("\r\n");
        }
        s
    }

    /// The file bytes as the original writes them: UTF-16LE with a byte-order mark.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = vec![0xFF, 0xFE];
        for u in self.to_text().encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        b
    }

    /// The value text of the first line with this key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines.iter().find_map(|l| match l {
            PrefLine::Setting { key: k, value, .. } if k == key => Some(value.as_str()),
            _ => None,
        })
    }

    /// A bool setting (`true`/`false`, or a number: non-zero is true).
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        let v = self.get(key)?.trim();
        match v {
            "true" => Some(true),
            "false" => Some(false),
            _ => v.parse::<f64>().ok().map(|n| n != 0.0),
        }
    }

    /// A numeric setting.
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get(key)?.trim().parse().ok()
    }

    /// Sets a value. Every line with this key is changed (the original's own file repeats a few
    /// keys, e.g. `gfx_hardware_shadows`); a missing key is appended with an empty comment.
    pub fn set(&mut self, key: &str, value: &str) {
        let mut found = false;
        for l in &mut self.lines {
            if let PrefLine::Setting { key: k, value: v, .. } = l
                && k == key
            {
                *v = value.to_owned();
                found = true;
            }
        }
        if !found {
            self.lines.push(PrefLine::Setting { key: key.to_owned(), value: value.to_owned(), tail: " # #".into() });
        }
    }

    /// Sets a bool as `true`/`false`.
    pub fn set_bool(&mut self, key: &str, v: bool) {
        self.set(key, if v { "true" } else { "false" });
    }

    /// Sets a number; whole numbers are written without a fraction (`100`, `1.2`).
    pub fn set_f64(&mut self, key: &str, v: f64) {
        let text = if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") };
        self.set(key, &text);
    }
}

fn parse_line(line: &str) -> PrefLine {
    let Some(semi) = line.find(';') else { return PrefLine::Other(line.to_owned()) };
    let (head, tail) = (&line[..semi], &line[semi + 1..]);
    let head = head.trim_start();
    let (key, value) = match head.find(' ') {
        Some(i) => (&head[..i], &head[i + 1..]),
        None => (head, ""),
    };
    if key.is_empty() || key.starts_with('#') {
        return PrefLine::Other(line.to_owned());
    }
    PrefLine::Setting { key: key.to_owned(), value: value.to_owned(), tail: tail.to_owned() }
}

/// UTF-16LE (with or without BOM) or UTF-8 bytes to text.
fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&units);
    }
    // No BOM: UTF-16LE text has a zero high byte in its second byte for ASCII keys.
    if bytes.len() >= 2 && bytes[1] == 0 && bytes[0] != 0 {
        let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "write_preferences_at_exit true; # write_preferences_at_exit <bool>, Write preferences at exit #\r\n\
gfx_brightness_setting 1.2; # gfx_brightness_setting <float>, Set brightness #\r\n\
local_player_name ; # local_player_name <name>, Name #\r\n\
gfx_hardware_shadows true; # a #\r\n\
gfx_hardware_shadows true; # b #\r\n";

    #[test]
    fn round_trips_utf16_with_bom() {
        let p = Preferences::parse(SAMPLE);
        assert_eq!(p.to_text(), SAMPLE);
        let bytes = p.to_bytes();
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
        assert_eq!(Preferences::read(&bytes), p);
    }

    #[test]
    fn reads_and_sets_values() {
        let mut p = Preferences::parse(SAMPLE);
        assert_eq!(p.get_bool("write_preferences_at_exit"), Some(true));
        assert_eq!(p.get_f64("gfx_brightness_setting"), Some(1.2));
        assert_eq!(p.get("local_player_name"), Some(""));
        p.set_bool("gfx_hardware_shadows", false);
        p.set_f64("gfx_brightness_setting", 1.0);
        p.set_f64("sound_music_volume", 16.0);
        let t = p.to_text();
        assert!(t.contains("gfx_hardware_shadows false; # a #\r\ngfx_hardware_shadows false; # b #"));
        assert!(t.contains("gfx_brightness_setting 1; # gfx_brightness_setting"));
        assert!(t.ends_with("sound_music_volume 16; # #\r\n"));
    }
}

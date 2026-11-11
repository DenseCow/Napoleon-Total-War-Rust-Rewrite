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
//!
//! A setting is read with [`Preferences::get`] (or its typed reads): the file's value, else the
//! default the game registers the setting with ([`registered_default`]).

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

    /// The setting's value text: the file's (its first line with this key), else the default the
    /// game registers the setting with ([`registered_default`]). This is the one reading of a
    /// setting: the option pages (`__prefs.get`), the battle setup, the campaign, audio and the
    /// renderer all come through here or the two typed reads below, so a setting no file gives is
    /// the same value for all of them. `None`: no line and no registered default (a text setting,
    /// or a key the game does not have).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.file_value(key).or_else(|| registered_default(key))
    }

    /// A bool setting (`true` / `false`); see [`Self::get`].
    ///
    /// A file value that does not read as a bool is passed over for the registered default.
    /// CONFIRMED: the exe's bool setting reads its value with a stream read (`0x0044ABA0`) that
    /// matches the two names `false` / `true` (`0x010E0180`, name table `0x01402BB4`) and stores
    /// nothing when neither matches, so the setting keeps what it was built with. The same holds
    /// for the int and float settings ([`Self::get_f64`]). [`Self::mistyped`] lists such lines.
    /// INFERRED: the stream is in its names mode (the file the exe writes spells the two words);
    /// in the other mode the same read takes `0` / `1`, and where the mode is set was not traced.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.file_value(key).and_then(bool_value).or_else(|| registered_default(key).and_then(bool_value))
    }

    /// A numeric setting; see [`Self::get`].
    ///
    /// A file value that does not read as a number is passed over for the registered default, as
    /// in [`Self::get_bool`]. CONFIRMED: the exe's int (`0x0044C500`) and float (`0x0044D300`)
    /// settings store the stream read's result only when the read did not fail (`0x010D3780`,
    /// `0x010E00B0`). The read takes the number the text starts with (`3abc` is 3) and knows only
    /// a sign, digits, a point and an exponent (`0x010E3C80`), so `nan` and `inf` are no numbers.
    /// A float setting's number beyond a float's range is stored as infinity (`number_value`).
    ///
    /// Difference: an int setting's read stops at a point in the exe (`2.5` is 2); here the
    /// fraction is kept and the readers of whole-number settings drop it. Ours reads every
    /// numeric setting as the float read does. The exe's int read (`0x010DFF00`) gathers the
    /// digit field (`0x010E4460`: a sign, then decimal digits, leading zeros dropped, at most 30
    /// kept and the rest read and lost) and converts it with `sscanf("%i")` (`0x010F5F50` →
    /// `0x0044AAC0`, the UCRT's `__stdio_common_vsscanf`); it fails only when no digit was read or
    /// sscanf converted nothing (CONFIRMED). So a number beyond an int's range does not fail. The
    /// statically linked UCRT's `%i` parses a signed 64-bit number (`0x0127C18E` with base 0,
    /// signed, → `0x01277818`), saturating beyond that range (`0x012753BF`: to i64::MAX, or
    /// i64::MIN for a negative), and stores its low 32 bits into the int (`0x0127D5C1`)
    /// (CONFIRMED). So `2147483648` stores -2147483648, `4294967296` stores 0, and a number beyond
    /// i64's range stores -1 (0 when negative). Difference: ours keeps the number.
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.file_value(key).and_then(number_value).or_else(|| registered_default(key).and_then(number_value))
    }

    /// The keys of the file's lines whose value does not read as the setting's type (the type of
    /// its registered default: a bool or a number), each once, in file order. The typed reads pass
    /// such a line over; whoever loads a file says so once with this list.
    pub fn mistyped(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = Vec::new();
        for l in &self.lines {
            let PrefLine::Setting { key, value, .. } = l else { continue };
            let Some(default) = registered_default(key) else { continue };
            let reads = if bool_value(default).is_some() { bool_value(value).is_some() } else { number_value(value).is_some() };
            if !reads && !keys.contains(&key.as_str()) {
                keys.push(key);
            }
        }
        keys
    }

    /// The value text of the file's first line with this key (no default).
    fn file_value(&self, key: &str) -> Option<&str> {
        self.lines.iter().find_map(|l| match l {
            PrefLine::Setting { key: k, value, .. } if k == key => Some(value.as_str()),
            _ => None,
        })
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

/// The value the exe gives a setting before it reads any file, as the file would spell it.
///
/// CONFIRMED: every setting is an "environment variable" object the exe builds in a static
/// initialiser (one small function per setting, at the address on its line below; constructors
/// `0x00454300` int, `0x00454380` float, `0x00454480` bool) from the key and its default. The keys are
/// the constructors' own key argument, which is not always the name in the usage text: the usage of
/// `limitless_ammo` names `show_cpu_moves`, that of `default_camera_type` names
/// `default_battle_camera`, and those of `gfx_volumetric_effect`, `invert_cam_x_axis`,
/// `invert_cam_y_axis` and `occupiable_building_markers` are spelled differently too.
///
/// A file line replaces the value, so this is what a setting is on an install whose original never
/// wrote `preferences.script.txt`, and for a key an older file lacks. Not listed (UNKNOWN): the five
/// text settings (`0x00454400`: `gfx_fixed_env_map`, `gfx_screenshot_folder`, `local_player_name`,
/// `packed_sounds_output_path`, `sound_raw_data_path`), and whatever the first run changes afterwards
/// (`gfx_first_run` starts true; `x_res` / `y_res` start at 0, which is no screen mode).
///
/// Readers take a setting through [`Preferences::get`] and its typed reads, never from here and
/// never with a default of their own: the list below is the original game's data, not an engine
/// rule, and this function is the one place another game's or a mod's settings list replaces it.
pub fn registered_default(key: &str) -> Option<&'static str> {
    REGISTERED_DEFAULTS.binary_search_by(|(k, _)| (*k).cmp(key)).ok().map(|i| REGISTERED_DEFAULTS[i].1)
}

/// See [`registered_default`]. In key order (it searches by it; the test checks the order).
const REGISTERED_DEFAULTS: &[(&str, &str)] = &[
    ("allow_drop_in_battles", "true"),                      // 0x00402C60
    ("app_multirun", "false"),                              // 0x00402D40
    ("audio_memory_size_in_megabytes", "100"),              // 0x00402D90
    ("automanage_regions", "false"),                        // 0x00402DE0
    ("autoresolve_difficulty", "0"),                        // 0x00402E30
    ("battle_advice_level", "2"),                           // 0x00402E80
    ("battle_camera_shake_enabled", "true"),                // 0x00402ED0
    ("battle_difficulty", "0"),                             // 0x00402F20
    ("battle_time_limit", "60"),                            // 0x00402F70
    ("bink_use_thread", "true"),                            // 0x00403050
    ("build_audio_mod_data", "false"),                      // 0x004030A0
    ("build_sound_packs", "false"),                         // 0x004030F0
    ("camera_move_speed", "100"),                           // 0x00403140
    ("camera_turn_speed", "5"),                             // 0x00403190
    ("campaign_advice_level", "2"),                         // 0x004031E0
    ("campaign_difficulty", "0"),                           // 0x00403230
    ("campaign_time_limit", "-1"),                          // 0x00403280
    ("campaign_unit_multiplier", "0.75"),                   // 0x004032D0
    ("default_camera_type", "0"),                           // 0x00403450
    ("enable_sound", "true"),                               // 0x004035B0
    ("fix_res", "true"),                                    // 0x00403660
    ("fix_window_pos", "true"),                             // 0x004036B0
    ("friend_only_drop_in_battles", "false"),               // 0x00403820
    ("generate_audio_pack_file", "false"),                  // 0x00403900
    ("gfx_aa", "0"),                                        // 0x00403950
    ("gfx_brightness_setting", "1.2"),                      // 0x004039A0
    ("gfx_building_quality", "2"),                          // 0x004039F0
    ("gfx_depth_of_field", "false"),                        // 0x00403A40
    ("gfx_disable_terrain_ao_gpu", "false"),                // 0x00403A90
    ("gfx_distortion", "false"),                            // 0x00403AE0
    ("gfx_effects_quality", "2"),                           // 0x00403B30
    ("gfx_first_run", "true"),                              // 0x00403B80
    ("gfx_frame_rate", "60"),                               // 0x00403C30
    ("gfx_fullscreen", "true"),                             // 0x00403C80
    ("gfx_gamma_setting", "2"),                             // 0x00403CD0
    ("gfx_grass_quality", "2"),                             // 0x00403D20
    ("gfx_hardware_shadows", "false"),                      // 0x00403D70
    ("gfx_hdr", "true"),                                    // 0x00403DC0
    ("gfx_low_quality_shaders", "false"),                   // 0x00403E10
    ("gfx_picture_in_picture", "true"),                     // 0x00403E60
    ("gfx_shadermodel", "1"),                               // 0x00403F10
    ("gfx_shadow_quality", "2"),                            // 0x00403F60
    ("gfx_ship_quality", "2"),                              // 0x00403FB0
    ("gfx_sky_quality", "2"),                               // 0x00404000
    ("gfx_ssao", "false"),                                  // 0x00404050
    ("gfx_terrain_quality", "0"),                           // 0x004040A0
    ("gfx_texture_filtering", "1"),                         // 0x004040F0
    ("gfx_texture_quality", "2"),                           // 0x00404140
    ("gfx_tree_quality", "2"),                              // 0x00404190
    ("gfx_unit_quality", "2"),                              // 0x004041E0
    ("gfx_unit_scale", "2"),                                // 0x00404230
    ("gfx_video_memory", "0"),                              // 0x00404280
    ("gfx_vignette", "true"),                               // 0x004042D0
    ("gfx_volumetric_effect", "false"),                     // 0x00404320
    ("gfx_vsync", "false"),                                 // 0x00404370
    ("gfx_water_quality", "2"),                             // 0x004043C0
    ("invert_cam_x_axis", "false"),                         // 0x004044A0
    ("invert_cam_y_axis", "false"),                         // 0x004044F0
    ("limitless_ammo", "true"),                             // 0x00404540
    ("minimised_ui", "false"),                              // 0x004045F0
    ("mouse_wheel_sensitivity", "50"),                      // 0x00404660
    ("multithreaded_model_enabled", "true"),                // 0x004046D0
    ("number_of_threads", "0"),                             // 0x00404780
    ("occupiable_building_markers", "true"),                // 0x004047D0
    ("override_autoresolve_handicap", "false"),             // 0x00404820
    ("show_cpu_moves", "true"),                             // 0x00404D30
    ("show_path_markers", "true"),                          // 0x00404D80
    ("show_selection_markers", "true"),                     // 0x00404DD0
    ("show_target_zones", "true"),                          // 0x00404E20
    ("sound_channels", "128"),                              // 0x00404E70
    ("sound_debug", "false"),                               // 0x00404EC0
    ("sound_file_caching", "false"),                        // 0x00404F10
    ("sound_master_enabled", "true"),                       // 0x00404F60
    ("sound_master_volume", "100"),                         // 0x00404FB0
    ("sound_max_files_to_read_each_frame", "256"),          // 0x00405000
    ("sound_multithreading", "false"),                      // 0x00405050
    ("sound_music_enabled", "true"),                        // 0x004050A0
    ("sound_music_volume", "100"),                          // 0x004050F0
    ("sound_provider", "0"),                                // 0x00405140
    ("sound_sfx_enabled", "true"),                          // 0x004051F0
    ("sound_sfx_volume", "100"),                            // 0x00405240
    ("sound_speech_enabled", "true"),                       // 0x00405290
    ("sound_speech_volume", "100"),                         // 0x004052E0
    ("sound_variations", "2"),                              // 0x00405330
    ("subtitles", "false"),                                 // 0x004054E0
    ("ui_card_mode", "0"),                                  // 0x004055C0
    ("ui_land_unit_ids", "0"),                              // 0x00405630
    ("ui_mouse_scroll", "true"),                            // 0x00405680
    ("ui_naval_unit_ids", "0"),                             // 0x004056D0
    ("ui_order_button_mode", "0"),                          // 0x00405720
    ("ui_radar_mode", "0"),                                 // 0x00405770
    ("ui_show_campaign_labels", "true"),                    // 0x004057C0
    ("unit_test", "false"),                                 // 0x00405810
    ("vfs_log_level", "0"),                                 // 0x00405860
    ("voice_chat_enable", "true"),                          // 0x004058B0
    ("voice_chat_microphone_gain", "100"),                  // 0x00405900
    ("voice_chat_microphone_gain_boost", "true"),           // 0x00405950
    ("voice_chat_quality", "2"),                            // 0x004059A0
    ("voice_chat_transmit_only_when_key_pressed", "true"),  // 0x004059F0
    ("voice_chat_volume", "100"),                           // 0x00405A40
    ("write_preferences_at_exit", "true"),                  // 0x00405A90
    ("x_pos", "0"),                                         // 0x00405AE0
    ("x_res", "0"),                                         // 0x00405B30
    ("y_pos", "0"),                                         // 0x00405B80
    ("y_res", "0"),                                         // 0x00405BD0
];

/// A value text as a bool: `true` or `false`, nothing else (see [`Preferences::get_bool`]).
fn bool_value(text: &str) -> Option<bool> {
    match text.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// A value text as a number: the number the text starts with, as the exe's stream read collects
/// it (a sign, digits, a point, digits, then an exponent when digits follow it; `0x010E3C80`).
/// Text with no digit is no number. A number too large for a float is infinite, with its sign.
///
/// CONFIRMED, the float read: `0x010E00B0` turns the collected text into a float with `0x010F5E50`,
/// a `sscanf` "%f" into a float. Its float specifier (`0x01278132`) fails only when there is no
/// digit; when the number overflows a float the result writer (`0x012772CA`) stores infinity
/// with the sign, and the read stores that. The overflow test is the float's range, not
/// the f64's: `3.5e38` is infinite too. Never NaN: the collector passes no `nan` / `inf` text.
/// Difference: the collector keeps 36 significant digits (a whole digit past them is made up
/// for by multiplying the float by 10, a fraction digit is dropped) and, in a number below 1,
/// one of the zeros right after the point (each other one is made up for by multiplying by
/// 0.1). Such a text can come out a last bit away from ours. A nonzero number too small for a
/// float is 0 in the exe and kept as it is here.
fn number_value(text: &str) -> Option<f64> {
    let text = text.trim_start();
    let b = text.as_bytes();
    let digits = |from: usize| b.get(from..).map_or(0, |rest| rest.iter().take_while(|c| c.is_ascii_digit()).count());
    let mut end = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let whole = digits(end);
    end += whole;
    let mut fraction = 0;
    if b.get(end) == Some(&b'.') {
        fraction = digits(end + 1);
        end += 1 + fraction;
    }
    if whole + fraction == 0 {
        return None;
    }
    if matches!(b.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(b.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits(end + 1 + sign);
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    let number = &text[..end];
    // The overflow test is the float's own parse, so the rounding at its edge is the exe's.
    if number.parse::<f32>().ok()?.is_infinite() {
        return Some(if number.starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY });
    }
    number.parse::<f64>().ok()
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

    /// The exe's own defaults stand in for keys no file gives; a key it does not build (or builds
    /// as text) has none, and the table has each key once (its lookup takes the first).
    #[test]
    fn registered_defaults_are_the_exes_and_unique() {
        assert_eq!(registered_default("camera_move_speed"), Some("100"));
        assert_eq!(registered_default("camera_turn_speed"), Some("5"));
        assert_eq!(registered_default("campaign_difficulty"), Some("0"));
        assert_eq!(registered_default("limitless_ammo"), Some("true"));
        assert_eq!(registered_default("default_camera_type"), Some("0"));
        assert_eq!(registered_default("local_player_name"), None);
        assert_eq!(registered_default("default_battle_camera"), None);
        assert_eq!(REGISTERED_DEFAULTS.len(), 105);
        assert!(REGISTERED_DEFAULTS.windows(2).all(|w| w[0].0 < w[1].0), "sorted, so no key twice");
        // Every key is found by the search, and every default is a bool or a number that reads
        // the same from a file line as from the table.
        let none = Preferences::default();
        for &(key, value) in REGISTERED_DEFAULTS {
            assert_eq!(registered_default(key), Some(value), "{key}");
            let file = Preferences::parse(&format!("{key} {value}; # #\r\n"));
            match value {
                "true" | "false" => {
                    assert_eq!(file.get_bool(key), Some(value == "true"), "{key} {value}");
                    assert_eq!(none.get_bool(key), Some(value == "true"), "{key} {value}");
                }
                _ => {
                    let number: f64 = value.parse().unwrap_or_else(|_| panic!("{key} {value}: neither a bool nor a number"));
                    assert_eq!(file.get_f64(key), Some(number), "{key} {value}");
                    assert_eq!(none.get_f64(key), Some(number), "{key} {value}");
                }
            }
        }
    }

    /// One reading of a setting for every user: the file's value, else the registered default; a
    /// key with neither is `None`. Before, each reader of a missing key took a default of its own
    /// (the battle setup unit size 3 and the trees 3 against the registered 2 the options page showed).
    #[test]
    fn a_setting_reads_as_the_files_value_else_the_registered_default() {
        let none = Preferences::default();
        assert_eq!(none.get("gfx_unit_scale"), registered_default("gfx_unit_scale"));
        assert_eq!(none.get_f64("gfx_unit_scale"), Some(2.0));
        assert_eq!(none.get_f64("gfx_tree_quality"), Some(2.0));
        assert_eq!(none.get_f64("campaign_unit_multiplier"), Some(0.75));
        assert_eq!(none.get_bool("ui_show_campaign_labels"), Some(true));
        assert_eq!((none.get_bool("enable_sound"), none.get_f64("sound_master_volume"), none.get_f64("sound_provider")), (Some(true), Some(100.0), Some(0.0)));
        assert_eq!((none.get("local_player_name"), none.get("no_such_setting")), (None, None));
        assert_eq!((none.get_bool("no_such_setting"), none.get_f64("no_such_setting")), (None, None));

        let file = Preferences::parse("gfx_unit_scale 0; # #\r\nui_show_campaign_labels false; # #\r\nlocal_player_name ; # #\r\n");
        assert_eq!((file.get("gfx_unit_scale"), file.get_f64("gfx_unit_scale")), (Some("0"), Some(0.0)));
        assert_eq!(file.get_bool("ui_show_campaign_labels"), Some(false));
        assert_eq!(file.get("local_player_name"), Some(""));
        assert_eq!(file.get_f64("gfx_tree_quality"), Some(2.0), "a key this file lacks");
        // A set value is a file value. Reading a default writes nothing into the file.
        let mut set = Preferences::default();
        assert_eq!(set.get("gfx_tree_quality"), Some("2"));
        assert!(set.lines.is_empty());
        set.set_f64("gfx_tree_quality", 3.0);
        assert_eq!(set.get_f64("gfx_tree_quality"), Some(3.0));

        // A value that does not read as the type is passed over by the typed reads, as the exe's
        // stream reads leave the setting alone; the text read still gives the file's text.
        let odd = Preferences::parse("gfx_unit_scale ; # #\r\nenable_sound maybe; # #\r\n");
        assert_eq!((odd.get("gfx_unit_scale"), odd.get_f64("gfx_unit_scale")), (Some(""), Some(2.0)));
        assert_eq!(odd.get_bool("enable_sound"), Some(true));
    }

    /// A number is what the text starts with: `nan` and `inf` text used to count as numbers and
    /// reached the sliders and the audio volumes.
    #[test]
    fn a_number_is_the_leading_number_and_never_nan() {
        let read = |value: &str| Preferences::parse(&format!("sound_master_volume {value}; # #\r\n")).get_f64("sound_master_volume");
        for (text, number) in [("40", 40.0), (" 40 ", 40.0), ("-1.5", -1.5), ("+.5", 0.5), ("3.", 3.0), ("1e2", 100.0), ("2E-1", 0.2), ("3abc", 3.0), ("7e", 7.0), ("7e+", 7.0)] {
            assert_eq!(read(text), Some(number), "{text:?}");
        }
        // Not numbers: the registered default (100) stands.
        for text in ["nan", "NaN", "inf", "-inf", "infinity", "", ".", "-", "e5", "abc", "true"] {
            assert_eq!(read(text), Some(100.0), "{text:?}");
        }
        // A key with no registered default has nothing to fall back to.
        assert_eq!(Preferences::parse("no_such_setting nan; # #\r\n").get_f64("no_such_setting"), None);
    }

    /// A number beyond a float's range is infinite with its sign, as the exe's float read stores
    /// it (`0x01278132` fails only on no digit, `0x012772CA` writes the infinity); the range is
    /// the float's, not the f64's. It is a number, so not a mistyped line. Before, the doc said
    /// such a value was no number while `1e39` read as a finite f64.
    #[test]
    fn a_number_beyond_a_floats_range_is_infinite() {
        let key = "campaign_unit_multiplier";
        let read = |value: &str| Preferences::parse(&format!("{key} {value}; # #\r\n")).get_f64(key);
        for text in ["1e39", "3.5e38", "1e999", "+1e39abc", "1000000000000000000000000000000000000000"] {
            assert_eq!(read(text), Some(f64::INFINITY), "{text:?}");
        }
        assert_eq!((read("-1e39"), read("-1e999")), (Some(f64::NEG_INFINITY), Some(f64::NEG_INFINITY)));
        // Just inside the float's range stays finite.
        assert_eq!((read("3.4e38"), read("-3.4e38")), (Some(3.4e38), Some(-3.4e38)));
        assert!(Preferences::parse(&format!("{key} 1e39; # #\r\n")).mistyped().is_empty());
    }

    /// A bool is one of the two names; a number is not one (the exe matches the names only).
    #[test]
    fn a_bool_is_true_or_false_only() {
        let read = |value: &str| Preferences::parse(&format!("gfx_vsync {value}; # #\r\n")).get_bool("gfx_vsync");
        assert_eq!((read("true"), read("false"), read(" true ")), (Some(true), Some(false), Some(true)));
        // Anything else: the registered default (false).
        assert_eq!((read("1"), read("True"), read("yes"), read("")), (Some(false), Some(false), Some(false), Some(false)));
    }

    /// The lines the typed reads pass over are listed once each, so the loader can report them;
    /// a text setting and a key the game does not have are never listed.
    #[test]
    fn mistyped_lines_are_listed_once_each() {
        let p = Preferences::parse(
            "gfx_vsync maybe; # #\r\nsound_master_volume loud; # #\r\ngfx_vsync 1; # #\r\ngfx_aa 2; # #\r\nenable_sound true; # #\r\nlocal_player_name 12; # #\r\nno_such_setting x; # #\r\ngfx_unit_scale nan; # #\r\n",
        );
        assert_eq!(p.mistyped(), ["gfx_vsync", "sound_master_volume", "gfx_unit_scale"]);
        assert!(Preferences::parse(SAMPLE).mistyped().is_empty());
        assert!(Preferences::default().mistyped().is_empty());
    }
}

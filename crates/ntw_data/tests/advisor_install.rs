//! The battle advisor's two tables (`battle_script_strings`, `advisors`) against a real install
//! (read-only). `#[ignore]`d. Run with:
//! ```text
//! cargo test -p ntw_data --test advisor_install -- --ignored --nocapture
//! ```
//! See `analysis/fidelity/BATTLE_FIDELITY.md` §59.8.
use std::path::PathBuf;

use ntw_data::GameDatabase;

const DEFAULT_DATA_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn data_dir() -> PathBuf {
    std::env::var_os("NTW_DATA_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR))
}

/// The Austerlitz intro's first line is spoken by Napoleon, whose `advisors` row names his
/// portrait; every speaker a line names has a row.
#[test]
#[ignore]
fn advisor_tables_decode() {
    let db = GameDatabase::from_install(data_dir()).expect("database");
    assert_eq!(db.battle_script_strings.len(), 296);
    assert_eq!(db.advisors.len(), 9);
    let line = db.battle_script_strings.get("CreativeAssembly.HB_Austerlitz_Intro_1").expect("Austerlitz intro line");
    assert_eq!(line.speaker, "Napoleon");
    assert!(line.string.starts_with("December 2nd, 1805."), "{}", line.string);
    let napoleon = db.advisors.get("Napoleon").expect("Napoleon's row");
    assert_eq!(napoleon.icon_path, r"data\UI\Common UI\Skins\advisor_napoleon_grownup.tga");
    let missing: Vec<&str> =
        db.battle_script_strings.iter().map(|l| l.speaker.as_str()).filter(|s| !s.is_empty() && db.advisors.get(s).is_none()).collect();
    assert!(missing.is_empty(), "speakers without a portrait row: {missing:?}");
}

/// The advisor's speech is found where the game looks for it (`advisor\<key>.mp3`, `0x0100BC80`;
/// `napoleon::audio::start_advice` builds the same path): every Austerlitz intro line has its file.
#[test]
#[ignore]
fn advisor_speech_files_are_where_the_game_looks() {
    use ntw_formats::sound::names::normalize_sound_path;
    let vfs = ntw_formats::pack::Vfs::open_install(data_dir()).expect("install");
    for n in 1..=5 {
        let path = normalize_sound_path(&format!("advisor\\CreativeAssembly.HB_Austerlitz_Intro_{n}.mp3"));
        assert!(vfs.contains(&path), "{path}");
    }
    let db = GameDatabase::from_install(data_dir()).expect("database");
    let with_file = db.battle_script_strings.iter().filter(|l| vfs.contains(&normalize_sound_path(&format!("advisor\\{}.mp3", l.key)))).count();
    println!("{with_file} of {} lines have a speech file", db.battle_script_strings.len());
    assert!(with_file > 100, "{with_file}");
}

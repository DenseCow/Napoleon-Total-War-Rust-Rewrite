//! Research helper: the `factions` rows with their three UNKNOWN flags (#9..#11), category and
//! flag folders (custom battle faction lists, analysis/frontend/FRONTEND_PAGES.md). Read-only.
fn main() {
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data".into());
    let db = ntw_data::GameDatabase::from_install(&dir).expect("db");
    for f in db.factions.rows() {
        if f.unknown_64 || f.unknown_65 || f.unknown_66 {
            println!("{:28} {:16} {} {} {} flag={} rep={:?}", f.key, f.category, u8::from(f.unknown_64), u8::from(f.unknown_65), u8::from(f.unknown_66), f.flag_path, f.republic_flag_path);
        }
    }
}

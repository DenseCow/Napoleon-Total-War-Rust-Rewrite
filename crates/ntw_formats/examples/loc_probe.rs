//! Research helper: search the install's localisation (all `.loc` files through the VFS).
//! Read-only. Usage:
//!   cargo run -p ntw_formats --example loc_probe -- key <substring> [max]     (keys containing it)
//!   cargo run -p ntw_formats --example loc_probe -- text <substring> [max]    (texts containing it)
use ntw_formats::loc::Localisation;
use ntw_formats::pack::Vfs;

const DATA: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Napoleon Total War\data";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("NTW_DATA_DIR").unwrap_or_else(|_| DATA.into());
    let vfs = Vfs::open_install(&dir).unwrap();
    let loc = Localisation::from_vfs(&vfs).unwrap();
    let mode = args.first().map(String::as_str).unwrap_or("key");
    let needle = args.get(1).cloned().unwrap_or_default().to_lowercase();
    let max: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(40);
    let mut hits: Vec<(&str, &str)> = loc
        .iter()
        .filter(|(k, t)| match mode {
            "text" => t.to_lowercase().contains(&needle),
            _ => k.to_lowercase().contains(&needle),
        })
        .collect();
    hits.sort();
    println!("{} matches", hits.len());
    for (k, t) in hits.into_iter().take(max) {
        println!("{k} = {t:?}");
    }
}

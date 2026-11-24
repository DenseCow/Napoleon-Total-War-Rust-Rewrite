//! Research helper: searches the install's localisation (read-only).
//!   cargo run -p ntw_formats --example loc_find -- <substring of key or text> [max]
use ntw_formats::loc::Localisation;
use ntw_formats::pack::Vfs;


fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = ntw_formats::install::data_dir();
    let loc = Localisation::from_vfs(&Vfs::open_install(&dir).unwrap()).unwrap();
    let needle = args[0].to_lowercase();
    let max: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(40);
    for (k, v) in loc.iter().filter(|(k, v)| k.to_lowercase().contains(&needle) || v.to_lowercase().contains(&needle)).take(max) {
        println!("{k} = {}", v.chars().take(100).collect::<String>());
    }
}
